//! 驱动运行时：全局状态与线程编排。
//!
//! 对应 C++ 的 `AppState` + 各静态单例（`g_serial` / `g_pipeServer` /
//! `g_kmServer` / `MonitorPush` / `HumanGuard` / `DeviceContext`）。
//!
//! 这里用 `Arc<App>` 串起来：串口读线程、管道线程、UDP 线程、UI 推送线程都持有
//! `Weak<App>`/`Arc<App>`，避免 C++ 里的全局裸指针。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ysc_core::consts::{Role, KMBOXNET_DEFAULT_PORT};
use ysc_core::enumerate::enum_com_ports;
use ysc_core::serial::SerialPort;
use ysc_core::util::{json_escape, now_ms};

use crate::debug_log::DEBUG;
use crate::host::{HostNotifier, HostSink};
use crate::kmbox::KmboxServer;

/// 设备槽位（对应 C++ `DeviceSlot`）。
pub struct Slot {
    pub serial: Option<Arc<SerialPort>>,
    pub port: String,
    pub baud: u32,
    pub connected: bool,
    /// 该槽位是 YSC V3 USB CDC：波特率是逻辑值，可任意设置且不影响 USB 吞吐。
    pub v3_cdc: bool,
    pub auto_detected: bool,
    pub role_manual: bool,
    pub role: Role,
    pub probe_state: i32,
    pub probe_done: bool,
    pub probe_deadline: u64,
    pub vid: i32,
    pub pid: i32,
    pub product: String,
    pub iface_mask: i32,
    pub saw_mouse_data: bool,
    pub saw_kbd_data: bool,
    pub probe_got_info: bool,
    pub probe_got_cfg: bool,
    pub probe_ambiguous: bool,
    pub probe_stage: u8,
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            serial: None,
            port: String::new(),
            baud: 0,
            connected: false,
            v3_cdc: false,
            auto_detected: false,
            role_manual: false,
            role: Role::None,
            probe_state: 0,
            probe_done: false,
            probe_deadline: 0,
            vid: -1,
            pid: -1,
            product: String::new(),
            iface_mask: 0,
            saw_mouse_data: false,
            saw_kbd_data: false,
            probe_got_info: false,
            probe_got_cfg: false,
            probe_ambiguous: false,
            probe_stage: 0,
        }
    }
}

impl Slot {
    pub fn in_use(&self) -> bool {
        self.serial.is_some()
    }
}

/// 单设备（旧接口）与 NET 服务状态。
pub struct Flags {
    pub serial_connected: bool,
    pub serial_port: String,
    pub serial_baud: u32,
    /// 单设备路径连的是 YSC V3 USB CDC：波特率仅作显示，可任意设置。
    pub serial_v3_cdc: bool,
    pub net_running: bool,
    pub net_port: u16,
}

impl Default for Flags {
    fn default() -> Self {
        Self {
            serial_connected: false,
            serial_port: String::new(),
            serial_baud: 0,
            serial_v3_cdc: false,
            net_running: false,
            net_port: KMBOXNET_DEFAULT_PORT,
        }
    }
}

/// 鼠标监控快照。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseMon {
    pub buttons: i32,
    pub x: i32,
    pub y: i32,
    pub wheel: i32,
    pub valid: i32,
}

/// 键盘物理状态快照。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KbdMon {
    pub mod_: i32,
    pub key_count: i32,
    pub overflow: i32,
    pub key: [u8; KBD_MON_KEYS_MAX],
    pub valid: i32,
}

pub const KBD_MON_KEYS_MAX: usize = 6;

impl Default for KbdMon {
    fn default() -> Self {
        Self {
            mod_: 0,
            key_count: 0,
            overflow: 0,
            key: [0; KBD_MON_KEYS_MAX],
            valid: 0,
        }
    }
}

/// 监控状态（鼠标 + 键盘 + 变化检测缓存）。
#[derive(Clone, Copy, Debug, Default)]
pub struct MonitorState {
    pub mouse: MouseMon,
    pub kbd: KbdMon,
    pub last_mouse: MouseMon,
    pub last_kbd: KbdMon,
    pub have_last: bool,
}

/// 单轴校准统计（对应 C++ `AxisStat`）。
#[derive(Clone)]
pub struct AxisStat {
    pub hist: [u32; HG_MAX_MS as usize + 1],
    pub count: u32,
    pub min_ms: u32,
    pub last_ms: u32,
    pub dir: i32,
    pub rev_t: u32,
    pub last_t: u32,
    pub opp_acc: i32,
    pub recent: [u32; HG_RECENT_MAX as usize],
    pub recent_count: u32,
}

pub const HG_MIN_MS: u32 = 5;
pub const HG_MAX_MS: u32 = 500;
pub const HG_RECENT_MAX: u32 = 32;

impl Default for AxisStat {
    fn default() -> Self {
        Self {
            hist: [0; HG_MAX_MS as usize + 1],
            count: 0,
            min_ms: 0,
            last_ms: 0,
            dir: 0,
            rev_t: 0,
            last_t: 0,
            opp_acc: 0,
            recent: [0; HG_RECENT_MAX as usize],
            recent_count: 0,
        }
    }
}

/// 物理护栏校准状态。
pub struct GuardState {
    pub active: bool,
    pub stop_requested: bool,
    pub x: AxisStat,
    pub y: AxisStat,
    pub axis: char,
    pub duration: i32,
    pub start_ms: u64,
    pub last_progress_ms: u64,
    pub session: u32,
    pub frames: u32,
    pub saw_t: bool,
    pub saw_no_t: bool,
    pub last_result: Option<String>,
}

impl Default for GuardState {
    fn default() -> Self {
        Self {
            active: false,
            stop_requested: false,
            x: AxisStat::default(),
            y: AxisStat::default(),
            axis: 'x',
            duration: 5000,
            start_ms: 0,
            last_progress_ms: 0,
            session: 0,
            frames: 0,
            saw_t: false,
            saw_no_t: false,
            last_result: None,
        }
    }
}

/// 驱动运行时。
pub struct App {
    pub sink: Arc<dyn HostSink>,
    pub notifier: Arc<dyn HostNotifier>,
    pub kmbox: KmboxServer,
    pub legacy: Arc<SerialPort>,
    pub slots: [Mutex<Slot>; 2],
    pub flags: Mutex<Flags>,
    pub monitor: Mutex<MonitorState>,
    pub ui_wake: Mutex<Option<SyncSender<()>>>,
    pub guard: Mutex<GuardState>,
    pub guard_stop: AtomicBool,
    pub net_ip: Mutex<String>,
    pub last_probe_tick: AtomicU64,
    pub iap_running: AtomicBool,
    pub iap_cancel: AtomicBool,
    pub iap2_running: AtomicBool,
    pub iap2_cancel: AtomicBool,
    /// PC 手柄测试器打开时置位；Tauri 宿主的 XInput 轮询线程据此推事件。
    pub gamepad_watch: AtomicBool,
    pub shutting_down: AtomicBool,
}

impl App {
    pub fn new(sink: Arc<dyn HostSink>, notifier: Arc<dyn HostNotifier>) -> Arc<Self> {
        Arc::new(Self {
            sink,
            notifier,
            kmbox: KmboxServer::new(),
            legacy: Arc::new(SerialPort::new()),
            slots: [Mutex::new(Slot::default()), Mutex::new(Slot::default())],
            flags: Mutex::new(Flags::default()),
            monitor: Mutex::new(MonitorState::default()),
            ui_wake: Mutex::new(None),
            guard: Mutex::new(GuardState::default()),
            guard_stop: AtomicBool::new(false),
            net_ip: Mutex::new(String::new()),
            last_probe_tick: AtomicU64::new(0),
            iap_running: AtomicBool::new(false),
            iap_cancel: AtomicBool::new(false),
            iap2_running: AtomicBool::new(false),
            iap2_cancel: AtomicBool::new(false),
            gamepad_watch: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
        })
    }

    /// 启动与宿主无关的后台线程（串口回调/监控推送/物理护栏/kmbox）。
    /// 旧管道主程序在调用本方法后再启动 `PipeServer`；Tauri 宿主直接调用本方法。
    pub fn start(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        self.legacy.set_callback(Arc::new(move |data: &[u8]| {
            if let Some(app) = weak.upgrade() {
                app.on_serial_response_combo(data);
            }
        }));

        let weak = Arc::downgrade(self);
        self.legacy.set_lost_callback(Arc::new(move |gen: u32| {
            if let Some(app) = weak.upgrade() {
                app.notifier.serial_lost(gen);
            }
        }));

        self.kmbox.set_app(Arc::downgrade(self));

        // UI 监控推送线程（8ms 限速，变化即推）
        // 容量 1：8K 帧率下只保留“还有一次待推送”的信号，与 C++ 自动复位事件等价。
        let (tx, rx) = mpsc::sync_channel::<()>(1);
        *self.ui_wake.lock().unwrap() = Some(tx);
        let weak = Arc::downgrade(self);
        let _ = thread::Builder::new()
            .name("ysc-ui-push".into())
            .spawn(move || {
                let mut last_push = 0u64;
                loop {
                    match rx.recv_timeout(Duration::from_millis(200)) {
                        Ok(()) => {}
                        // 超时只是“当前没有新状态”，线程必须继续等下一次变化。
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    let Some(app) = weak.upgrade() else { break };
                    if app.shutting_down.load(Ordering::Acquire) {
                        break;
                    }
                    let dt = now_ms().saturating_sub(last_push);
                    if dt < 8 {
                        thread::sleep(Duration::from_millis(8 - dt));
                    }
                    if app.shutting_down.load(Ordering::Acquire) {
                        break;
                    }
                    last_push = now_ms();
                    if app.ui_attached() {
                        let body = app.serialize_ui_json();
                        app.emit("monitor_data", Some(&body));
                    }
                }
            });

        // 物理护栏工作线程（50ms 心跳，进度 100ms）
        let weak = Arc::downgrade(self);
        let _ = thread::Builder::new()
            .name("ysc-human-guard".into())
            .spawn(move || loop {
                thread::sleep(Duration::from_millis(50));
                let Some(app) = weak.upgrade() else { break };
                if app.guard_stop.load(Ordering::Acquire) {
                    break;
                }
                app.guard_tick();
            });

        DEBUG.log("=== YSC 8K Driver (Rust) started ===");
    }

    pub fn ui_attached(&self) -> bool {
        self.sink.ui_attached()
    }

    pub fn emit(&self, event_type: &str, body: Option<&str>) {
        self.sink.emit(event_type, body);
    }

    /// 兼容旧代码的事件发送入口。
    pub fn send_event(&self, event_type: &str, body: Option<&str>) {
        self.emit(event_type, body);
    }

    /// 串口失联后的清理（原 `tray::handle_legacy_lost`）。
    pub fn handle_legacy_lost(&self, gen: u32) {
        let connected = self.flags.lock().unwrap().serial_connected;
        if connected && self.legacy.reader_gen() == gen {
            self.log("SERIAL unexpected disconnect (physical/IO), cleaning up");
            self.guard_abort("device_lost");
            self.legacy.disconnect();
            self.flags.lock().unwrap().serial_connected = false;
            self.update_tray_tip();
            self.emit("serial_disconnected", None);
        }
    }

    /// 托盘提示文本（原 `tray::update_tray_tip` 的文本构造部分）。
    pub fn tray_tip(&self) -> String {
        let f = self.flags.lock().unwrap();
        let mut tip = String::from("YSC 8K Driver");
        if f.serial_connected {
            tip.push_str(&format!(" - {} @ {}", f.serial_port, f.serial_baud));
            if f.serial_v3_cdc {
                tip.push_str(" (USB CDC)");
            }
        } else {
            tip.push_str(" - 未连接");
        }
        if f.net_running {
            tip.push_str(&format!(" | Net:{}", f.net_port));
        }
        tip
    }

    pub fn update_tray_tip(&self) {
        self.notifier.set_tray_tip(&self.tray_tip());
    }

    /// 枚举 COM 口并广播 `ports_changed` + `ports_list`（旧托盘定时器 / Tauri 轮询共用）。
    pub fn emit_ports(&self) {
        let ports = enum_com_ports();
        let mut body = String::from("\"ports\":[");
        for (i, p) in ports.iter().enumerate() {
            if i > 0 {
                body.push(',');
            }
            body.push_str(&format!("\"{}\"", json_escape(p)));
        }
        body.push(']');
        self.emit("ports_changed", Some(&body));
        self.emit("ports_list", Some(&body));
    }

    pub fn log(&self, msg: &str) {
        DEBUG.log(msg);
    }

    pub fn debug_window_log(&self, msg: &str) {
        DEBUG.debug_window_log(msg);
    }

    #[cfg(test)]
    pub fn test_app() -> Arc<App> {
        struct TestSink;
        impl HostSink for TestSink {
            fn emit(&self, _event_type: &str, _body: Option<&str>) {}
            fn ui_attached(&self) -> bool {
                true
            }
        }
        struct TestNotifier;
        impl HostNotifier for TestNotifier {
            fn serial_lost(&self, _gen: u32) {}
            fn slot_lost(&self, _slot: usize, _gen: u32) {}
            fn set_tray_tip(&self, _text: &str) {}
        }
        App::new(Arc::new(TestSink), Arc::new(TestNotifier))
    }

    /// 唤醒 UI 推送线程（等价 C++ `SetEvent(s_hUiWake)`）。
    pub fn wake_ui(&self) {
        if let Some(tx) = self.ui_wake.lock().unwrap().as_ref() {
            let _ = tx.try_send(());
        }
    }

    pub fn on_serial_response_combo(&self, data: &[u8]) {
        self.monitor_on_serial(data);
        self.guard_on_frame(data);
        self.forward_debug_response(data);
    }

    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        self.guard_stop.store(true, Ordering::Release);
        self.guard_shutdown();
        self.wake_ui();
        self.detach_all();
        self.kmbox.stop();
        self.legacy.disconnect();
        DEBUG.shutdown();
    }

    /// 供命令层读取单设备状态。
    pub fn serial_snapshot(&self) -> (bool, String, u32, bool) {
        let f = self.flags.lock().unwrap();
        (
            f.serial_connected,
            f.serial_port.clone(),
            f.serial_baud,
            f.serial_v3_cdc,
        )
    }
}
