# FROZEN — 停止维护

本目录的 C++ 驱动 + SDK 源工程自 **2026-10-21** 起冻结。

- 不再接收功能更新、bug 修复或协议变更。
- 仅作为 Rust/Tauri 实现的回归对照保留；`ysc_sdk` 的 C ABI 以 Rust 版为准。
- 新功能/修复请改 `../ysc_8k_tauri/`、`../ysc_8k_driver_rs/crates/ysc_host/`。
- 不要再把本目录编译出的 `ysc_8k_driver.exe` 打进安装包。

项目当前状态见仓库根 `AGENTS.md`。
