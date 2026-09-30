use std::sync::{Arc, Mutex};

use futures::executor::block_on;

use super::*;

struct FakeData {
    state: DesktopState,
    captures: usize,
    inputs: Vec<Input>,
    change_during_capture: bool,
    fail_input: bool,
    fail_indicator: bool,
    indicator_flag: Option<Arc<AtomicBool>>,
    capture_hook: Option<Box<dyn FnOnce() + Send>>,
}

struct FakeBackend(Arc<Mutex<FakeData>>);

impl Backend for FakeBackend {
    fn activate(
        &mut self,
        revoked: Arc<AtomicBool>,
        _expires_at: Instant,
        _monitor: usize,
    ) -> Result<(), BrokerError> {
        let mut data = self.0.lock().unwrap();
        if data.fail_indicator {
            return Err(failure("Indicator unavailable"));
        }
        data.indicator_flag = Some(revoked);
        Ok(())
    }
    fn state(&mut self, _monitor: usize) -> Result<DesktopState, BrokerError> {
        Ok(self.0.lock().unwrap().state.clone())
    }

    fn capture(&mut self, _region: Rect) -> Result<Capture, BrokerError> {
        let mut data = self.0.lock().unwrap();
        data.captures += 1;
        if data.change_during_capture {
            data.state.foreground += 1;
        }
        let hook = data.capture_hook.take();
        drop(data);
        if let Some(hook) = hook {
            hook();
        }
        Ok(Capture {
            png: vec![137, 80, 78, 71],
            width: 960,
            height: 540,
        })
    }

    fn input(&mut self, input: Input, permitted: &dyn Fn() -> bool) -> Result<(), BrokerError> {
        if !permitted() {
            return Err(failure("Cancelled"));
        }
        let mut data = self.0.lock().unwrap();
        data.inputs.push(input);
        if data.fail_input {
            return Err(failure("Uncertain partial input"));
        }
        Ok(())
    }
}

fn config() -> DesktopBrokerConfig {
    DesktopBrokerConfig {
        allow_capture: true,
        allow_input: true,
        model_supports_vision: true,
        approved_model_endpoint: "http://127.0.0.1:8080/v1".to_owned(),
        monitor: Some(0),
        grant_duration: Duration::from_secs(120),
    }
}

fn broker(config: DesktopBrokerConfig) -> (DesktopBroker, Arc<Mutex<FakeData>>) {
    let rect = Rect {
        left: -1920,
        top: -1080,
        right: 0,
        bottom: 0,
    };
    let data = Arc::new(Mutex::new(FakeData {
        state: DesktopState {
            monitor: rect,
            monitor_identity: "display1".to_owned(),
            foreground: 12,
            foreground_rect: rect,
            last_input: 42,
        },
        captures: 0,
        inputs: Vec::new(),
        change_during_capture: false,
        fail_input: false,
        fail_indicator: false,
        indicator_flag: None,
        capture_hook: None,
    }));
    let broker = DesktopBroker::with_backend(config, Box::new(FakeBackend(data.clone()))).unwrap();
    (broker, data)
}

fn call(broker: &DesktopBroker, run: &str, tool: &str, args: Value) -> Result<Value, BrokerError> {
    block_on(broker.call_for_run(run, tool, args))
}

fn observe(broker: &DesktopBroker, run: &str) -> String {
    let result = call(broker, run, "desktop_observe", json!({})).unwrap();
    assert_eq!(result["content"][1]["type"], "image");
    let metadata: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(metadata["physical_rect"]["left"], -1920);
    metadata["observation_id"].as_str().unwrap().to_owned()
}

#[test]
fn default_denies_before_touching_backend() {
    let (broker, data) = broker(DesktopBrokerConfig::default());
    assert!(broker.tool_schemas().is_empty());
    assert!(call(&broker, "a", "desktop_observe", json!({})).is_err());
    assert_eq!(data.lock().unwrap().captures, 0);
}

#[test]
fn validates_vision_destination_monitor_and_grant_limits() {
    let cases = [
        DesktopBrokerConfig {
            model_supports_vision: false,
            ..config()
        },
        DesktopBrokerConfig {
            monitor: None,
            ..config()
        },
        DesktopBrokerConfig {
            grant_duration: Duration::ZERO,
            ..config()
        },
        DesktopBrokerConfig {
            grant_duration: Duration::from_secs(301),
            ..config()
        },
        DesktopBrokerConfig {
            approved_model_endpoint: "https://example.com/v1".to_owned(),
            ..config()
        },
        DesktopBrokerConfig {
            approved_model_endpoint: "http://localhost:8080/v1".to_owned(),
            ..config()
        },
        DesktopBrokerConfig {
            approved_model_endpoint: "http://user@127.0.0.1/v1".to_owned(),
            ..config()
        },
        DesktopBrokerConfig {
            allow_capture: false,
            ..config()
        },
    ];
    for invalid in cases {
        let (_, data) = broker(config());
        assert!(DesktopBroker::with_backend(invalid, Box::new(FakeBackend(data))).is_err());
    }
}

#[test]
fn click_maps_resized_negative_monitor_coordinates_and_consumes_id() {
    let (broker, data) = broker(config());
    let id = observe(&broker, "a");
    call(
        &broker,
        "a",
        "desktop_click",
        json!({"observation_id":id,"x":0,"y":539}),
    )
    .unwrap();
    assert_eq!(
        data.lock().unwrap().inputs,
        [Input::Click {
            x: -1919,
            y: -1,
            right: false
        }]
    );
    assert!(
        call(
            &broker,
            "a",
            "desktop_click",
            json!({"observation_id":id,"x":1,"y":1})
        )
        .is_err()
    );
}

#[test]
fn coordinates_outside_image_are_rejected_without_clamping() {
    let (broker, data) = broker(config());
    let id = observe(&broker, "a");
    for (x, y) in [(960, 1), (-1, 1), (1, 540), (i64::MAX, 1)] {
        assert!(
            call(
                &broker,
                "a",
                "desktop_click",
                json!({"observation_id":id,"x":x,"y":y})
            )
            .is_err()
        );
    }
    assert!(data.lock().unwrap().inputs.is_empty());
}

#[test]
fn focus_geometry_and_user_intervention_invalidate_observation() {
    for kind in 0..3 {
        let (broker, data) = broker(config());
        let id = observe(&broker, "a");
        {
            let mut data = data.lock().unwrap();
            match kind {
                0 => data.state.foreground += 1,
                1 => data.state.monitor.left -= 1,
                2 => data.state.last_input += 1,
                _ => unreachable!(),
            }
        }
        assert!(
            call(
                &broker,
                "a",
                "desktop_type",
                json!({"observation_id":id,"text":"hello"})
            )
            .is_err()
        );
        assert!(data.lock().unwrap().inputs.is_empty());
        assert!(broker.state.lock().unwrap().observation.is_none());
    }
}

#[test]
fn run_lease_blocks_peers_until_owner_release() {
    let (broker, data) = broker(config());
    observe(&broker, "a");
    assert!(call(&broker, "b", "desktop_observe", json!({})).is_err());
    assert!(call(&broker, "b", "desktop_release", json!({})).is_err());
    call(&broker, "a", "desktop_release", json!({})).unwrap();
    observe(&broker, "b");
    assert_eq!(data.lock().unwrap().captures, 2);
}

#[test]
fn stale_lease_can_be_reclaimed_but_never_reused_for_input() {
    let (broker, data) = broker(config());
    let id = observe(&broker, "a");
    broker
        .state
        .lock()
        .unwrap()
        .observation
        .as_mut()
        .unwrap()
        .captured_at -= OBSERVATION_TTL;
    assert!(
        call(
            &broker,
            "a",
            "desktop_type",
            json!({"observation_id":id,"text":"hello"})
        )
        .is_err()
    );
    observe(&broker, "b");
    assert!(data.lock().unwrap().inputs.is_empty());
}

#[test]
fn cancellation_revocation_and_grant_expiry_deny_capture() {
    let (mut broker, data) = broker(config());
    observe(&broker, "a");
    broker.cancel_run("a");
    assert!(call(&broker, "a", "desktop_observe", json!({})).is_err());
    observe(&broker, "b");
    broker.granted_at -= Duration::from_secs(120);
    assert!(call(&broker, "b", "desktop_observe", json!({})).is_err());
    broker.revoke();
    assert!(call(&broker, "c", "desktop_observe", json!({})).is_err());
    assert_eq!(data.lock().unwrap().captures, 2);
}

#[test]
fn capture_race_discards_image_and_input_failure_requires_new_observation() {
    let (broker, data) = broker(config());
    data.lock().unwrap().change_during_capture = true;
    assert!(call(&broker, "a", "desktop_observe", json!({})).is_err());
    assert!(broker.state.lock().unwrap().observation.is_none());
    data.lock().unwrap().change_during_capture = false;
    let id = observe(&broker, "a");
    data.lock().unwrap().fail_input = true;
    assert!(
        call(
            &broker,
            "a",
            "desktop_hotkey",
            json!({"observation_id":id,"keys":["CTRL","A"]})
        )
        .is_err()
    );
    assert!(broker.state.lock().unwrap().observation.is_none());
    assert!(
        call(
            &broker,
            "a",
            "desktop_hotkey",
            json!({"observation_id":id,"keys":["CTRL","A"]})
        )
        .is_err()
    );
    assert_eq!(data.lock().unwrap().inputs.len(), 1);
}

#[test]
fn input_requires_additional_grant_and_arguments_are_bounded() {
    let (read_only, data) = broker(DesktopBrokerConfig {
        allow_input: false,
        ..config()
    });
    let id = observe(&read_only, "a");
    assert_eq!(read_only.tool_schemas().len(), 2);
    assert!(
        call(
            &read_only,
            "a",
            "desktop_click",
            json!({"observation_id":id,"x":1,"y":1})
        )
        .is_err()
    );
    assert!(data.lock().unwrap().inputs.is_empty());
    let (broker, data) = broker(config());
    let id = observe(&broker, "a");
    for keys in [
        json!(["CTRL", "CTRL"]),
        json!(["F999"]),
        json!([]),
        json!(["A", "B", "C", "D", "E"]),
    ] {
        assert!(
            call(
                &broker,
                "a",
                "desktop_hotkey",
                json!({"observation_id":id,"keys":keys})
            )
            .is_err()
        );
    }
    assert!(
        call(
            &broker,
            "a",
            "desktop_type",
            json!({"observation_id":id,"text":"x".repeat(4097)})
        )
        .is_err()
    );
    assert!(
        call(
            &broker,
            "a",
            "desktop_scroll",
            json!({"observation_id":id,"x":1,"y":1,"direction":"up","notches":11})
        )
        .is_err()
    );
    assert!(data.lock().unwrap().inputs.is_empty());
}

#[test]
fn concurrent_call_is_rejected_instead_of_queueing_stale_input() {
    let (broker, data) = broker(config());
    let _guard = broker.state.lock().unwrap();
    assert!(call(&broker, "a", "desktop_observe", json!({})).is_err());
    assert_eq!(data.lock().unwrap().captures, 0);
}

#[test]
fn oversized_signed_monitor_rectangle_is_rejected_without_overflow() {
    assert!(
        Rect {
            left: i32::MIN,
            top: i32::MIN,
            right: i32::MAX,
            bottom: i32::MAX
        }
        .dimensions()
        .is_err()
    );
}

#[test]
fn visible_indicator_is_required_and_stop_revokes_capture() {
    let (broker, data) = broker(config());
    data.lock().unwrap().fail_indicator = true;
    assert!(call(&broker, "a", "desktop_observe", json!({})).is_err());
    assert_eq!(data.lock().unwrap().captures, 0);
    data.lock().unwrap().fail_indicator = false;
    observe(&broker, "a");
    data.lock()
        .unwrap()
        .indicator_flag
        .as_ref()
        .unwrap()
        .store(true, Ordering::SeqCst);
    assert!(call(&broker, "a", "desktop_observe", json!({})).is_err());
    assert_eq!(data.lock().unwrap().captures, 1);
}

#[test]
fn dropping_broker_revokes_indicator_lifetime() {
    let (broker, data) = broker(config());
    observe(&broker, "a");
    let flag = data.lock().unwrap().indicator_flag.clone().unwrap();
    assert!(!flag.load(Ordering::SeqCst));
    drop(broker);
    assert!(flag.load(Ordering::SeqCst));
}

#[test]
fn monitor_index_cannot_silently_switch_capture_to_another_display() {
    let (broker, data) = broker(config());
    observe(&broker, "a");
    data.lock().unwrap().state.monitor_identity = "replacement-display".to_owned();
    assert!(call(&broker, "a", "desktop_observe", json!({})).is_err());
    assert_eq!(data.lock().unwrap().captures, 1);
    assert!(broker.revoked.load(Ordering::SeqCst));
}

#[test]
fn cancelling_sibling_during_capture_preserves_owner_but_owner_cancel_withholds_image() {
    for cancelled_run in ["owner", "sibling"] {
        let (broker, data) = broker(config());
        let broker = Arc::new(broker);
        let weak = Arc::downgrade(&broker);
        let join = Arc::new(Mutex::new(None));
        let join_from_capture = join.clone();
        data.lock().unwrap().capture_hook = Some(Box::new(move || {
            let broker = weak.upgrade().unwrap();
            let canceller = broker.clone();
            let thread = std::thread::spawn(move || canceller.cancel_run(cancelled_run));
            let deadline = Instant::now() + Duration::from_secs(2);
            // 取消先写入独立锁，因此无需等待正在进行的桌面捕获退出。
            while !broker.run_cancelled(cancelled_run) {
                assert!(
                    Instant::now() < deadline,
                    "Cancellation must not wait on the desktop lock"
                );
                std::thread::yield_now();
            }
            *join_from_capture.lock().unwrap() = Some(thread);
        }));
        let result = call(&broker, "owner", "desktop_observe", json!({}));
        join.lock().unwrap().take().unwrap().join().unwrap();
        assert_eq!(result.is_ok(), cancelled_run == "sibling");
        assert_eq!(data.lock().unwrap().captures, 1);
    }
}
