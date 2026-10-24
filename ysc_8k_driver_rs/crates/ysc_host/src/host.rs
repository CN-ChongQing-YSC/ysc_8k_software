//! 宿主抽象：事件出口与宿主通知。
//!
//! 旧 `ysc_driver` 用命名管道实现 `HostSink`，用隐藏窗口消息实现 `HostNotifier`；
//! Tauri 宿主用 `app.emit("ysc:driver-event", ...)` 与宿主循环实现同一组接口。

/// 事件出口。`body` 是不含外层大括号的 JSON 片段，与旧管道协议一致。
pub trait HostSink: Send + Sync {
    fn emit(&self, event_type: &str, body: Option<&str>);
    /// UI 是否已连接。旧管道看 pipe client；Tauri 看主窗口是否存在。
    fn ui_attached(&self) -> bool;
}

/// 宿主通知。核心不依赖 Win32 窗口/消息循环。
pub trait HostNotifier: Send + Sync {
    /// 串口读线程发现物理/IO 失联；`gen` 用于丢弃旧连接的回调。
    fn serial_lost(&self, gen: u32);
    /// 双设备槽位失联；`slot` 是 0/1。
    fn slot_lost(&self, slot: usize, gen: u32);
    /// 更新托盘提示（旧托盘 / Tauri 托盘）。
    fn set_tray_tip(&self, text: &str);
}
