use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use ysc_host::App;

pub struct TauriState {
    pub app: Arc<App>,
    /// 启动自动烧录模式时配置的 V3 镜像路径；None = 普通启动。
    pub auto_flash: Option<String>,
    /// 自动烧录的取消标志：UI 关掉开关 / 退出窗口时置位，循环下一轮退出。
    pub auto_cancel: Arc<AtomicBool>,
    /// 自动烧录线程是否还在等设备（供 UI 显示"等待插入"还是"已停止"）。
    pub auto_running: Arc<AtomicBool>,
}
