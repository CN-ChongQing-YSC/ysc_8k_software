//! 启动自动烧录：参考 `rustXFDeriver` 的 `iap_flash auto` 端口自动选择思路，
//! 复用 YSC 已验证的 `ysc_host::iap2::start_file`，不重写 IAP 状态机。
//!
//! 只有在显式启用时才运行（exe 同目录存在 `v3_firmware.bin`，或传
//! `--auto-flash-v3=<path>`）；普通启动完全不受影响。

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ysc_core::enumerate::enum_towmcu_ports;
use ysc_core::util::json_escape;
use ysc_host::App;

const POLL_MS: u64 = 1000;

fn log(app: &App, msg: &str, cls: &str) {
    app.send_event(
        "iap2_log",
        Some(&format!(
            "\"message\":\"{}\",\"cls\":\"{}\"",
            json_escape(msg),
            cls
        )),
    );
}

/// 在后台等待一个 V3 端口，然后调用现有 `start_file` 完成：
/// APP → cmd50 → IAP → ERASE/PROM/VERIFY/END → 等回 APP。
pub fn spawn(host: Arc<App>, path: String) {
    let _ = thread::Builder::new()
        .name("ysc-auto-flash".into())
        .spawn(move || {
            if !std::path::Path::new(&path).is_file() {
                log(&host, &format!("自动烧录取消：镜像不存在 {}", path), "err");
                return;
            }
            log(
                &host,
                "自动烧录模式：等待 YSC V3 设备插入（插上即烧录）...",
                "info",
            );
            log(&host, &format!("自动烧录镜像: {}", path), "info");

            let mut wait_logged = false;
            loop {
                if let Some(p) = enum_towmcu_ports().into_iter().find(|p| p.side == "V3") {
                    // 端口刚枚举出来时 CDC 可能还没就绪；先探一次版本，避免
                    // start_file 立刻报“无法与设备通信”。
                    let ver = ysc_host::iap2::probe_version(&p.port);
                    if !ver.is_empty() {
                        log(
                            &host,
                            &format!("自动烧录：选中 {} ({})，版本 {}", p.port, p.serial, ver),
                            "info",
                        );
                        // start_file 内部会判断 APP/IAP、发 cmd50、等 30s 重枚举。
                        ysc_host::iap2::start_file(Arc::clone(&host), p.port, path);
                        return;
                    }
                    if !wait_logged {
                        log(
                            &host,
                            &format!("自动烧录：{} 已枚举，等待设备响应版本查询...", p.port),
                            "warn",
                        );
                        wait_logged = true;
                    }
                }
                thread::sleep(Duration::from_millis(POLL_MS));
            }
        });
}
