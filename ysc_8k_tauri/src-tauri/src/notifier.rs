use std::sync::mpsc::Sender;

use ysc_host::HostNotifier;

/// 宿主循环消息。核心线程只投递，不在串口读线程里直接动 Tauri 窗口。
pub enum HostMsg {
    SerialLost(u32),
    SlotLost(usize, u32),
    TrayTip(String),
}

pub struct TauriNotifier {
    tx: Sender<HostMsg>,
}

impl TauriNotifier {
    pub fn new(tx: Sender<HostMsg>) -> Self {
        Self { tx }
    }
}

impl HostNotifier for TauriNotifier {
    fn serial_lost(&self, gen: u32) {
        let _ = self.tx.send(HostMsg::SerialLost(gen));
    }

    fn slot_lost(&self, slot: usize, gen: u32) {
        let _ = self.tx.send(HostMsg::SlotLost(slot, gen));
    }

    fn set_tray_tip(&self, text: &str) {
        let _ = self.tx.send(HostMsg::TrayTip(text.to_string()));
    }
}
