//! 托盘图标、隐藏窗口、调试窗口、热插拔通知与消息循环。
//!
//! 对齐 `main.cpp` 的 `wWinMain` / `WndProc` / `ShowTrayMenu` / `HandleMenuCommand`。

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};

use once_cell::sync::OnceCell;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, UpdateWindow, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, FF_DONTCARE, FIXED_PITCH,
    FW_NORMAL, OUT_DEFAULT_PRECIS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{EM_REPLACESEL, EM_SETLIMITTEXT, EM_SETSEL};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, GetWindowTextLengthW, KillTimer, LoadCursorW,
    LoadIconW, MessageBoxW, MoveWindow, PostMessageW, PostQuitMessage, RegisterClassExW,
    RegisterDeviceNotificationW, SendMessageW, SetForegroundWindow, SetTimer, ShowWindow,
    TrackPopupMenu, TranslateMessage, UnregisterDeviceNotification, CW_USEDEFAULT,
    DEVICE_NOTIFY_WINDOW_HANDLE, DEV_BROADCAST_DEVICEINTERFACE_W, ES_AUTOHSCROLL, ES_AUTOVSCROLL,
    ES_MULTILINE, ES_READONLY, HDEVNOTIFY, HMENU, IDC_ARROW, IDI_APPLICATION, MB_ICONERROR, MB_OK,
    MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MSG, SW_HIDE, SW_SHOW,
    TPM_BOTTOMALIGN, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_DESTROY,
    WM_DEVICECHANGE, WM_NULL, WM_SETFONT, WM_SIZE, WM_TIMER, WNDCLASSEXW, WS_CAPTION, WS_CHILD,
    WS_HSCROLL, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SIZEBOX, WS_SYSMENU, WS_VISIBLE, WS_VSCROLL,
};

use ysc_core::consts::SUPPORTED_BAUDS;
use ysc_core::enumerate::enum_com_ports;
use ysc_core::serial::SerialPort;
use ysc_core::util::{json_escape, wide};
use ysc_host::app::App;
use ysc_host::debug_log::{DEBUG, WM_DEBUG_LOG};
use ysc_host::kmbox;
use ysc_host::HostNotifier;

/// 读线程异常退出通知（wParam = 世代号）。
pub const WM_SERIAL_LOST: u32 = 0x0400 + 3; // WM_USER + 3
/// 多设备槽位读线程异常退出（wParam = slot，lParam = 世代号）。
pub const WM_SLOT_LOST: u32 = 0x0400 + 4;

pub const WM_TRAYICON: u32 = 0x0400 + 1; // WM_USER + 1

const TIMER_PORT_CHANGE: usize = 1001;
const TIMER_DEVICE_PROBE: usize = 1002;
const PORT_CHANGE_DEBOUNCE_MS: u32 = 400;
const DEVICE_PROBE_MS: u32 = 100;

const IDM_DISCONNECT_SERIAL: usize = 2001;
const IDM_NET_SERVICE: usize = 2002;
const IDM_EXIT: usize = 2006;
const IDM_DEBUG_LOG: usize = 2007;
const IDM_COM_PORT_BASE: usize = 3000;
const IDM_BAUDRATE_BASE: usize = 4000;

const DBT_DEVICEARRIVAL: usize = 0x8000;
const DBT_DEVICEREMOVECOMPLETE: usize = 0x8004;
const DBT_DEVTYP_DEVICEINTERFACE: u32 = 0x0000_0005;

static APP: OnceCell<Arc<App>> = OnceCell::new();
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static DEBUG_HWND: Mutex<isize> = Mutex::new(0);
static DEBUG_VISIBLE: AtomicBool = AtomicBool::new(false);
static DEV_NOTIFY: Mutex<isize> = Mutex::new(0);

fn main_hwnd() -> HWND {
    HWND(MAIN_HWND.load(Ordering::Acquire) as *mut c_void)
}

/// 旧 Win32 宿主的通知实现：串口/槽位失联投递窗口消息，托盘提示走 Shell_NotifyIconW。
pub struct Win32Notifier {
    hwnd: AtomicIsize,
}

impl Win32Notifier {
    pub fn new() -> Self {
        Self {
            hwnd: AtomicIsize::new(0),
        }
    }

    pub fn set_hwnd(&self, hwnd: HWND) {
        self.hwnd.store(hwnd.0 as isize, Ordering::Release);
    }

    fn hwnd(&self) -> HWND {
        HWND(self.hwnd.load(Ordering::Acquire) as *mut c_void)
    }
}

impl Default for Win32Notifier {
    fn default() -> Self {
        Self::new()
    }
}

impl HostNotifier for Win32Notifier {
    fn serial_lost(&self, gen: u32) {
        let hwnd = self.hwnd();
        if !hwnd.0.is_null() {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_SERIAL_LOST, WPARAM(gen as usize), LPARAM(0));
            }
        }
    }

    fn slot_lost(&self, slot: usize, gen: u32) {
        let hwnd = self.hwnd();
        if !hwnd.0.is_null() {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_SLOT_LOST, WPARAM(slot), LPARAM(gen as isize));
            }
        }
    }

    fn set_tray_tip(&self, text: &str) {
        let hwnd = self.hwnd();
        if hwnd.0.is_null() {
            return;
        }
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_TIP;
        set_tip_text(&mut nid, text);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }
}

/// 启动消息循环（阻塞到退出）。
pub fn run(app: Arc<App>, notifier: Arc<Win32Notifier>, hinstance: HINSTANCE) -> i32 {
    let _ = APP.set(Arc::clone(&app));

    // 主隐藏窗口类。
    let main_class = wide("Ysc8kDriverClass");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        lpszClassName: PCWSTR(main_class.as_ptr()),
        ..Default::default()
    };
    unsafe {
        RegisterClassExW(&wc);
    }

    // 调试窗口类。
    let dbg_class = wide("YscDebugClass");
    let dwc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(debug_wndproc),
        hInstance: hinstance,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        lpszClassName: PCWSTR(dbg_class.as_ptr()),
        ..Default::default()
    };
    unsafe {
        RegisterClassExW(&dwc);
    }

    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(main_class.as_ptr()),
            w!("YSC 8K Driver"),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            400,
            300,
            None,
            None,
            Some(hinstance),
            None,
        )
    };
    let Ok(hwnd) = hwnd else {
        return 1;
    };
    MAIN_HWND.store(hwnd.0 as isize, Ordering::Release);
    notifier.set_hwnd(hwnd);
    register_device_notification(hwnd);
    add_tray_icon(hwnd);

    unsafe {
        SetTimer(Some(hwnd), TIMER_DEVICE_PROBE, DEVICE_PROBE_MS, None);
    }

    let mut msg = MSG::default();
    loop {
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // 退出清理。
    app.shutdown();
    remove_tray_icon(hwnd);
    if let Some(h) = DEBUG_HWND.lock().unwrap().to_owned().checked_abs() {
        if h != 0 {
            unsafe {
                let _ = DestroyWindow(HWND(h as *mut c_void));
            }
        }
    }
    let dev = *DEV_NOTIFY.lock().unwrap();
    if dev != 0 {
        unsafe {
            let _ = UnregisterDeviceNotification(HDEVNOTIFY(dev as *mut c_void));
        }
    }
    let _ = &app;
    msg.wParam.0 as i32
}

// ---------------------------------------------------------------------------
// 窗口过程
// ---------------------------------------------------------------------------

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(app) = APP.get().cloned() else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };
    match msg {
        WM_TRAYICON => {
            if (lparam.0 & 0xffff) as u32 == windows::Win32::UI::WindowsAndMessaging::WM_RBUTTONUP {
                show_tray_menu(&app, hwnd);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            handle_menu_command(&app, (wparam.0 & 0xffff) as usize);
            LRESULT(0)
        }
        WM_DEBUG_LOG => {
            let ptr = lparam.0 as *mut String;
            if !ptr.is_null() {
                let text = *Box::from_raw(ptr);
                append_debug_text(&text);
            }
            LRESULT(0)
        }
        WM_SERIAL_LOST => {
            let gen = wparam.0 as u32;
            app.handle_legacy_lost(gen);
            LRESULT(0)
        }
        WM_SLOT_LOST => {
            let slot = wparam.0;
            let gen = lparam.0 as u32;
            app.handle_slot_lost(slot, gen);
            LRESULT(0)
        }
        WM_DEVICECHANGE => {
            if wparam.0 == DBT_DEVICEARRIVAL || wparam.0 == DBT_DEVICEREMOVECOMPLETE {
                unsafe {
                    let _ = KillTimer(Some(hwnd), TIMER_PORT_CHANGE);
                    SetTimer(Some(hwnd), TIMER_PORT_CHANGE, PORT_CHANGE_DEBOUNCE_MS, None);
                }
            }
            LRESULT(1)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_PORT_CHANGE {
                unsafe {
                    let _ = KillTimer(Some(hwnd), TIMER_PORT_CHANGE);
                }
                let ports = enum_com_ports();
                let body = ports_body(&ports);
                app.send_event("ports_changed", Some(&body));
                app.send_event("ports_list", Some(&body));
            } else if wparam.0 == TIMER_DEVICE_PROBE {
                app.probe_tick();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let dev = *DEV_NOTIFY.lock().unwrap();
            if dev != 0 {
                unsafe {
                    let _ = UnregisterDeviceNotification(HDEVNOTIFY(dev as *mut c_void));
                }
                *DEV_NOTIFY.lock().unwrap() = 0;
            }
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn ports_body(ports: &[String]) -> String {
    let mut body = String::from("\"ports\":[");
    for (i, p) in ports.iter().enumerate() {
        if i > 0 {
            body.push(',');
        }
        body.push_str(&format!("\"{}\"", json_escape(p)));
    }
    body.push(']');
    body
}

// ---------------------------------------------------------------------------
// 托盘菜单
// ---------------------------------------------------------------------------

fn add_tray_icon(hwnd: HWND) {
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAYICON;
    nid.hIcon = unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default();
    set_tip_text(&mut nid, "YSC 8K Driver - 未连接");
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    }
}

fn remove_tray_icon(hwnd: HWND) {
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

fn set_tip_text(nid: &mut NOTIFYICONDATAW, text: &str) {
    let w = wide(text);
    let n = w.len().min(nid.szTip.len());
    nid.szTip[..n].copy_from_slice(&w[..n]);
    if n < nid.szTip.len() {
        nid.szTip[n] = 0;
    }
}

/// 刷新托盘 tooltip（C++ `UpdateTrayTip`）。
fn menu_append(menu: HMENU, flags: u32, id: usize, text: &str) {
    let w = wide(text);
    let _ = unsafe {
        AppendMenuW(
            menu,
            windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS(flags),
            id,
            PCWSTR(w.as_ptr()),
        )
    };
}

fn show_tray_menu(app: &App, hwnd: HWND) {
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else {
        return;
    };

    let (connected, port, baud) = app.serial_snapshot();
    if connected {
        menu_append(
            menu,
            MF_STRING.0,
            IDM_DISCONNECT_SERIAL,
            &format!("断开串口 ({} @ {})", port, baud),
        );
        let Ok(baud_menu) = (unsafe { CreatePopupMenu() }) else {
            return;
        };
        for (i, &b) in SUPPORTED_BAUDS.iter().enumerate() {
            let flags = if b == baud {
                MF_STRING.0 | MF_CHECKED.0
            } else {
                MF_STRING.0
            };
            menu_append(baud_menu, flags, IDM_BAUDRATE_BASE + i, &b.to_string());
        }
        let _ = unsafe {
            AppendMenuW(
                menu,
                MF_POPUP,
                baud_menu.0 as usize,
                windows::core::PCWSTR(wide("切换波特率").as_ptr()),
            )
        };
    } else {
        let ports = enum_com_ports();
        if ports.is_empty() {
            menu_append(menu, MF_STRING.0 | MF_GRAYED.0, 0, "无可用串口");
        } else {
            let Ok(com_menu) = (unsafe { CreatePopupMenu() }) else {
                return;
            };
            for (i, p) in ports.iter().enumerate() {
                menu_append(com_menu, MF_STRING.0, IDM_COM_PORT_BASE + i, p);
            }
            let _ = unsafe {
                AppendMenuW(
                    menu,
                    MF_POPUP,
                    com_menu.0 as usize,
                    windows::core::PCWSTR(wide("连接串口").as_ptr()),
                )
            };
        }
    }

    menu_append(menu, MF_SEPARATOR.0, 0, "");
    let (net_running, net_port) = {
        let f = app.flags.lock().unwrap();
        (f.net_running, f.net_port)
    };
    let net_flags = if net_running {
        MF_STRING.0 | MF_CHECKED.0
    } else {
        MF_STRING.0
    };
    let net_label = if net_running {
        format!("Net 服务 ({}:{})", kmbox::get_local_ip(), net_port)
    } else {
        "Net 服务".to_string()
    };
    menu_append(menu, net_flags, IDM_NET_SERVICE, &net_label);

    menu_append(menu, MF_SEPARATOR.0, 0, "");
    let log_flags = if DEBUG.is_window_open() {
        MF_STRING.0 | MF_CHECKED.0
    } else {
        MF_STRING.0
    };
    menu_append(menu, log_flags, IDM_DEBUG_LOG, "调试窗口");
    menu_append(menu, MF_SEPARATOR.0, 0, "");
    menu_append(menu, MF_STRING.0, IDM_EXIT, "退出");

    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}

fn handle_menu_command(app: &App, id: usize) {
    if (IDM_COM_PORT_BASE..IDM_COM_PORT_BASE + 100).contains(&id) {
        let index = id - IDM_COM_PORT_BASE;
        let ports = enum_com_ports();
        if let Some(port) = ports.get(index) {
            let baud = SerialPort::detect_baudrate(port);
            if baud == 0 {
                debug_window_msg(app, "DetectBaudrate FAILED");
                message_box(
                    main_hwnd(),
                    "无法检测到设备\n请确认设备已连接且上电",
                    "错误",
                );
                return;
            }
            if app.legacy.connect(port, baud).is_ok() {
                app.debug_window_log(&format!("Serial CONNECTED {} @ {}", port, baud));
                app.log(&format!("SERIAL CONNECTED {} @ {}", port, baud));
                {
                    let mut f = app.flags.lock().unwrap();
                    f.serial_port = port.clone();
                    f.serial_baud = baud;
                    f.serial_connected = true;
                }
                let _ = app.legacy.start_read_thread();
                app.update_tray_tip();
                app.send_event(
                    "serial_connected",
                    Some(&format!(
                        "\"port\":\"{}\",\"baud\":{}",
                        json_escape(port),
                        baud
                    )),
                );
            } else {
                message_box(main_hwnd(), "无法打开串口", "错误");
            }
        }
        return;
    }

    if (IDM_BAUDRATE_BASE..=IDM_BAUDRATE_BASE + 8).contains(&id) {
        let index = id - IDM_BAUDRATE_BASE;
        let connected = app.flags.lock().unwrap().serial_connected;
        if index < SUPPORTED_BAUDS.len() && connected {
            let new_baud = SUPPORTED_BAUDS[index];
            let cur = app.flags.lock().unwrap().serial_baud;
            if new_baud != cur {
                if app.legacy.switch_baudrate(new_baud) {
                    {
                        let mut f = app.flags.lock().unwrap();
                        f.serial_baud = new_baud;
                        f.serial_connected = true;
                    }
                    let _ = app.legacy.start_read_thread();
                    app.update_tray_tip();
                    app.send_event("baudrate_switched", Some(&format!("\"baud\":{}", new_baud)));
                } else {
                    app.flags.lock().unwrap().serial_connected = false;
                    app.update_tray_tip();
                    app.send_event(
                        "baudrate_failed",
                        Some("\"message\":\"切换失败，设备可能已断开\""),
                    );
                    message_box(main_hwnd(), "波特率切换失败\n设备可能已断开", "错误");
                }
            }
        }
        return;
    }

    match id {
        IDM_DISCONNECT_SERIAL => {
            app.guard_abort("device_lost");
            app.legacy.disconnect();
            app.flags.lock().unwrap().serial_connected = false;
            app.log("SERIAL DISCONNECTED");
            app.update_tray_tip();
            app.send_event("serial_disconnected", None);
        }
        IDM_NET_SERVICE => {
            let running = app.flags.lock().unwrap().net_running;
            if running {
                app.debug_window_log("Net service STOP");
                app.log("NET SERVICE STOP");
                app.kmbox.stop();
                app.flags.lock().unwrap().net_running = false;
                app.send_event("kmnet_stopped", None);
            } else {
                let port = app.flags.lock().unwrap().net_port;
                if app.kmbox.start(port) {
                    app.debug_window_log(&format!("Net service START on port {}", port));
                    app.log(&format!("NET SERVICE START on port {}", port));
                    app.flags.lock().unwrap().net_running = true;
                    let ip = kmbox::get_local_ip();
                    app.send_event(
                        "kmnet_started",
                        Some(&format!(
                            "\"port\":{},\"ip\":\"{}\",\"mac\":\"{:08X}\"",
                            port,
                            json_escape(&ip),
                            ysc_core::consts::DEFAULT_MAC
                        )),
                    );
                } else {
                    message_box(main_hwnd(), "Net 服务启动失败", "错误");
                }
            }
            app.update_tray_tip();
        }
        IDM_DEBUG_LOG => toggle_debug_window(app),
        IDM_EXIT => unsafe {
            let _ = DestroyWindow(main_hwnd());
        },
        _ => {}
    }
}

fn debug_window_msg(app: &App, msg: &str) {
    app.debug_window_log(msg);
}

fn message_box(hwnd: HWND, text: &str, caption: &str) {
    let t = wide(text);
    let c = wide(caption);
    unsafe {
        MessageBoxW(
            Some(hwnd),
            PCWSTR(t.as_ptr()),
            PCWSTR(c.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

// ---------------------------------------------------------------------------
// 调试窗口
// ---------------------------------------------------------------------------

pub fn toggle_debug_window(app: &App) {
    let h = *DEBUG_HWND.lock().unwrap();
    if DEBUG_VISIBLE.load(Ordering::Acquire) {
        DEBUG_VISIBLE.store(false, Ordering::Release);
        DEBUG.set_window_open(false);
        if h != 0 {
            unsafe {
                let _ = ShowWindow(HWND(h as *mut c_void), SW_HIDE);
            }
        }
        return;
    }
    if h != 0 {
        DEBUG_VISIBLE.store(true, Ordering::Release);
        DEBUG.set_window_open(true);
        unsafe {
            let _ = ShowWindow(HWND(h as *mut c_void), SW_SHOW);
            let _ = SetForegroundWindow(HWND(h as *mut c_void));
        }
        return;
    }

    let hinstance = HINSTANCE(
        unsafe { GetModuleHandleW(None) }
            .map(|m| m.0)
            .unwrap_or(std::ptr::null_mut()),
    );
    let Ok(hwnd) = (unsafe {
        CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WS_EX_APPWINDOW,
            w!("YscDebugClass"),
            w!("YSC 8K Driver - 调试窗口"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_SIZEBOX | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            700,
            500,
            None,
            None,
            Some(hinstance),
            None,
        )
    }) else {
        return;
    };
    let Ok(edit) = (unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            w!(""),
            WS_CHILD
                | WS_VISIBLE
                | WS_VSCROLL
                | WS_HSCROLL
                | WINDOW_STYLE(
                    (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL | ES_AUTOHSCROLL) as u32,
                ),
            0,
            0,
            700,
            500,
            Some(hwnd),
            None,
            Some(hinstance),
            None,
        )
    }) else {
        return;
    };
    unsafe {
        SendMessageW(
            edit,
            EM_SETLIMITTEXT,
            Some(WPARAM(0x7FFF_FFFE)),
            Some(LPARAM(0)),
        );
        let font = CreateFontW(
            -13,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            windows::Win32::Graphics::Gdi::DEFAULT_QUALITY,
            FIXED_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
            w!("Consolas"),
        );
        if !font.0.is_null() {
            SendMessageW(
                edit,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
        let _ = UpdateWindow(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    *DEBUG_HWND.lock().unwrap() = hwnd.0 as isize;
    DEBUG.clear_pending();
    DEBUG.set_hwnd(hwnd);
    DEBUG_VISIBLE.store(true, Ordering::Release);
    DEBUG.set_window_open(true);
    app.debug_window_log("=== Debug window opened ===");
}

unsafe extern "system" fn debug_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_SIZE => {
            let edit = find_edit(hwnd);
            if !edit.0.is_null() {
                let w = (lparam.0 & 0xffff) as i32;
                let h = ((lparam.0 >> 16) & 0xffff) as i32;
                unsafe {
                    let _ = MoveWindow(edit, 0, 0, w, h, true);
                }
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_CLOSE => {
            DEBUG_VISIBLE.store(false, Ordering::Release);
            DEBUG.set_window_open(false);
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            LRESULT(0)
        }
        WM_DEBUG_LOG => {
            for line in DEBUG.drain_pending() {
                append_debug_text(&line);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn find_edit(parent: HWND) -> HWND {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindow, GW_CHILD};
    unsafe { GetWindow(parent, GW_CHILD).unwrap_or_default() }
}

fn append_debug_text(text: &str) {
    let h = *DEBUG_HWND.lock().unwrap();
    if h == 0 {
        return;
    }
    let hwnd = HWND(h as *mut c_void);
    let edit = find_edit(hwnd);
    if edit.0.is_null() {
        return;
    }
    unsafe {
        let len = GetWindowTextLengthW(edit);
        if len > 65536 {
            SendMessageW(
                edit,
                EM_SETSEL,
                Some(WPARAM(0)),
                Some(LPARAM((len / 2) as isize)),
            );
            let empty = wide("");
            SendMessageW(
                edit,
                EM_REPLACESEL,
                Some(WPARAM(0)),
                Some(LPARAM(empty.as_ptr() as isize)),
            );
        }
        let new_len = GetWindowTextLengthW(edit);
        SendMessageW(
            edit,
            EM_SETSEL,
            Some(WPARAM(new_len as usize)),
            Some(LPARAM(new_len as isize)),
        );
        let w = wide(text);
        SendMessageW(
            edit,
            EM_REPLACESEL,
            Some(WPARAM(0)),
            Some(LPARAM(w.as_ptr() as isize)),
        );
    }
}

// ---------------------------------------------------------------------------
// 设备接口变更通知
// ---------------------------------------------------------------------------

fn register_device_notification(hwnd: HWND) {
    let guid = windows::core::GUID::from_values(
        0x86e0_d1e0,
        0x8089,
        0x11d0,
        [0x9c, 0xe4, 0x08, 0x00, 0x3e, 0x30, 0x1f, 0x73],
    );
    let mut filter = DEV_BROADCAST_DEVICEINTERFACE_W::default();
    filter.dbcc_size = std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32;
    filter.dbcc_devicetype = DBT_DEVTYP_DEVICEINTERFACE;
    filter.dbcc_classguid = guid;
    let r = unsafe {
        RegisterDeviceNotificationW(
            windows::Win32::Foundation::HANDLE(hwnd.0),
            &filter as *const _ as *const c_void,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        )
    };
    match r {
        Ok(dev) => *DEV_NOTIFY.lock().unwrap() = dev.0 as isize,
        Err(_) => DEBUG.log("RegisterDeviceNotification failed (hotplug events disabled)"),
    }
}

// ---------------------------------------------------------------------------
// 串口失联
// ---------------------------------------------------------------------------
