use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct FakeModel {
    requests: Mutex<Vec<Vec<Value>>>,
    active: AtomicUsize,
    peak: AtomicUsize,
}
#[async_trait]
impl Model for FakeModel {
    async fn complete(&self, messages: &[Value], _: &[Value]) -> Result<Value> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(active, Ordering::SeqCst);
        self.requests.lock().unwrap().push(messages.to_vec());
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        if messages.iter().any(|m| m["role"] == "tool") {
            Ok(json!({"role":"assistant","content":"observed"}))
        } else {
            Ok(
                json!({"role":"assistant","content":null,"tool_calls":[{"id":"capture-1","type":"function","function":{"name":"fake_image","arguments":"{}"}}]}),
            )
        }
    }
}
struct FakeHost;
#[async_trait]
impl ToolHost for FakeHost {
    fn tools(&self) -> Vec<Value> {
        vec![json!({"name":"fake_image","description":"test","inputSchema":{"type":"object"}})]
    }
    async fn call(&self, _: &str, _: &str, _: Value) -> Result<Value> {
        Ok(json!({"content":[{"type":"image","mimeType":"image/png","data":"aW1hZ2U="}]}))
    }
}
fn fake() -> Arc<FakeModel> {
    Arc::new(FakeModel {
        requests: Mutex::new(vec![]),
        active: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
    })
}

#[tokio::test]
async fn tool_images_reach_model_after_tool_pair_and_runs_are_serialized() {
    let dir = tempfile::tempdir().unwrap();
    let model = fake();
    let runtime = Runtime::open(dir.path(), model.clone(), Arc::new(FakeHost), true).unwrap();
    let a = runtime.start("look".into(), None, None).unwrap();
    let b = runtime.start("look too".into(), None, None).unwrap();
    assert_eq!(
        runtime.wait(&a.id, 5000).await.unwrap().status,
        Status::Completed
    );
    assert_eq!(
        runtime.wait(&b.id, 5000).await.unwrap().status,
        Status::Completed
    );
    assert_eq!(model.peak.load(Ordering::SeqCst), 1);
    let requests = model.requests.lock().unwrap();
    let request = requests.iter().find(|r| r.len() == 5).unwrap();
    assert_eq!(request[2]["role"], "assistant");
    assert_eq!(request[3]["role"], "tool");
    assert_eq!(request[4]["role"], "user");
    assert!(
        request[4]["content"][1]["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    assert!(!request[3]["content"].as_str().unwrap().contains("aW1hZ2U="));
}

#[tokio::test]
async fn permission_intersection_team_isolation_and_subtree_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Runtime::open(dir.path(), fake(), Arc::new(FakeHost), false).unwrap();
    let parent = runtime
        .start(
            "parent".into(),
            None,
            Some(BTreeSet::from(["agent_spawn".into()])),
        )
        .unwrap();
    let child = runtime
        .start(
            "child".into(),
            Some(parent.id.clone()),
            Some(BTreeSet::from(["fake_image".into(), "agent_spawn".into()])),
        )
        .unwrap();
    assert_eq!(child.permissions, BTreeSet::from(["agent_spawn".into()]));
    let other = runtime.start("other team".into(), None, None).unwrap();
    assert!(runtime.send(&other.id, &child.id, "hello".into()).is_err());
    runtime.send(&parent.id, &child.id, "hello".into()).unwrap();
    assert_eq!(runtime.status(&child.id).unwrap().mailbox.len(), 1);
    runtime.cancel(&parent.id).unwrap();
    assert_eq!(runtime.status(&child.id).unwrap().status, Status::Cancelled);
    assert_eq!(runtime.status(&other.id).unwrap().status, Status::Running);
    runtime.cancel(&other.id).unwrap();
}

#[tokio::test]
async fn restart_marks_inflight_runs_interrupted_without_replay() {
    let dir = tempfile::tempdir().unwrap();
    let run = Run {
        id: "crashed".into(),
        parent: None,
        root: "crashed".into(),
        depth: 0,
        status: Status::Running,
        permissions: BTreeSet::new(),
        output: None,
        mailbox: vec![],
    };
    let record = Record {
        event: Event {
            sequence: 1,
            run_id: run.id.clone(),
            kind: "tool_intent".into(),
        },
        run,
    };
    std::fs::write(
        dir.path().join("runs.jsonl"),
        format!("{}\n", serde_json::to_string(&record).unwrap()),
    )
    .unwrap();
    let model = fake();
    let runtime = Runtime::open(dir.path(), model.clone(), Arc::new(FakeHost), false).unwrap();
    assert_eq!(
        runtime.status("crashed").unwrap().status,
        Status::Interrupted
    );
    assert_eq!(runtime.events(1)[0].kind, "interrupted_on_restart");
    assert!(model.requests.lock().unwrap().is_empty());
    assert!(Runtime::open(dir.path(), fake(), Arc::new(FakeHost), false).is_err());
}

#[test]
fn images_are_withheld_without_vision_and_never_stringified() {
    let (text, images) = project_result(
        "c",
        json!({"content":[{"type":"image","mimeType":"image/png","data":"aW1hZ2U="}]}),
        false,
    )
    .unwrap();
    assert!(images.is_empty());
    assert!(!text.contains("aW1hZ2U="));
    assert!(text.contains("withheld"));
    assert!(
        project_result(
            "c",
            json!({"content":[{"type":"image","mimeType":"text/html","data":"aA=="}]}),
            true
        )
        .is_err()
    );
}

#[tokio::test]
async fn cancellation_still_signals_when_journal_is_full() {
    struct LifecycleHost(AtomicUsize);
    #[async_trait]
    impl ToolHost for LifecycleHost {
        fn tools(&self) -> Vec<Value> {
            vec![]
        }
        fn cancel_run(&self, _: &str) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn revoke(&self) {
            self.0.fetch_add(100, Ordering::SeqCst);
        }
        async fn call(&self, _: &str, _: &str, _: Value) -> Result<Value> {
            bail!("not available")
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let host = Arc::new(LifecycleHost(AtomicUsize::new(0)));
    let runtime = Runtime::open(dir.path(), fake(), host.clone(), false).unwrap();
    let run = runtime.start("cancel me".into(), None, None).unwrap();
    fill_journal(&runtime, 64 * 1024 * 1024 + 1);
    assert!(runtime.cancel(&run.id).is_err());
    assert_eq!(runtime.status(&run.id).unwrap().status, Status::Cancelled);
    assert!(*runtime.state.lock().unwrap().cancellation[&run.id].borrow());
    assert_eq!(host.0.load(Ordering::SeqCst), 1);
    runtime.shutdown().await;
    assert!(host.0.load(Ordering::SeqCst) >= 101);
}

#[tokio::test]
async fn model_cannot_cancel_sibling_or_increase_delegation_depth() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Runtime::open(dir.path(), fake(), Arc::new(FakeHost), false).unwrap();
    let parent = runtime.start("parent".into(), None, None).unwrap();
    let child = runtime
        .start("child".into(), Some(parent.id.clone()), None)
        .unwrap();
    let grandchild = runtime
        .start("grandchild".into(), Some(child.id.clone()), None)
        .unwrap();
    assert!(
        runtime
            .start("too deep".into(), Some(grandchild.id.clone()), None)
            .is_err()
    );
    let sibling = runtime
        .start("sibling".into(), Some(parent.id.clone()), None)
        .unwrap();
    assert!(
        runtime
            .invoke(&child.id, "agent_cancel", json!({"run_id":sibling.id}))
            .await
            .is_err()
    );
    assert!(
        runtime
            .invoke(&child.id, "agent_cancel", json!({"run_id":parent.id}))
            .await
            .is_err()
    );
    runtime.shutdown().await;
}

#[tokio::test]
async fn shutdown_closes_admission_before_draining_and_stays_closed() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = Runtime::open(directory.path(), fake(), Arc::new(FakeHost), false).unwrap();
    let run = runtime.start("in flight".into(), None, None).unwrap();
    runtime.begin_shutdown();
    assert!(runtime.start("late request".into(), None, None).is_err());
    assert!(
        runtime
            .start("late child".into(), Some(run.id.clone()), None)
            .is_err()
    );
    runtime.shutdown().await;
    assert_eq!(runtime.status(&run.id).unwrap().status, Status::Cancelled);
    assert!(runtime.start("after drain".into(), None, None).is_err());
}

#[tokio::test]
async fn record_cannot_grow_journal_past_capacity() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = Runtime::open(directory.path(), fake(), Arc::new(FakeHost), false).unwrap();
    fill_journal(&runtime, 64 * 1024 * 1024 - 1);
    assert!(runtime.start("would overflow".into(), None, None).is_err());
    assert_eq!(
        runtime
            .state
            .lock()
            .unwrap()
            .journal
            .metadata()
            .unwrap()
            .len(),
        64 * 1024 * 1024 - 1
    );
    assert!(runtime.list().is_empty());
}

fn fill_journal(runtime: &Runtime, target_bytes: u64) {
    let mut state = runtime.state.lock().unwrap();
    let mut remaining = target_bytes - state.journal.metadata().unwrap().len();
    let zeros = [0_u8; 64 * 1024];
    while remaining > 0 {
        let count = remaining.min(zeros.len() as u64) as usize;
        state.journal.write_all(&zeros[..count]).unwrap();
        remaining -= count as u64;
    }
}
