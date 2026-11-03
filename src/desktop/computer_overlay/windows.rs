//! Win32 is needed for desktop-wide, click-through, capture-excluded windows;
//! gpui-kit controls do not expose these OS window flags.
use std::{
    cell::RefCell,
    collections::HashSet,
    sync::{OnceLock, mpsc},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

const WAKE: u32 = WM_APP + 31;
const ACCENT: u32 = 0x00d89432;
const PANEL: u32 = 0x00332c24;
const TIMER: usize = 1;

struct Command {
    owner: String,
    active: bool,
    status: Option<String>,
}
struct Service {
    tx: mpsc::Sender<Command>,
    thread: u32,
}
static SERVICE: OnceLock<Option<Service>> = OnceLock::new();

pub(super) fn update(owner: &str, active: bool, status: Option<&str>) {
    let service = SERVICE.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        let (ready, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("computer-overlay".into())
            .spawn(move || unsafe { run(rx, ready) })
            .ok()?;
        Some(Service {
            tx,
            thread: result.recv().ok()??,
        })
    });
    if let Some(service) = service {
        let _ = service.tx.send(Command {
            owner: owner.into(),
            active,
            status: status.map(str::to_owned),
        });
        // The queue is created before the worker announces its thread id.
        unsafe {
            PostThreadMessageW(service.thread, WAKE, 0, 0);
        }
    }
}

struct Screen {
    hwnd: HWND,
    rect: RECT,
}
struct State {
    owners: HashSet<String>,
    screens: Vec<Screen>,
    mouse_hook: HHOOK,
    key_hook: HHOOK,
    clicks: Vec<(POINT, Instant)>,
    keys: Option<(String, Instant)>,
    modifiers: HashSet<u32>,
    status: String,
}
thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

unsafe fn run(rx: mpsc::Receiver<Command>, ready: mpsc::SyncSender<Option<u32>>) {
    unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mut message: MSG = std::mem::zeroed();
        PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
        let class = wide("OmpComputerUseOverlay");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: GetModuleHandleW(std::ptr::null()),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            let _ = ready.send(None);
            return;
        }
        STATE.with_borrow_mut(|slot| {
            *slot = Some(State {
                owners: HashSet::new(),
                screens: Vec::new(),
                mouse_hook: std::ptr::null_mut(),
                key_hook: std::ptr::null_mut(),
                clicks: Vec::new(),
                keys: None,
                modifiers: HashSet::new(),
                status: String::new(),
            })
        });
        let _ = ready.send(Some(GetCurrentThreadId()));
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            if message.message == WAKE {
                STATE.with_borrow_mut(|slot| {
                    let state = slot.as_mut().unwrap();
                    let was_active = !state.owners.is_empty();
                    let mut changed = false;
                    for command in rx.try_iter() {
                        if command.active {
                            state.owners.insert(command.owner);
                            if let Some(status) = command.status {
                                let status = format!("最近操作：{status}");
                                changed |= state.status != status;
                                state.status = status;
                            }
                        } else {
                            state.owners.remove(&command.owner);
                        }
                    }
                    let active = !state.owners.is_empty();
                    if active && !was_active {
                        state.open();
                    }
                    if !active && was_active {
                        state.close();
                    }
                    if changed || active != was_active {
                        state.repaint();
                    }
                });
            } else {
                #[cfg(test)]
                if message.message == WAKE + 1 {
                    STATE.with_borrow_mut(|slot| {
                        if let Some(state) = slot {
                            if let Some(screen) = state.screens.first() {
                                state.clicks.push((
                                    POINT {
                                        x: (screen.rect.left + screen.rect.right) / 2,
                                        y: (screen.rect.top + screen.rect.bottom) / 2,
                                    },
                                    Instant::now(),
                                ));
                            }
                            state.keys = Some(("Ctrl + L".into(), Instant::now()));
                            state.arm_timer();
                            state.repaint();
                        }
                    });
                    continue;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        STATE.with_borrow_mut(|slot| {
            if let Some(state) = slot {
                state.close();
            }
        });
    }
}

impl State {
    unsafe fn open(&mut self) {
        unsafe {
            if self.status.is_empty() {
                self.status = "正在执行电脑操作".into();
            }
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(add_screen),
                &mut self.screens as *mut _ as isize,
            );
            if self.screens.is_empty() {
                return;
            }
            // Hooks only observe synthetic input during an active local turn.
            // They always forward the event; physical user input is never shown.
            self.mouse_hook = SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_hook),
                GetModuleHandleW(std::ptr::null()),
                0,
            );
            self.key_hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(key_hook),
                GetModuleHandleW(std::ptr::null()),
                0,
            );
        }
    }

    unsafe fn close(&mut self) {
        unsafe {
            if !self.mouse_hook.is_null() {
                UnhookWindowsHookEx(self.mouse_hook);
            }
            if !self.key_hook.is_null() {
                UnhookWindowsHookEx(self.key_hook);
            }
            self.mouse_hook = std::ptr::null_mut();
            self.key_hook = std::ptr::null_mut();
            for screen in self.screens.drain(..) {
                KillTimer(screen.hwnd, TIMER);
                DestroyWindow(screen.hwnd);
            }
            self.clicks.clear();
            self.keys = None;
            self.modifiers.clear();
            self.status.clear();
        }
    }

    unsafe fn repaint(&self) {
        for screen in &self.screens {
            unsafe {
                InvalidateRect(screen.hwnd, std::ptr::null(), 0);
            }
        }
    }

    unsafe fn arm_timer(&self) {
        if let Some(screen) = self.screens.first() {
            unsafe {
                SetTimer(screen.hwnd, TIMER, 50, None);
            }
        }
    }
}

unsafe extern "system" fn add_screen(_: HMONITOR, _: HDC, rect: *mut RECT, data: LPARAM) -> i32 {
    unsafe {
        let screens = &mut *(data as *mut Vec<Screen>);
        let rect = *rect;
        let class = wide("OmpComputerUseOverlay");
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("OMP正在操作电脑").as_ptr(),
            WS_POPUP,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if !hwnd.is_null() {
            SetLayeredWindowAttributes(hwnd, 0, 255, LWA_COLORKEY);
            // Never put an indicator into a model screenshot. If capture
            // exclusion is unavailable, do not display a capture-obscuring layer.
            if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0
                || !supports_capture_exclusion()
            {
                DestroyWindow(hwnd);
            } else {
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                screens.push(Screen { hwnd, rect });
            }
        }
        1
    }
}

fn supports_capture_exclusion() -> bool {
    // On older Windows, the same affinity value means a black capture rectangle.
    #[repr(C)]
    struct Version {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        service_pack: [u16; 128],
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(version: *mut Version) -> i32;
    }
    let mut version: Version = unsafe { std::mem::zeroed() };
    version.size = std::mem::size_of::<Version>() as u32;
    unsafe { RtlGetVersion(&mut version) == 0 && version.build >= 19041 }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_NCHITTEST => HTTRANSPARENT as isize,
            WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut paint: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(hwnd, &mut paint);
                STATE.with(|slot| {
                    if let Ok(slot) = slot.try_borrow()
                        && let Some(state) = slot.as_ref()
                    {
                        paint_screen(dc, hwnd, state);
                    }
                });
                EndPaint(hwnd, &paint);
                0
            }
            WM_TIMER => {
                with_state_mut(|state| {
                    let before = state.clicks.len();
                    state
                        .clicks
                        .retain(|(_, time)| time.elapsed() < Duration::from_millis(800));
                    let expired_key = state
                        .keys
                        .as_ref()
                        .is_some_and(|(_, time)| time.elapsed() >= Duration::from_millis(1600));
                    if expired_key {
                        state.keys = None;
                    }
                    if before != state.clicks.len() || expired_key {
                        state.repaint();
                    }
                    if state.clicks.is_empty() && state.keys.is_none() {
                        KillTimer(hwnd, TIMER);
                    }
                });
                0
            }
            WM_DISPLAYCHANGE => {
                with_state_mut(|state| {
                    state.close();
                    if !state.owners.is_empty() {
                        state.open();
                    }
                });
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

// Win32 may reenter a callback while an overlay window is being shown or closed.
// Dropping one visual update is preferable to unwinding across the FFI boundary.
fn with_state_mut(action: impl FnOnce(&mut State)) {
    STATE.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut()
            && let Some(state) = slot.as_mut()
        {
            action(state);
        }
    });
}

unsafe fn fill(dc: HDC, rect: RECT, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(dc, &rect, brush);
        DeleteObject(brush);
    }
}

unsafe fn text(dc: HDC, label: &str, mut rect: RECT) {
    unsafe {
        let label = wide(label);
        DrawTextW(
            dc,
            label.as_ptr(),
            (label.len() - 1) as i32,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }
}

unsafe fn paint_screen(dc: HDC, hwnd: HWND, state: &State) {
    unsafe {
        let Some(screen) = state.screens.iter().find(|screen| screen.hwnd == hwnd) else {
            return;
        };
        let w = screen.rect.right - screen.rect.left;
        let h = screen.rect.bottom - screen.rect.top;
        fill(
            dc,
            RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            },
            0,
        );
        for rect in [
            RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: 3,
            },
            RECT {
                left: 0,
                top: h - 3,
                right: w,
                bottom: h,
            },
            RECT {
                left: 0,
                top: 0,
                right: 3,
                bottom: h,
            },
            RECT {
                left: w - 3,
                top: 0,
                right: w,
                bottom: h,
            },
        ] {
            fill(dc, rect, ACCENT);
        }
        let banner_width = 360.min(w - 12);
        let banner = RECT {
            left: (w - banner_width) / 2,
            top: 3,
            right: (w + banner_width) / 2,
            bottom: 62,
        };
        fill(dc, banner, PANEL);
        let font = CreateFontW(
            -18,
            0,
            0,
            0,
            500,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            wide("Microsoft YaHei UI").as_ptr(),
        );
        let old_font = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, 0x00ffffff);
        text(
            dc,
            "OMP正在操作电脑",
            RECT {
                bottom: 34,
                ..banner
            },
        );
        SetTextColor(dc, 0x00d8d2cc);
        text(dc, &state.status, RECT { top: 34, ..banner });
        if let Some((keys, _)) = &state.keys {
            let rect = RECT {
                left: (w - banner_width) / 2,
                top: h - 100,
                right: (w + banner_width) / 2,
                bottom: h - 54,
            };
            fill(dc, rect, PANEL);
            SetTextColor(dc, 0x00ffffff);
            text(dc, keys, rect);
        }
        let pen = CreatePen(PS_SOLID, 3, ACCENT);
        let old_pen = SelectObject(dc, pen);
        let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        for (point, _) in &state.clicks {
            let x = point.x - screen.rect.left;
            let y = point.y - screen.rect.top;
            Ellipse(dc, x - 18, y - 18, x + 18, y + 18);
            Ellipse(dc, x - 4, y - 4, x + 4, y + 4);
        }
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        DeleteObject(pen);
        SelectObject(dc, old_font);
        DeleteObject(font);
    }
}

unsafe extern "system" fn mouse_hook(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let event = &*(data as *const MSLLHOOKSTRUCT);
            if event.flags & LLMHF_INJECTED != 0
                && matches!(
                    message as u32,
                    WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN
                )
            {
                with_state_mut(|state| {
                    if state.clicks.len() == 8 {
                        state.clicks.remove(0);
                    }
                    state.clicks.push((event.pt, Instant::now()));
                    state.arm_timer();
                    state.repaint();
                });
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, message, data)
    }
}

unsafe extern "system" fn key_hook(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let event = &*(data as *const KBDLLHOOKSTRUCT);
            if event.flags & LLKHF_INJECTED != 0 {
                with_state_mut(|state| {
                    let key = event.vkCode;
                    if matches!(
                        key as u16,
                        VK_CONTROL
                            | VK_LCONTROL
                            | VK_RCONTROL
                            | VK_MENU
                            | VK_LMENU
                            | VK_RMENU
                            | VK_SHIFT
                            | VK_LSHIFT
                            | VK_RSHIFT
                            | VK_LWIN
                            | VK_RWIN
                    ) {
                        if matches!(message as u32, WM_KEYUP | WM_SYSKEYUP) {
                            state.modifiers.remove(&key);
                        } else {
                            state.modifiers.insert(key);
                        }
                    }
                    if matches!(message as u32, WM_KEYDOWN | WM_SYSKEYDOWN) {
                        let label = key_label(event, &state.modifiers);
                        state.keys = Some((label, Instant::now()));
                        state.arm_timer();
                        state.repaint();
                    }
                });
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, message, data)
    }
}

fn key_label(event: &KBDLLHOOKSTRUCT, modifiers: &HashSet<u32>) -> String {
    let mut parts = Vec::new();
    for (keys, name) in [
        (&[VK_CONTROL, VK_LCONTROL, VK_RCONTROL][..], "Ctrl"),
        (&[VK_MENU, VK_LMENU, VK_RMENU][..], "Alt"),
        (&[VK_SHIFT, VK_LSHIFT, VK_RSHIFT][..], "Shift"),
        (&[VK_LWIN, VK_RWIN][..], "Win"),
    ] {
        if keys.iter().any(|key| modifiers.contains(&(*key as u32))) {
            parts.push(name.to_string());
        }
    }
    // Do not display literal text/password characters or Unicode packets.
    if parts.is_empty()
        && (event.vkCode == VK_PACKET as u32 || (0x30..=0x5a).contains(&event.vkCode))
    {
        return "正在输入文字".into();
    }
    let mut name = [0u16; 80];
    let param = ((event.scanCode << 16) | ((event.flags & LLKHF_EXTENDED) << 24)) as i32;
    let length = unsafe { GetKeyNameTextW(param, name.as_mut_ptr(), name.len() as i32) };
    if length > 0 {
        let name = String::from_utf16_lossy(&name[..length as usize]);
        if !parts.contains(&name) {
            parts.push(name);
        }
    }
    if parts.is_empty() {
        "按键操作".into()
    } else {
        parts.join(" + ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printable_input_is_not_disclosed_by_the_key_indicator() {
        let mut key: KBDLLHOOKSTRUCT = unsafe { std::mem::zeroed() };
        key.vkCode = b'A' as u32;
        assert_eq!(key_label(&key, &HashSet::new()), "正在输入文字");
        key.vkCode = VK_PACKET as u32;
        assert_eq!(key_label(&key, &HashSet::new()), "正在输入文字");
    }

    fn screens() -> Vec<HWND> {
        unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> i32 {
            unsafe {
                let mut class = [0u16; 80];
                let len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
                if String::from_utf16_lossy(&class[..len.max(0) as usize])
                    == "OmpComputerUseOverlay"
                {
                    (*(data as *mut Vec<HWND>)).push(hwnd);
                }
            }
            1
        }
        let mut result = Vec::new();
        unsafe {
            EnumWindows(Some(collect), &mut result as *mut _ as isize);
        }
        result
    }

    #[test]
    #[ignore = "explicit native desktop overlay smoke test; displays hints without sending input"]
    fn native_overlay_is_nonactivating_clickthrough_and_capture_excluded() {
        assert!(supports_capture_exclusion());
        let foreground = unsafe { GetForegroundWindow() };
        update("smoke-a", true, None);
        std::thread::sleep(Duration::from_millis(250));
        let windows = screens();
        assert!(!windows.is_empty());
        for &hwnd in &windows {
            unsafe {
                let style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                assert_eq!(
                    style & (WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST),
                    WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST
                );
                let mut affinity = 0;
                assert_ne!(GetWindowDisplayAffinity(hwnd, &mut affinity), 0);
                assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE);
                assert_eq!(
                    SendMessageW(hwnd, WM_NCHITTEST, 0, 0),
                    HTTRANSPARENT as isize
                );
                assert_ne!(IsWindowVisible(hwnd), 0);
                let dc = GetDC(hwnd);
                assert_eq!(
                    GetPixel(dc, 1, 1),
                    ACCENT,
                    "the screen edge must be painted"
                );
                ReleaseDC(hwnd, dc);
            }
        }
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        update("smoke-b", true, None);
        update("smoke-a", false, None);
        unsafe {
            PostThreadMessageW(
                SERVICE.get().unwrap().as_ref().unwrap().thread,
                WAKE + 1,
                0,
                0,
            );
        }
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            screens().len(),
            windows.len(),
            "a second session still owns the indicator"
        );
        update("smoke-b", false, None);
        std::thread::sleep(Duration::from_millis(150));
        assert!(screens().is_empty());
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    }
}
