# YSC 8K 上位机软件

本仓库包含 YSC 8K 设备的 Windows 上位机软件，已从固件仓库
`ysc_v2_8k_passthrough` 解耦，独立维护、独立发版。

**桌面端当前唯一维护方向：纯 Rust + Tauri v2 单 exe。**
Electron + C++ 组合栈已冻结在 `1.17.0`，只读保留用于回归，不再修 bug/加功能。
项目当前进度、冻结边界和 AI 必读状态见 [`AGENTS.md`](AGENTS.md)。

## 目录

| 目录 | 说明 | 状态 |
|---|---|---|
| `ysc_8k_tauri/` | **活动桌面端**：Rust 宿主 + WebView2 + NSIS，单 exe | 维护 |
| `packages/ysc-ui/` | 当前 YSC 桌面 UI（从 Electron renderer 迁入） | 维护 |
| `packages/ysc-core/` | 协议、串口传输、设备 store、共享 UI | 维护 |
| `ysc_8k_web/` | 网页版（Web Serial） | 维护 |
| `ysc_8k_driver_rs/` | Rust 核心：`ysc_core` / `ysc_host` / SDK / 旧管道二进制 | 核心维护；管道壳冻结 |
| `ysc_8k_driver/` | C++ 驱动 + SDK 源工程 | **冻结** |
| `ysc_8k_electron/` | Electron + Vue 桌面壳 | **冻结** |

## 快速开始（Tauri 桌面版）

```powershell
cd D:\project\ysc_project\ysc_8k_software

# 首次
npm --prefix ysc_8k_tauri install
npm --prefix ysc_8k_tauri/web install

# 开发（Vite + Tauri）
npm run dev:tauri

# 类型检查 + 调试构建
npm run typecheck:tauri
npm run build:tauri:debug

# 发布 NSIS（产出 release/YSC_8K_Setup_<ver>.exe + sha256）
npm run build:tauri
```

构建细节见 [`ysc_8k_tauri/README.md`](ysc_8k_tauri/README.md)。
Rust 核心测试：

```powershell
cd ysc_8k_driver_rs
cargo test --workspace
```

## YSC V3 固件更新

侧栏「固件 → 固件更新 (YSC V3)」只显示 `side="V3"`（`YSCV3-CDC`）端口：

- 查询 `YSC-V3-IAP` / `YSC-V3-APP <date>`，显示 IAP/APP 模式。
- APP 下可「进入下载模式」，双片握手最长等待 30s。
- 服务器在线目录：`https://ysc-order-2026.top/fwapi`，`deviceType=ysc_v3_8k`，
  当前最新 `1.2.1`（69,800 B）；Tauri 页可检查、下载、SHA-256 校验后升级。
- 手动选择 V3 加密镜像（AES 密钥 `YSC_V3_8K_2026`）仍保留；
  END 后必须等回 `YSC-V3-APP`，否则报 `APP_NOT_BOOTED`。
- 启动自动烧录：exe 同目录放 `v3_firmware.bin`，或传
  `--auto-flash-v3=<path>`；自动模式只烧处于 IAP 的设备，详见
  [`ysc_8k_tauri/README.md`](ysc_8k_tauri/README.md)。

真机验收：`YSC-V3-IAP` → 写入 `ysc_v3_8k_passthrough_app_encrypted.bin` →
`YSC-V3-APP Oct 21 2026`。当前页面：[`docs/v3-release.png`](ysc_8k_tauri/docs/v3-release.png)。

## 发布流程（Release）

**安装包只通过 GitHub Releases 分发，不提交进 git 仓库。**

1. 同步版本号：`ysc_8k_tauri/package.json`（Tauri 构建脚本读取它）、
   `ysc_8k_tauri/src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`；
   旧栈版本 `VERSION` / `ysc_8k_driver` 继续冻结在 `1.17.0`。
2. `CHANGELOG.md` 落版：把「未发布（Unreleased）」改成 `## v<ver> — <日期>`，
   并在顶部新开空的「未发布」段。
3. 构建：`npm run build:tauri`，产物归档为
   `ysc_8k_tauri/release/YSC_8K_Setup_<ver>.exe`（约 5.4 MiB），脚本输出 sha256。
4. 打 tag + 发 Release：tag `v<ver>`；安装包作为 **Release 附件**上传，
   附件名用 ASCII：`YSC_8K_Setup_<ver>.exe`。直链形如
   `https://github.com/CN-ChongQing-YSC/ysc_8k_software/releases/download/v<ver>/YSC_8K_Setup_<ver>.exe`
5. 验证：确认 Release 资产与 sha256，再从直链完整下载一次、比对哈希。

`ysc_8k_tauri/release/`、`src-tauri/target/` 已在 `.gitignore` 中，
**不要 `git add -f` 强推安装包**。

## 旧栈（只读）

- Electron 开发/打包：`ysc_8k_electron/`，其 `FROZEN.md` 说明冻结边界。
- C++ 驱动/SDK：`ysc_8k_driver/`，其 `FROZEN.md` 说明冻结边界。
- 旧 Rust 命名管道驱动：`ysc_8k_driver_rs/crates/ysc_driver/`，仅回归用。
- 旧管道 `\\.\pipe\ysc_8k_driver` 的逐行 JSON 协议仍可用于回归，
  但 Tauri 正式版不再使用该管道、也不启动 `ysc_8k_driver.exe`。
