//! YSC 8K 宿主核心。
//!
//! 这是旧 `ysc_driver` 二进制里与传输/窗口无关的部分：
//! 设备槽位、命令分发、IAP/iap2、kmbox、物理护栏、监控解析。
//!
//! 它只通过两个 trait 与宿主交互：
//! - [`HostSink`]：事件出口（旧管道 / Tauri event）。
//! - [`HostNotifier`]：串口失联、槽位失联、托盘提示等宿主通知。
//!
//! `ysc_driver`（旧命名管道二进制）与 `ysc_8k_tauri`（新 Tauri 单 exe）
//! 各自实现这两个 trait，核心逻辑只维护一份。

pub mod app;
pub mod command_bridge;
pub mod commands;
pub mod debug_log;
pub mod device;
pub mod guard;
pub mod host;
pub mod iap;
pub mod iap2;
pub mod kmbox;
pub mod monitor;
pub mod netutil;

pub use app::App;
pub use host::{HostNotifier, HostSink};
