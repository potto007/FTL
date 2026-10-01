use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use computer_use::{ActionResult, Platform};

use super::*;

struct FakeActor {
    calls: Arc<Mutex<Vec<Action>>>,
    fail_release: bool,
}

#[async_trait]
impl Actor for FakeActor {
    fn platform(&self) -> Option<Platform> {
        Some(Platform::Windows)
    }
    async fn perform_actions(
        &mut self,
        actions: &[Action],
        _options: Options,
    ) -> Result<ActionResult, String> {
        self.calls.lock().unwrap().extend_from_slice(actions);
        if self.fail_release {
            return Err("fake failure".to_owned());
        }
        Ok(ActionResult {
            screenshot: None,
            cursor_position: None,
        })
    }
}

#[test]
fn drop_releases_hotkey_in_reverse_order_without_real_input() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut actor = FakeActor {
        calls: calls.clone(),
        fail_release: false,
    };
    {
        let mut batch = BalancedInput {
            actor: &mut actor,
            releases: Vec::new(),
        };
        batch.releases.push(Action::KeyUp {
            key: Key::Keycode(0x11),
        });
        batch.releases.push(Action::KeyUp {
            key: Key::Keycode(0x41),
        });
    }
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            Action::KeyUp {
                key: Key::Keycode(0x41)
            },
            Action::KeyUp {
                key: Key::Keycode(0x11)
            }
        ]
    );
}

#[test]
fn release_failure_retries_all_keys_and_reports_uncertain_state() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut actor = FakeActor {
        calls: calls.clone(),
        fail_release: true,
    };
    let mut batch = BalancedInput {
        actor: &mut actor,
        releases: vec![
            Action::KeyUp {
                key: Key::Keycode(0x11),
            },
            Action::MouseUp {
                button: MouseButton::Left,
            },
        ],
    };
    assert!(batch.finish().is_err());
    assert_eq!(calls.lock().unwrap().len(), 4);
    assert!(batch.releases.is_empty());
}
