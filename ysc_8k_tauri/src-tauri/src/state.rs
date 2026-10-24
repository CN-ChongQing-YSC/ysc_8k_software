use std::sync::Arc;

use ysc_host::App;

pub struct TauriState {
    pub app: Arc<App>,
    /// 启动自动烧录模式时配置的 V3 镜像路径；None = 普通启动。
    pub auto_flash: Option<String>,
}
