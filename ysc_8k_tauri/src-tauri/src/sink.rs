use tauri::{AppHandle, Emitter, Manager};
use ysc_core::util::build_event_line;
use ysc_host::HostSink;

/// Tauri 事件出口：把旧管道的逐行 JSON 原样投给 WebView。
///
/// 前端 `driver-api-shim.ts` 只需 `JSON.parse` 后按 `type` 分发，事件形状与旧管道一致。
pub struct TauriSink {
    app: AppHandle,
}

impl TauriSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl HostSink for TauriSink {
    fn emit(&self, event_type: &str, body: Option<&str>) {
        let line = build_event_line(event_type, body);
        let _ = self.app.emit("ysc:driver-event", line);
    }

    fn ui_attached(&self) -> bool {
        self.app.get_webview_window("main").is_some()
    }
}
