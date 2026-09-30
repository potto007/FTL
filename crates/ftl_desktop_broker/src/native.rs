use std::fs::{File, OpenOptions};
use std::mem::size_of;
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

#[path = "indicator.rs"]
mod indicator;

use computer_use::{
    Action, Actor, Key, MouseButton, Options, ScreenshotParams, ScreenshotRegion, ScrollDirection,
    ScrollDistance, Vector2I,
};
use futures::executor::block_on;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetLastInputInfo, LASTINPUTINFO,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect};
use windows::core::BOOL;

use super::{Backend, BrokerError, Capture, DesktopState, Direction, Input, Rect, failure};

pub(super) struct NativeBackend {
    actor: Box<dyn Actor>,
    exclusive: Option<File>,
    expected: Option<(usize, DesktopState)>,
    indicator: Option<indicator::StopIndicator>,
}

impl NativeBackend {
    pub(super) fn new() -> Self {
        Self {
            actor: computer_use::create_actor(),
            exclusive: None,
            expected: None,
            indicator: None,
        }
    }

    fn acquire(&mut self) -> Result<(), BrokerError> {
        if self.exclusive.is_none() {
            let root = std::env::var_os("LOCALAPPDATA")
                .ok_or_else(|| failure("Interactive user profile is unavailable"))?;
            let directory = PathBuf::from(root).join("FTL");
            std::fs::create_dir_all(&directory)
                .map_err(|_| failure("Cannot create desktop broker lock directory"))?;
            // 禁止共享同一个句柄文件，跨进程维持单一桌面控制者；退出自动释放。
            self.exclusive = Some(
                OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .read(true)
                    .write(true)
                    .share_mode(0)
                    .open(directory.join("desktop-broker.lock"))
                    .map_err(|_| failure("Another FTL process owns the desktop broker"))?,
            );
        }
        Ok(())
    }
}

impl Backend for NativeBackend {
    fn activate(
        &mut self,
        revoked: Arc<AtomicBool>,
        expires_at: Instant,
        monitor: usize,
    ) -> Result<(), BrokerError> {
        self.acquire()?;
        if self.indicator.is_none() {
            let state = desktop_state(monitor)?;
            self.indicator = Some(indicator::StopIndicator::start(
                revoked,
                expires_at,
                state.monitor,
            )?);
        }
        Ok(())
    }
    fn state(&mut self, monitor: usize) -> Result<DesktopState, BrokerError> {
        self.acquire()?;
        let state = desktop_state(monitor)?;
        self.expected = Some((monitor, state.clone()));
        Ok(state)
    }

    fn capture(&mut self, region: Rect) -> Result<Capture, BrokerError> {
        region.dimensions()?;
        let result = block_on(self.actor.perform_actions(
            &[],
            Options {
                screenshot_params: Some(ScreenshotParams {
                    max_long_edge_px: Some(1600),
                    // 缩放尺寸会四舍五入，预留余量避免正方形显示器超过输出硬上限。
                    max_total_px: Some(1_590_000),
                    region: Some(ScreenshotRegion {
                        top_left: Vector2I::new(region.left, region.top),
                        bottom_right: Vector2I::new(region.right, region.bottom),
                    }),
                }),
            },
        ))
        .map_err(|_| failure("Native screenshot failed; no image was returned"))?;
        let screenshot = result
            .screenshot
            .ok_or_else(|| failure("Native capture returned no image"))?;
        Ok(Capture {
            png: screenshot.data,
            width: screenshot.width as u32,
            height: screenshot.height as u32,
        })
    }

    fn input(&mut self, input: Input, permitted: &dyn Fn() -> bool) -> Result<(), BrokerError> {
        let (monitor, expected) = self
            .expected
            .as_ref()
            .ok_or_else(|| failure("No validated desktop state"))?;
        if &desktop_state(*monitor)? != expected {
            return Err(failure("Desktop changed immediately before input"));
        }
        // 检查真实按键，避免合成输入与用户正在按住的按钮组合。
        if (1..=255).any(|key| unsafe { GetAsyncKeyState(key) } < 0) {
            return Err(failure(
                "A physical key or mouse button is held; input paused",
            ));
        }
        let mut batch = BalancedInput {
            actor: self.actor.as_mut(),
            releases: Vec::new(),
        };
        match input {
            Input::Click { x, y, right } => {
                let button = if right {
                    MouseButton::Right
                } else {
                    MouseButton::Left
                };
                if !permitted() {
                    return Err(failure("Desktop input cancelled"));
                }
                batch.releases.push(Action::MouseUp {
                    button: button.clone(),
                });
                batch.dispatch(Action::MouseDown {
                    button,
                    at: Vector2I::new(x, y),
                })?;
            }
            Input::Type(text) => {
                if !permitted() {
                    return Err(failure("Desktop input cancelled"));
                }
                batch.dispatch(Action::TypeText { text })?;
            }
            Input::Hotkey(keys) => {
                for code in keys {
                    if !permitted() {
                        return Err(failure(
                            "Desktop input cancelled; held-key release attempted",
                        ));
                    }
                    let key = Key::Keycode(code);
                    batch.releases.push(Action::KeyUp { key: key.clone() });
                    batch.dispatch(Action::KeyDown { key })?;
                }
            }
            Input::Scroll {
                x,
                y,
                direction,
                notches,
            } => {
                if !permitted() {
                    return Err(failure("Desktop input cancelled"));
                }
                let direction = match direction {
                    Direction::Up => ScrollDirection::Up,
                    Direction::Down => ScrollDirection::Down,
                    Direction::Left => ScrollDirection::Left,
                    Direction::Right => ScrollDirection::Right,
                };
                batch.dispatch(Action::MouseWheel {
                    at: Vector2I::new(x, y),
                    direction,
                    distance: ScrollDistance::Clicks(notches),
                })?;
            }
        }
        batch.finish()
    }
}

struct BalancedInput<'a> {
    actor: &'a mut dyn Actor,
    releases: Vec<Action>,
}

impl BalancedInput<'_> {
    fn dispatch(&mut self, action: Action) -> Result<(), BrokerError> {
        block_on(self.actor.perform_actions(&[action], Options {screenshot_params:None}))
            .map(|_| ())
            .map_err(|_| failure("Native input failed; effects may be partial. Do not retry without a fresh observation"))
    }

    fn finish(&mut self) -> Result<(), BrokerError> {
        let mut failed = false;
        while let Some(action) = self.releases.pop() {
            // 首次释放失败时重试一次；仍失败则报告未知状态，不能宣称已释放。
            if self.dispatch(action.clone()).is_err() && self.dispatch(action).is_err() {
                failed = true;
            }
        }
        if failed {
            Err(failure(
                "Input release failed; desktop state is uncertain. Stop automation and release held keys manually",
            ))
        } else {
            Ok(())
        }
    }
}

impl Drop for BalancedInput<'_> {
    fn drop(&mut self) {
        // 取消、错误和展开栈都会尽力补偿已经发送或结果未知的按下事件。
        let _ = self.finish();
    }
}

struct DpiGuard(DPI_AWARENESS_CONTEXT);

impl DpiGuard {
    fn new() -> Result<Self, BrokerError> {
        // Win32 线程 DPI 状态只在当前同步调用期间修改。
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.0.is_null() {
            return Err(failure("Physical monitor coordinates are unavailable"));
        }
        Ok(Self(previous))
    }
}

impl Drop for DpiGuard {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}

unsafe extern "system" fn collect_monitor(
    handle: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // 指针来自本次同步枚举调用，Win32 不保存该指针。
    if !unsafe { GetMonitorInfoW(handle, &mut info.monitorInfo) }.as_bool() {
        return BOOL(0);
    }
    let monitors = unsafe { &mut *(data.0 as *mut Vec<(String, Rect)>) };
    let end = info
        .szDevice
        .iter()
        .position(|c| *c == 0)
        .unwrap_or(info.szDevice.len());
    monitors.push((
        String::from_utf16_lossy(&info.szDevice[..end]),
        rect(info.monitorInfo.rcMonitor),
    ));
    BOOL(1)
}

fn rect(value: RECT) -> Rect {
    Rect {
        left: value.left,
        top: value.top,
        right: value.right,
        bottom: value.bottom,
    }
}

fn desktop_state(index: usize) -> Result<DesktopState, BrokerError> {
    if !computer_use::is_supported_on_current_platform() {
        return Err(failure(
            "An accessible interactive Windows desktop is required",
        ));
    }
    let _dpi = DpiGuard::new()?;
    let mut monitors: Vec<(String, Rect)> = Vec::new();
    let ok = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&mut monitors as *mut _ as isize),
        )
    };
    if !ok.as_bool() {
        return Err(failure("Monitor enumeration failed"));
    }
    monitors.sort_by(|a, b| a.0.cmp(&b.0));
    let (identity, monitor) = monitors
        .get(index)
        .cloned()
        .ok_or_else(|| failure("Approved monitor is unavailable"))?;
    monitor.dimensions()?;
    let window = unsafe { GetForegroundWindow() };
    if window.0.is_null() {
        return Err(failure("No foreground window is available"));
    }
    let mut bounds = RECT::default();
    unsafe { GetWindowRect(window, &mut bounds) }
        .map_err(|_| failure("Cannot inspect foreground window"))?;
    let mut last = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if !unsafe { GetLastInputInfo(&mut last) }.as_bool() {
        return Err(failure("Cannot inspect desktop input activity"));
    }
    Ok(DesktopState {
        monitor,
        monitor_identity: identity,
        foreground: window.0 as usize as u64,
        foreground_rect: rect(bounds),
        last_input: last.dwTime,
    })
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
