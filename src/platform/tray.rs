//! The icon in the notification area ("tray").
//!
//! Plain Win32 (`Shell_NotifyIconW`) on a thread of its own with a hidden
//! message window, instead of an extra crate: the icon keeps working while
//! the egui window is hidden, and nothing depends on the GUI event loop.
//!
//! The message window also lets a second start of AeternaVault bring the
//! running one to the front (see [`super::instance`]).

use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Click on the icon or "Open AeternaVault".
    Open,
    BackupNow,
    Quit,
    /// AeternaVault was started a second time.
    Activate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayLabels {
    pub tooltip: String,
    pub open: String,
    pub backup_now: String,
    pub quit: String,
}

pub type OnEvent = Arc<dyn Fn(TrayEvent) + Send + Sync>;

#[cfg(windows)]
pub use imp::Tray;

#[cfg(windows)]
pub(crate) use imp::{TRAY_CLASS, WM_APP_ACTIVATE};

#[cfg(not(windows))]
pub struct Tray;

#[cfg(not(windows))]
impl Tray {
    pub fn start(_key: &str, _labels: TrayLabels, _on_event: OnEvent) -> Option<Self> {
        None
    }
    pub fn set_visible(&self, _visible: bool) {}
    pub fn set_labels(&self, _labels: TrayLabels) {}
    pub fn balloon(&self, _title: &str, _text: &str, _warning: bool) {}
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::{Arc, Mutex, mpsc};

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{
        NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIIF_WARNING, NIM_ADD, NIM_DELETE,
        NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CW_USEDEFAULT, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
        DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos, GetMessageW,
        GetSystemMetrics, GetWindowLongPtrW, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR,
        LoadIconW, LoadImageW, MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage,
        RegisterClassW, RegisterWindowMessageW, SM_CXSMICON, SM_CYSMICON, SetForegroundWindow,
        SetWindowLongPtrW, TPM_BOTTOMALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
        TrackPopupMenu, TranslateMessage, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY,
        WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NCDESTROY, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
    };

    use super::{OnEvent, TrayEvent, TrayLabels};

    pub(crate) const TRAY_CLASS: &str = "AeternaVaultTrayWindow";
    const WM_APP_ICON: u32 = WM_APP + 1;
    const WM_APP_UPDATE: u32 = WM_APP + 2;
    pub(crate) const WM_APP_ACTIVATE: u32 = WM_APP + 3;
    const ICON_ID: u32 = 1;
    const MENU_OPEN: usize = 1;
    const MENU_BACKUP: usize = 2;
    const MENU_QUIT: usize = 3;

    pub(crate) fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn fill(target: &mut [u16], text: &str) {
        let units: Vec<u16> = text.encode_utf16().take(target.len() - 1).collect();
        target[..units.len()].copy_from_slice(&units);
        target[units.len()] = 0;
    }

    struct Inner {
        labels: Mutex<TrayLabels>,
        balloon: Mutex<Option<(String, String, bool)>>,
        want_visible: AtomicBool,
        added: AtomicBool,
        on_event: OnEvent,
        taskbar_created: u32,
    }

    pub struct Tray {
        inner: Arc<Inner>,
        hwnd: Arc<AtomicIsize>,
    }

    impl Tray {
        /// Creates the (still invisible) icon window. `key` names the window so
        /// a second instance can find it.
        pub fn start(key: &str, labels: TrayLabels, on_event: OnEvent) -> Option<Self> {
            let message_name = wide("TaskbarCreated");
            // SAFETY: the string is NUL-terminated and lives during the call.
            let taskbar_created = unsafe { RegisterWindowMessageW(message_name.as_ptr()) };
            let inner = Arc::new(Inner {
                labels: Mutex::new(labels),
                balloon: Mutex::new(None),
                want_visible: AtomicBool::new(false),
                added: AtomicBool::new(false),
                on_event,
                taskbar_created,
            });
            let hwnd = Arc::new(AtomicIsize::new(0));
            let (ready_tx, ready_rx) = mpsc::channel();
            let thread_inner = Arc::clone(&inner);
            let thread_hwnd = Arc::clone(&hwnd);
            let key = key.to_string();
            std::thread::Builder::new()
                .name("aeterna-tray".into())
                .spawn(move || {
                    let window = create_window(&key, thread_inner);
                    thread_hwnd.store(window as isize, Ordering::Relaxed);
                    let _ = ready_tx.send(!window.is_null());
                    if !window.is_null() {
                        message_loop();
                    }
                })
                .ok()?;
            match ready_rx.recv() {
                Ok(true) => Some(Self { inner, hwnd }),
                _ => {
                    tracing::warn!("the notification area icon could not be created");
                    None
                }
            }
        }

        fn post(&self, message: u32) {
            let hwnd = self.hwnd.load(Ordering::Relaxed);
            if hwnd != 0 {
                // SAFETY: posting to our own window; a destroyed window just fails.
                unsafe {
                    PostMessageW(hwnd as HWND, message, 0, 0);
                }
            }
        }

        pub fn set_visible(&self, visible: bool) {
            if self.inner.want_visible.swap(visible, Ordering::Relaxed) != visible {
                self.post(WM_APP_UPDATE);
            }
        }

        pub fn set_labels(&self, labels: TrayLabels) {
            let mut current = self.inner.labels.lock().unwrap_or_else(|e| e.into_inner());
            if *current != labels {
                *current = labels;
                drop(current);
                self.post(WM_APP_UPDATE);
            }
        }

        /// A short message bubble next to the icon (shown only if the icon is visible).
        pub fn balloon(&self, title: &str, text: &str, warning: bool) {
            *self.inner.balloon.lock().unwrap_or_else(|e| e.into_inner()) =
                Some((title.to_string(), text.to_string(), warning));
            self.post(WM_APP_UPDATE);
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            self.post(WM_CLOSE);
        }
    }

    fn create_window(key: &str, inner: Arc<Inner>) -> HWND {
        let class = wide(TRAY_CLASS);
        let title = wide(key);
        // SAFETY: standard window class registration and creation. All strings
        // are NUL-terminated and outlive the calls. The `Arc` handed to the
        // window is released again in WM_NCDESTROY.
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class.as_ptr(),
            };
            // Registering twice (e.g. after a restart of the thread) fails harmlessly.
            RegisterClassW(&wc);
            // A normal but never shown top-level window: message-only windows do
            // not receive the "TaskbarCreated" broadcast after Explorer restarts.
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                0,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if !hwnd.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Arc::into_raw(inner) as isize);
            }
            hwnd
        }
    }

    fn message_loop() {
        // SAFETY: standard message loop for windows created on this thread.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
        // SAFETY: NOTIFYICONDATAW is plain data; zero is a valid starting point.
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = ICON_ID;
        data
    }

    fn load_icon() -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
        // SAFETY: resource id 1 is the application icon embedded by build.rs;
        // without it the standard application icon is used.
        unsafe {
            // MAKEINTRESOURCEW(1): an integer resource id in place of a name.
            #[allow(clippy::manual_dangling_ptr)]
            let resource = 1usize as *const u16;
            let icon = LoadImageW(
                GetModuleHandleW(std::ptr::null()),
                resource,
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                LR_DEFAULTCOLOR,
            );
            if icon.is_null() {
                LoadIconW(std::ptr::null_mut(), IDI_APPLICATION)
            } else {
                icon
            }
        }
    }

    /// Brings the icon in line with what the window wants.
    fn sync(hwnd: HWND, inner: &Inner) {
        let want = inner.want_visible.load(Ordering::Relaxed);
        let added = inner.added.load(Ordering::Relaxed);
        let labels = inner
            .labels
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mut data = icon_data(hwnd);
        // SAFETY: `data` is fully initialised for the requested flags.
        unsafe {
            if want {
                data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
                data.uCallbackMessage = WM_APP_ICON;
                data.hIcon = load_icon();
                fill(&mut data.szTip, &labels.tooltip);
                let message = if added { NIM_MODIFY } else { NIM_ADD };
                if Shell_NotifyIconW(message, &data) != 0 {
                    inner.added.store(true, Ordering::Relaxed);
                }
            } else if added {
                Shell_NotifyIconW(NIM_DELETE, &data);
                inner.added.store(false, Ordering::Relaxed);
            }

            let balloon = inner
                .balloon
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if let Some((title, text, warning)) = balloon
                && inner.added.load(Ordering::Relaxed)
            {
                let mut info = icon_data(hwnd);
                info.uFlags = NIF_INFO;
                fill(&mut info.szInfoTitle, &title);
                fill(&mut info.szInfo, &text);
                info.dwInfoFlags = if warning { NIIF_WARNING } else { NIIF_INFO };
                Shell_NotifyIconW(NIM_MODIFY, &info);
            }
        }
    }

    fn show_menu(hwnd: HWND, inner: &Inner) {
        let labels = inner
            .labels
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let open = wide(&labels.open);
        let backup = wide(&labels.backup_now);
        let quit = wide(&labels.quit);
        // SAFETY: the menu is created, shown and destroyed within this call;
        // the label strings outlive it.
        let chosen = unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return;
            }
            AppendMenuW(menu, MF_STRING, MENU_OPEN, open.as_ptr());
            AppendMenuW(menu, MF_STRING, MENU_BACKUP, backup.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, MENU_QUIT, quit.as_ptr());
            let mut point = POINT { x: 0, y: 0 };
            GetCursorPos(&mut point);
            // Required so the menu closes when clicking elsewhere.
            SetForegroundWindow(hwnd);
            let chosen = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                point.x,
                point.y,
                0,
                hwnd,
                std::ptr::null(),
            );
            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);
            chosen as usize
        };
        match chosen {
            MENU_OPEN => (inner.on_event)(TrayEvent::Open),
            MENU_BACKUP => (inner.on_event)(TrayEvent::BackupNow),
            MENU_QUIT => (inner.on_event)(TrayEvent::Quit),
            _ => {}
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // SAFETY: GWLP_USERDATA holds the pointer from `Arc::into_raw` (or 0
        // before it was set); it stays valid until WM_NCDESTROY releases it.
        unsafe {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Inner;
            if ptr.is_null() {
                return DefWindowProcW(hwnd, message, wparam, lparam);
            }
            let inner = &*ptr;
            match message {
                WM_APP_ICON => {
                    match lparam as u32 {
                        WM_LBUTTONUP | WM_LBUTTONDBLCLK => (inner.on_event)(TrayEvent::Open),
                        WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd, inner),
                        _ => {}
                    }
                    0
                }
                WM_APP_UPDATE => {
                    sync(hwnd, inner);
                    0
                }
                WM_APP_ACTIVATE => {
                    (inner.on_event)(TrayEvent::Activate);
                    0
                }
                WM_CLOSE => {
                    inner.want_visible.store(false, Ordering::Relaxed);
                    sync(hwnd, inner);
                    DestroyWindow(hwnd);
                    0
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    0
                }
                WM_NCDESTROY => {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    drop(Arc::from_raw(ptr));
                    DefWindowProcW(hwnd, message, wparam, lparam)
                }
                m if m == inner.taskbar_created && inner.taskbar_created != 0 => {
                    // Explorer restarted: the icon has to be added again.
                    inner.added.store(false, Ordering::Relaxed);
                    sync(hwnd, inner);
                    0
                }
                _ => DefWindowProcW(hwnd, message, wparam, lparam),
            }
        }
    }
}
