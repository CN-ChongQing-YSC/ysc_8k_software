//! 批量自动烧录：参考 `rustXFDeriver` 的 `iap_flash auto` 端口自动选择思路，
//! 复用 YSC 已验证的 `ysc_host::iap2::start_file`，不重写 IAP 状态机。
//!
//! 触发条件（2026-10-24 定稿，按现场要求）：**只要设备当前处于 IAP 模式就烧**。
//!   · 烧完设备跳回 APP → 版本串不再含 `-IAP` → 下一轮自然跳过，不会自我循环；
//!   · 烧失败/中途拔插导致停在 IAP → 下一轮自动重烧（这正是现场要的）；
//!   · COM 号不会因 IAP/APP 切换而变，所以不需要按序列号做去重；
//!   · 一次只烧一台：`iap2_running` 为真时整个循环让路。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ysc_core::enumerate::{enum_towmcu_ports, TowmcuPort};
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

fn is_iap(ver: &str) -> bool {
    ver.contains("-IAP")
}

/// 后台批量等待 V3 端口：只要探到 IAP 模式就烧，烧完继续等下一台。
///
/// `cancel` 由 UI 开关/退出置位，循环下一轮退出；`running` 供 UI 显示状态。
pub fn spawn(host: Arc<App>, path: String, cancel: Arc<AtomicBool>, running: Arc<AtomicBool>) {
    let _ = thread::Builder::new()
        .name("ysc-auto-flash".into())
        .spawn(move || {
            running.store(true, Ordering::Release);
            if !std::path::Path::new(&path).is_file() {
                log(&host, &format!("自动烧录取消：镜像不存在 {}", path), "err");
                running.store(false, Ordering::Release);
                return;
            }
            log(
                &host,
                "批量自动烧录：等待处于 IAP 模式的 YSC V3 设备（烧完一台继续等下一台）...",
                "info",
            );
            log(&host, &format!("自动烧录镜像: {}", path), "info");

            let mut count: u32 = 0;
            let mut wait_logged = false;

            loop {
                if cancel.load(Ordering::Acquire) {
                    log(&host, "自动烧录已停止（开关关闭）", "warn");
                    break;
                }
                if host.iap2_running.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(POLL_MS));
                    continue;
                }

                let ports: Vec<TowmcuPort> = enum_towmcu_ports()
                    .into_iter()
                    .filter(|p| p.side == "V3")
                    .collect();

                for p in &ports {
                    // 端口刚枚举出来时 CDC 可能还没就绪；先探一次版本。
                    let ver = ysc_host::iap2::probe_version(&p.port);
                    if ver.is_empty() {
                        if !wait_logged {
                            log(
                                &host,
                                &format!("自动烧录：{} 已枚举，等待设备响应版本查询...", p.port),
                                "warn",
                            );
                            wait_logged = true;
                        }
                        continue;
                    }
                    wait_logged = false;
                    if !is_iap(&ver) {
                        continue; // 已在 APP：不碰它，避免烧完自我循环
                    }
                    count += 1;
                    log(
                        &host,
                        &format!(
                            "自动烧录 #{}：{} 处于 IAP（{}），开始烧录 ...",
                            count, p.port, ver
                        ),
                        "info",
                    );
                    // start_file 起自己的线程后立即返回；下一轮 iap2_running 为真会让路，
                    // 烧完设备跳到 APP，版本串不再含 -IAP，自然不会被再选中。
                    ysc_host::iap2::start_file(Arc::clone(&host), p.port.clone(), path.clone());
                    break; // 一次只烧一台
                }

                thread::sleep(Duration::from_millis(POLL_MS));
            }
            running.store(false, Ordering::Release);
        });
}
