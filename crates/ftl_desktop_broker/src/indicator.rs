use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, COLOR_WINDOW, DT_CENTER, DT_VCENTER, DT_WORDBREAK, DrawTextW, EndPaint,
    GetSysColorBrush, PAINTSTRUCT, UpdateWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
    GetClientRect, GetMessageW, GetWindowLongPtrW, MSG, PostQuitMessage, RegisterClassW, SetTimer,
    SetWindowLongPtrW, TranslateMessage, WM_CLOSE, WM_DESTROY, WM_LBUTTONUP, WM_NCCREATE, WM_PAINT,
    WM_TIMER, WNDCLASSW, WS_BORDER, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    WS_VISIBLE,
};
use windows::core::w;

use super::{BrokerError, DpiGuard, Rect, failure};

static REGISTERED: OnceLock<bool> = OnceLock::new();

struct IndicatorState {
    revoked: Arc<AtomicBool>,
    expires_at: Instant,
}

pub(super) struct StopIndicator {
    revoked: Arc<AtomicBool>,
}

impl StopIndicator {
    pub(super) fn start(
        revoked: Arc<AtomicBool>,
        expires_at: Instant,
        monitor: Rect,
    ) -> Result<Self, BrokerError> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let state = Arc::new(IndicatorState {
            revoked: revoked.clone(),
            expires_at,
        });
        std::thread::Builder::new()
            .name("ftl-desktop-stop".to_owned())
            .spawn(move || {
                // 窗口和消息循环归属同一个线程；状态的 Arc 活到窗口销毁之后。
                let result = create_window(&state, monitor);
                match result {
                    Ok(window) => {
                        if sender.send(Ok(())).is_err() {
                            state.revoked.store(true, Ordering::SeqCst);
                        }
                        let mut message = MSG::default();
                        while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
                            unsafe {
                                let _ = TranslateMessage(&message);
                                DispatchMessageW(&message);
                            }
                        }
                        state.revoked.store(true, Ordering::SeqCst);
                        unsafe {
                            let _ = DestroyWindow(window);
                        }
                    }
                    Err(error) => {
                        state.revoked.store(true, Ordering::SeqCst);
                        let _ = sender.send(Err(error));
                    }
                }
            })
            .map_err(|_| failure("Cannot start visible desktop stop control"))?;
        match receiver.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(Self { revoked }),
            Ok(Err(error)) => Err(error),
            Err(_) => {
                revoked.store(true, Ordering::SeqCst);
                Err(failure("Visible desktop stop control did not become ready"))
            }
        }
    }
}

impl Drop for StopIndicator {
    fn drop(&mut self) {
        self.revoked.store(true, Ordering::SeqCst);
    }
}

fn create_window(state: &Arc<IndicatorState>, monitor: Rect) -> Result<HWND, BrokerError> {
    let _dpi = DpiGuard::new()?;
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }
        .map_err(|_| failure("Cannot initialize desktop stop control"))?
        .into();
    let registered = REGISTERED.get_or_init(|| {
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: w!("FTLDesktopStopControlV1"),
            hbrBackground: unsafe { GetSysColorBrush(COLOR_WINDOW) },
            ..Default::default()
        };
        (unsafe { RegisterClassW(&class) }) != 0
    });
    if !registered {
        return Err(failure("Cannot register visible desktop stop control"));
    }
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            w!("FTLDesktopStopControlV1"),
            w!("FTL desktop control - Stop"),
            WS_POPUP | WS_BORDER | WS_VISIBLE,
            monitor.left.saturating_add(16),
            monitor.top.saturating_add(16),
            300,
            64,
            None,
            None,
            Some(instance),
            Some(Arc::as_ptr(state).cast()),
        )
    }
    .map_err(|_| failure("Cannot show visible desktop stop control"))?;
    if unsafe { SetTimer(Some(window), 1, 100, None) } == 0 {
        unsafe {
            let _ = DestroyWindow(window);
        }
        return Err(failure("Cannot monitor desktop stop control lifetime"));
    }
    if !unsafe { UpdateWindow(window) }.as_bool() {
        unsafe {
            let _ = DestroyWindow(window);
        }
        return Err(failure("Cannot render visible desktop stop control"));
    }
    Ok(window)
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return LRESULT(1);
    }
    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const IndicatorState;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    let state = unsafe { &*pointer };
    match message {
        WM_LBUTTONUP | WM_CLOSE => {
            state.revoked.store(true, Ordering::SeqCst);
            unsafe {
                let _ = DestroyWindow(window);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if state.revoked.load(Ordering::SeqCst) || Instant::now() >= state.expires_at {
                state.revoked.store(true, Ordering::SeqCst);
                unsafe {
                    let _ = DestroyWindow(window);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            state.revoked.store(true, Ordering::SeqCst);
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let dc = unsafe { BeginPaint(window, &mut paint) };
            let mut rect = RECT::default();
            unsafe {
                let _ = GetClientRect(window, &mut rect);
            }
            let mut label: Vec<u16> = "FTL desktop control\nClick here to STOP"
                .encode_utf16()
                .collect();
            unsafe {
                DrawTextW(
                    dc,
                    &mut label,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_WORDBREAK,
                );
                let _ = EndPaint(window, &paint);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}
