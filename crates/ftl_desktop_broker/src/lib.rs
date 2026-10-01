//! 带有显式授权、观察凭据和独占租约的 Windows 桌面工具。

use std::collections::HashSet;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;
use serde_json::{Value, json};
use url::Url;

#[cfg(target_os = "windows")]
mod native;

const OBSERVATION_TTL: Duration = Duration::from_secs(30);
const MAX_CAPTURE_PIXELS: u64 = 33_177_600;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
static NEXT_BROKER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Default)]
pub struct DesktopBrokerConfig {
    pub allow_capture: bool,
    pub allow_input: bool,
    pub model_supports_vision: bool,
    pub approved_model_endpoint: String,
    pub monitor: Option<usize>,
    pub grant_duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerError(pub String);

impl fmt::Display for BrokerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BrokerError {}

fn failure(message: &str) -> BrokerError {
    BrokerError(message.to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Rect {
    fn dimensions(self) -> Result<(u32, u32), BrokerError> {
        let width = i64::from(self.right) - i64::from(self.left);
        let height = i64::from(self.bottom) - i64::from(self.top);
        if width <= 0 || height <= 0 || (width as u64) * (height as u64) > MAX_CAPTURE_PIXELS {
            return Err(failure("Monitor dimensions exceed capture limits"));
        }
        Ok((width as u32, height as u32))
    }

    fn intersects(self, other: Self) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DesktopState {
    monitor: Rect,
    monitor_identity: String,
    foreground: u64,
    foreground_rect: Rect,
    last_input: u32,
}

struct Capture {
    png: Vec<u8>,
    width: u32,
    height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Input {
    Click {
        x: i32,
        y: i32,
        right: bool,
    },
    Type(String),
    Hotkey(Vec<i32>),
    Scroll {
        x: i32,
        y: i32,
        direction: Direction,
        notches: i32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

trait Backend: Send {
    fn activate(
        &mut self,
        revoked: Arc<AtomicBool>,
        expires_at: Instant,
        monitor: usize,
    ) -> Result<(), BrokerError>;
    fn state(&mut self, monitor: usize) -> Result<DesktopState, BrokerError>;
    fn capture(&mut self, region: Rect) -> Result<Capture, BrokerError>;
    fn input(&mut self, input: Input, permitted: &dyn Fn() -> bool) -> Result<(), BrokerError>;
}

struct Observation {
    id: String,
    owner: String,
    state: DesktopState,
    width: u32,
    height: u32,
    captured_at: Instant,
}

struct State {
    backend: Box<dyn Backend>,
    observation: Option<Observation>,
    approved_monitor_identity: Option<String>,
}

pub struct DesktopBroker {
    config: DesktopBrokerConfig,
    granted_at: Instant,
    revoked: Arc<AtomicBool>,
    generation: AtomicU64,
    broker_id: u64,
    cancelled_runs: Mutex<HashSet<String>>,
    state: Mutex<State>,
}

impl DesktopBroker {
    pub fn new(config: DesktopBrokerConfig) -> Result<Self, BrokerError> {
        #[cfg(target_os = "windows")]
        {
            Self::with_backend(config, Box::new(native::NativeBackend::new()))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = config;
            Err(failure(
                "Native desktop tools require an interactive Windows session",
            ))
        }
    }

    fn with_backend(
        config: DesktopBrokerConfig,
        backend: Box<dyn Backend>,
    ) -> Result<Self, BrokerError> {
        if config.allow_input && !config.allow_capture {
            return Err(failure("Desktop input requires an explicit capture grant"));
        }
        if config.allow_capture {
            let endpoint = Url::parse(&config.approved_model_endpoint)
                .map_err(|_| failure("An approved loopback model endpoint is required"))?;
            if endpoint.scheme() != "http"
                || !matches!(endpoint.host_str(), Some("127.0.0.1" | "[::1]" | "::1"))
                || !endpoint.username().is_empty()
                || endpoint.password().is_some()
                || endpoint.query().is_some()
                || endpoint.fragment().is_some()
            {
                return Err(failure(
                    "Desktop observations require an approved literal loopback HTTP endpoint",
                ));
            }
            if !config.model_supports_vision
                || config.monitor.is_none()
                || config.grant_duration.is_zero()
                || config.grant_duration > Duration::from_secs(300)
            {
                return Err(failure(
                    "Capture requires explicit vision capability, monitor and a grant lasting 1 to 300 seconds",
                ));
            }
        }
        Ok(Self {
            config,
            granted_at: Instant::now(),
            revoked: Arc::new(AtomicBool::new(false)),
            generation: AtomicU64::new(1),
            broker_id: NEXT_BROKER.fetch_add(1, Ordering::Relaxed),
            cancelled_runs: Mutex::new(HashSet::new()),
            state: Mutex::new(State {
                backend,
                observation: None,
                approved_monitor_identity: None,
            }),
        })
    }

    pub fn tool_schemas(&self) -> Vec<Value> {
        if !self.config.allow_capture {
            return Vec::new();
        }
        let mut tools = vec![
            schema(
                "desktop_observe",
                "Capture the explicitly approved Windows monitor. Returns an observation_id valid for one action and 30 seconds.",
                json!({}),
                &[],
            ),
            schema(
                "desktop_release",
                "Release this run's desktop observation lease.",
                json!({}),
                &[],
            ),
        ];
        if self.config.allow_input {
            let observation = json!({"type":"string"});
            let coordinate = json!({"type":"integer","minimum":0});
            tools.extend([
                schema("desktop_click", "Click at integer coordinates in the last returned image. A fresh observation is returned after the action.", json!({"observation_id":observation,"x":coordinate,"y":coordinate,"button":{"type":"string","enum":["left","right"]}}), &["observation_id","x","y"]),
                schema("desktop_type", "Type Unicode text into the observed foreground window. Maximum 4096 UTF-8 bytes.", json!({"observation_id":observation,"text":{"type":"string","maxLength":4096}}), &["observation_id","text"]),
                schema("desktop_hotkey", "Press and release 1 to 4 distinct Windows keys. Supported names: CTRL, ALT, SHIFT, WIN, ENTER, TAB, ESC, BACKSPACE, DELETE, SPACE, LEFT, RIGHT, UP, DOWN, HOME, END, PAGEUP, PAGEDOWN, A-Z and 0-9.", json!({"observation_id":observation,"keys":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":4}}), &["observation_id","keys"]),
                schema("desktop_scroll", "Scroll 1 to 10 wheel notches at coordinates in the observed image.", json!({"observation_id":observation,"x":coordinate,"y":coordinate,"direction":{"type":"string","enum":["up","down","left","right"]},"notches":{"type":"integer","minimum":1,"maximum":10}}), &["observation_id","x","y","direction","notches"]),
            ]);
        }
        tools
    }

    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn cancel_run(&self, run_id: &str) {
        // 先记录取消，再释放短锁；绝不能持有取消锁等待桌面操作锁。
        if let Ok(mut cancelled) = self.cancelled_runs.lock() {
            if cancelled.len() >= 1024 {
                self.revoke();
            }
            cancelled.insert(run_id.to_owned());
        } else {
            self.revoke();
        }
        if let Ok(mut state) = self.state.lock() {
            if state
                .observation
                .as_ref()
                .is_some_and(|o| o.owner == run_id)
            {
                state.observation = None;
            }
        }
    }

    fn run_cancelled(&self, run_id: &str) -> bool {
        self.cancelled_runs
            .lock()
            .map_or(true, |cancelled| cancelled.contains(run_id))
    }

    fn check_grant(&self) -> Result<(), BrokerError> {
        if !self.config.allow_capture
            || self.revoked.load(Ordering::SeqCst)
            || self.granted_at.elapsed() >= self.config.grant_duration
        {
            return Err(failure(
                "Desktop capture grant is absent, revoked or expired",
            ));
        }
        Ok(())
    }

    /// 调用中没有异步挂起点；输入和补偿释放完成后才返回。
    pub async fn call_for_run(
        &self,
        run_id: &str,
        name: &str,
        args: Value,
    ) -> Result<Value, BrokerError> {
        self.check_grant()?;
        if run_id.is_empty() || run_id.len() > 256 {
            return Err(failure("Invalid run identity"));
        }
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| failure("Desktop operation already in progress"))?;
        if self.run_cancelled(run_id) {
            return Err(failure("Run was cancelled"));
        }
        let generation = self.generation.load(Ordering::SeqCst);
        state.backend.activate(
            self.revoked.clone(),
            self.granted_at + self.config.grant_duration,
            self.config.monitor.unwrap(),
        )?;
        self.check_grant()?;
        if state
            .observation
            .as_ref()
            .is_some_and(|o| o.owner != run_id && o.captured_at.elapsed() < OBSERVATION_TTL)
        {
            return Err(failure("Desktop is leased to another run"));
        }
        if name == "desktop_release" {
            state.observation = None;
            return Ok(
                json!({"content":[{"type":"text","text":"Desktop lease released"}],"isError":false}),
            );
        }
        if name != "desktop_observe" {
            if !self.config.allow_input {
                return Err(failure("Desktop input has not been approved"));
            }
            let observation = state
                .observation
                .as_ref()
                .ok_or_else(|| failure("Capture a new observation before input"))?;
            if observation.owner != run_id
                || observation.captured_at.elapsed() >= OBSERVATION_TTL
                || args.get("observation_id").and_then(Value::as_str)
                    != Some(observation.id.as_str())
            {
                return Err(failure("Observation is stale or belongs to another run"));
            }
            let expected = observation.state.clone();
            let input = parse_input(name, &args, observation)?;
            let current = state.backend.state(self.config.monitor.unwrap())?;
            if current != expected {
                state.observation = None;
                return Err(failure(
                    "Desktop focus, geometry or user input changed; capture a new observation",
                ));
            }
            self.check_grant()?;
            if self.run_cancelled(run_id) || generation != self.generation.load(Ordering::SeqCst) {
                return Err(failure("Desktop action cancelled"));
            }
            // 执行前消耗凭据，部分成功或未知结果不能盲目重试。
            state.observation = None;
            state.backend.input(input, &|| {
                self.check_grant().is_ok()
                    && !self.run_cancelled(run_id)
                    && generation == self.generation.load(Ordering::SeqCst)
            })?;
        }
        self.check_grant()?;
        if self.run_cancelled(run_id) || generation != self.generation.load(Ordering::SeqCst) {
            return Err(failure(
                "Desktop action completed or was cancelled; observation withheld",
            ));
        }
        let before = state.backend.state(self.config.monitor.unwrap())?;
        match state.approved_monitor_identity.as_ref() {
            Some(identity) if identity != &before.monitor_identity => {
                self.revoke();
                return Err(failure(
                    "Approved display identity changed; a new human grant is required",
                ));
            }
            None => state.approved_monitor_identity = Some(before.monitor_identity.clone()),
            Some(_) => {}
        }
        before.monitor.dimensions()?;
        let capture = state.backend.capture(before.monitor)?;
        let after = state.backend.state(self.config.monitor.unwrap())?;
        if before != after {
            return Err(failure(
                "Desktop changed during capture; observation discarded",
            ));
        }
        self.check_grant()?;
        if self.run_cancelled(run_id) || generation != self.generation.load(Ordering::SeqCst) {
            return Err(failure("Capture cancelled; observation discarded"));
        }
        if capture.png.len() > MAX_IMAGE_BYTES
            || capture.width == 0
            || capture.height == 0
            || capture.width.max(capture.height) > 1600
            || u64::from(capture.width) * u64::from(capture.height) > 1_600_000
        {
            return Err(failure("Screenshot exceeds encoded size limits"));
        }
        let sequence = self.generation.fetch_add(1, Ordering::SeqCst);
        let id = format!("desktop-{}-{sequence}", self.broker_id);
        let metadata = json!({"observation_id":id,"run_id":run_id,"monitor":self.config.monitor,
            "physical_rect":before.monitor,"image_width":capture.width,"image_height":capture.height,
            "foreground_id":before.foreground,"valid_for_ms":OBSERVATION_TTL.as_millis(),"coordinate_space":"image_pixels",
            "model_endpoint":self.config.approved_model_endpoint});
        state.observation = Some(Observation {
            id,
            owner: run_id.to_owned(),
            state: before,
            width: capture.width,
            height: capture.height,
            captured_at: Instant::now(),
        });
        Ok(
            json!({"content":[{"type":"text","text":metadata.to_string()},
            {"type":"image","mimeType":"image/png","data":STANDARD.encode(capture.png)}],"isError":false}),
        )
    }
}

impl Drop for DesktopBroker {
    fn drop(&mut self) {
        self.revoke();
    }
}

fn schema(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
}

fn image_point(args: &Value, observation: &Observation) -> Result<(i32, i32), BrokerError> {
    let x = args
        .get("x")
        .and_then(Value::as_u64)
        .ok_or_else(|| failure("x must be a nonnegative integer"))?;
    let y = args
        .get("y")
        .and_then(Value::as_u64)
        .ok_or_else(|| failure("y must be a nonnegative integer"))?;
    if x >= u64::from(observation.width) || y >= u64::from(observation.height) {
        return Err(failure("Coordinates are outside the observed image"));
    }
    let (width, height) = observation.state.monitor.dimensions()?;
    let px = i64::from(observation.state.monitor.left)
        + (((2 * x + 1) * u64::from(width)) / (2 * u64::from(observation.width))) as i64;
    let py = i64::from(observation.state.monitor.top)
        + (((2 * y + 1) * u64::from(height)) / (2 * u64::from(observation.height))) as i64;
    Ok((px as i32, py as i32))
}

fn parse_input(name: &str, args: &Value, observation: &Observation) -> Result<Input, BrokerError> {
    if !observation
        .state
        .monitor
        .intersects(observation.state.foreground_rect)
    {
        return Err(failure("Foreground window is outside the approved monitor"));
    }
    match name {
        "desktop_click" => {
            let (x, y) = image_point(args, observation)?;
            let right = match args.get("button").and_then(Value::as_str).unwrap_or("left") {
                "left" => false,
                "right" => true,
                _ => return Err(failure("Unsupported mouse button")),
            };
            Ok(Input::Click { x, y, right })
        }
        "desktop_type" => {
            let text = args
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| failure("text must be a string"))?;
            if text.is_empty() || text.len() > 4096 {
                return Err(failure("Text must contain 1 to 4096 UTF-8 bytes"));
            }
            Ok(Input::Type(text.to_owned()))
        }
        "desktop_hotkey" => {
            let keys = args
                .get("keys")
                .and_then(Value::as_array)
                .ok_or_else(|| failure("keys must be an array"))?;
            if keys.is_empty() || keys.len() > 4 {
                return Err(failure("Hotkey must contain 1 to 4 keys"));
            }
            let mut codes = Vec::new();
            for key in keys {
                let code = keycode(
                    key.as_str()
                        .ok_or_else(|| failure("Key names must be strings"))?,
                )?;
                if codes.contains(&code) {
                    return Err(failure("Repeated hotkey keys are not supported"));
                }
                codes.push(code);
            }
            Ok(Input::Hotkey(codes))
        }
        "desktop_scroll" => {
            let (x, y) = image_point(args, observation)?;
            let notches = args
                .get("notches")
                .and_then(Value::as_i64)
                .filter(|n| (1..=10).contains(n))
                .ok_or_else(|| failure("notches must be between 1 and 10"))?
                as i32;
            let direction = match args.get("direction").and_then(Value::as_str) {
                Some("up") => Direction::Up,
                Some("down") => Direction::Down,
                Some("left") => Direction::Left,
                Some("right") => Direction::Right,
                _ => return Err(failure("Unsupported scroll direction")),
            };
            Ok(Input::Scroll {
                x,
                y,
                direction,
                notches,
            })
        }
        _ => Err(failure("Unknown desktop tool")),
    }
}

fn keycode(key: &str) -> Result<i32, BrokerError> {
    match key {
        "CTRL" => Ok(0x11),
        "ALT" => Ok(0x12),
        "SHIFT" => Ok(0x10),
        "WIN" => Ok(0x5B),
        "ENTER" => Ok(0x0D),
        "TAB" => Ok(0x09),
        "ESC" => Ok(0x1B),
        "BACKSPACE" => Ok(0x08),
        "DELETE" => Ok(0x2E),
        "SPACE" => Ok(0x20),
        "LEFT" => Ok(0x25),
        "UP" => Ok(0x26),
        "RIGHT" => Ok(0x27),
        "DOWN" => Ok(0x28),
        "HOME" => Ok(0x24),
        "END" => Ok(0x23),
        "PAGEUP" => Ok(0x21),
        "PAGEDOWN" => Ok(0x22),
        value
            if value.len() == 1
                && value
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) =>
        {
            Ok(i32::from(value.as_bytes()[0]))
        }
        _ => Err(failure("Unsupported hotkey name")),
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
