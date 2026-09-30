use super::*;

pub struct TestDirectory(pub PathBuf);
impl TestDirectory {
    pub fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "ftl-agent-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn stdio_framing_is_bounded_and_rejects_truncation() {
    let mut reader = std::io::Cursor::new(b"{\"id\":1}\n{\"id\":2}\n");
    assert_eq!(read_frame(&mut reader).unwrap().unwrap(), b"{\"id\":1}\n");
    assert_eq!(read_frame(&mut reader).unwrap().unwrap(), b"{\"id\":2}\n");
    assert!(read_frame(&mut reader).unwrap().is_none());
    assert!(read_frame(&mut std::io::Cursor::new(b"truncated")).is_err());
    assert!(read_frame(&mut std::io::Cursor::new(vec![b'x'; 1024 * 1024 + 1])).is_err());
}

struct ModelFixture;
#[async_trait::async_trait]
impl ftl_agent_core::Model for ModelFixture {
    async fn complete(&self, _: &[Value], _: &[Value]) -> Result<Value> {
        Ok(json!({"role":"assistant","content":"independent local result"}))
    }
}

#[tokio::test]
async fn mcp_start_wait_and_recovery_use_real_runtime_without_ui() {
    let directory = TestDirectory::new();
    let host: Arc<dyn ToolHost> = Arc::new(EmptyToolHost);
    let runtime = Runtime::open(&directory.0, Arc::new(ModelFixture), host.clone(), false).unwrap();
    let initialized = dispatch(
        &runtime,
        host.as_ref(),
        json!({"id":1,"method":"initialize"}),
    )
    .await;
    assert_eq!(
        initialized["result"]["serverInfo"]["name"],
        "ftl-agent-service"
    );
    let started=dispatch(&runtime,host.as_ref(),json!({"id":2,"method":"tools/call","params":{"name":"agent_start","arguments":{"prompt":"test"}}})).await;
    let run: Value =
        serde_json::from_str(started["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let result = call(
        &runtime,
        host.as_ref(),
        "agent_wait",
        json!({"run_id":run["id"],"timeout_ms":1000}),
    )
    .await
    .unwrap();
    let result: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["status"], "completed");
    assert_eq!(result["output"], "independent local result");
    runtime.shutdown().await;
    drop(runtime);
    let restored = Runtime::open(&directory.0, Arc::new(ModelFixture), host, false).unwrap();
    assert_eq!(
        restored
            .status(run["id"].as_str().unwrap())
            .unwrap()
            .output
            .as_deref(),
        Some("independent local result")
    );
}

#[tokio::test]
async fn desktop_tools_cannot_escape_pinned_service_model_via_mcp() {
    struct DesktopFixture;
    #[async_trait::async_trait]
    impl ToolHost for DesktopFixture {
        fn tools(&self) -> Vec<Value> {
            vec![json!({"name":"desktop_observe"})]
        }
        async fn call(&self, _: &str, _: &str, _: Value) -> Result<Value> {
            panic!("MCP must not call desktop host directly")
        }
    }
    let directory = TestDirectory::new();
    let host: Arc<dyn ToolHost> = Arc::new(DesktopFixture);
    let runtime = Runtime::open(&directory.0, Arc::new(ModelFixture), host.clone(), true).unwrap();
    assert!(
        !service_tools(host.as_ref())
            .iter()
            .any(|t| t["name"] == "desktop_observe")
    );
    assert!(
        call(&runtime, host.as_ref(), "desktop_observe", json!({}))
            .await
            .is_err()
    );
}
