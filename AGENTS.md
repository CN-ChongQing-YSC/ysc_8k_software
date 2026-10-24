# YSC 8K — 项目状态（AI 每次先读）

> 本文件是仓库级“当前进度”真相源。`CLAUDE.md` 是同一状态的本地补充记忆。
> 任何 AI 开始任务前先读本文件；如果代码与本文件冲突，以本文件记录的架构方向为准，
> 并在改动后同步更新本文件。

## 0. 当前阶段（2026-10-21 起）

**桌面端已从 Electron + C++ 迁移到纯 Rust + Tauri v2 单 exe；首个可运行版本和 V3 真机升级已跑通。**

- **活动代码 / 唯一维护方向**
  - `ysc_8k_tauri/`：Tauri v2 桌面应用（Rust 宿主 + WebView2 + NSIS），当前版本 `2.0.0`
  - `packages/ysc-ui/`：当前 YSC 桌面 UI（已从 `ysc_8k_electron/src/renderer` 迁入）
  - `packages/ysc-core/`：协议、传输、设备 store、共享 UI（继续维护）
  - `ysc_8k_driver_rs/crates/ysc_host/`：设备/命令/IAP 核心（已从旧 `ysc_driver` 抽出）
  - `ysc_8k_web/`：网页版（Web Serial）继续维护
- **冻结代码 / 只读回归参照**
  - `ysc_8k_electron/`：Electron + Vue 桌面壳，**停止功能更新和 bug 修复**
  - `ysc_8k_driver/`：C++ 驱动 + SDK 源工程，**停止功能更新和 bug 修复**
  - `ysc_8k_driver_rs/crates/ysc_driver/`：旧命名管道二进制薄壳，仅用于回归
- **禁止**
  - 不要再往 Electron/C++ 加新功能或修 bug；新能力只进 Tauri 栈。
  - 不要把 `ysc_8k_driver.exe` 作为 Tauri 的 sidecar / 子进程重新引入。
  - 不要在 Tauri 进程里恢复 `\\.\pipe\ysc_8k_driver` 命名管道。
  - 不要删除旧栈源码；保留 `FROZEN.md` 和回归能力，等 Tauri 验收后再评估清理。

## 1. 目标架构

```text
WebView2 (Vue UI, @ysc/ui)
  window.driverApi 兼容 shim
  @ysc/core
        │ Tauri invoke/listen（进程内，无管道）
Rust Tauri 宿主 (ysc_8k_tauri/src-tauri)
  ysc_host::App (device/commands/iap/iap2/kmbox/guard/monitor)
  ysc_core (serial/frame/enumerate/family)
  TauriSink → emit "ysc:driver-event"
  tray / single-instance / dialog / CH343 / window
```

关键决策：

- **单进程、单 exe**：没有 Electron、Node 运行时、C++ 驱动子进程、命名管道。
- **界面不重写**：复用当前 YSC UI；通过 `window.driverApi` 兼容层接 Tauri IPC，
  现有面板/事件名/载荷形状尽量逐字保持不变。
- **保留 YSC 已验证的 Rust 协议/IAP**：参考工程 `rustXFDeriver` 把协议放在 TS、
  Rust 只做字节；YSC 的协议/IAP 已经在 Rust 里且经过真机验证，本次不重写到 TS。
  这是与参考工程的有意差异，记录在此。
- **旧栈冻结**：Electron/C++ 冻结在 `1.17.0`；首个 Tauri 发布版为 `2.0.0`。

## 2. YSC V3 硬件 / 固件（已落地）

- 硬件工程：`D:\project\ysc_project\ysc_v3_8k_passthrough`（双 CH32V317，IAP+APP）。
- IAP AES 密钥：`YSC_V3_8K_2026`；旧密钥 `XF_V317_2026`。
- 对外身份：APP `YSC-V3-APP <date>`、IAP `YSC-V3-IAP`、CDC 序列号 `YSCV3-CDC`。
- Rust 驱动已接入 V3：`ysc_core::family` 识别版本串，`enumerate` 标 `side="V3"`，
  `iap2` 按 V2/V3 自动分派；V3 END 后必须等回 `YSC-V3-APP`，否则 `APP_NOT_BOOTED`。
- V3 灯效：IAP 紫罗兰脉冲（`violet_pulse_table`，25ms/步，1.6s 周期），
  已在 `releases/ysc_v3_8k_passthrough_iap.hex` 验证。
- **Tauri 固件更新页必须包含 V3 板块**：`firmware-v3` / `V3FirmwarePanel`，
  只显示 `side="V3"` 端口，查询 `YSC-V3-IAP/APP`，手动加密镜像升级，
  等待 `YSC-V3-APP`；在线目录暂未发布时显示“暂无/使用手动文件”。
- **进入下载模式是双片 3 阶段握手，真机可能耗时 10–20s**：宿主扫描窗口已放宽到
  30s，失败时 UI 不再误报“已自动选中新串口”。测试时请耐心等待，不要 10s 内拔线。
- **启动自动烧录（2026-10-24）**：exe 同目录放 `v3_firmware.bin`，或启动参数
  `--auto-flash-v3=<path>`，应用会切到 V3 页、等待 V3 端口并**先 probe_version**
  （避免端口刚枚举就报通信失败），再由 `ysc_host::iap2::start_file` 完成
  APP→IAP→烧录→回 APP。`--no-auto-flash` 可强制普通启动；普通启动不受影响。

## 3. 构建 / 测试（迁移完成后）

```powershell
# Rust 核心与宿主
cd D:\project\ysc_project\ysc_8k_software\ysc_8k_driver_rs
cargo test --workspace

# 前端
cd D:\project\ysc_project\ysc_8k_software\ysc_8k_tauri
npm --prefix web run typecheck
npm --prefix web run build

# Tauri
npm run build:app:debug   # 调试可执行文件（GUI 无控制台，日志在 exe 同目录）
npm run build:app:dir     # 只出目录
npm run build:app         # NSIS 安装包
```

发布规则仍见 `CLAUDE.md`：安装包只走 GitHub Releases，附件名
`YSC_8K_Setup_<version>.exe`，不提交进 git。

## 4. 迁移阶段

- [x] M0 记忆与冻结声明
- [x] M1 抽取 `ysc_host`（HostSink/HostHooks/App 解耦）
- [x] M2 Tauri 骨架（窗口/托盘/单实例/命令表）
- [x] M3 in-process 宿主 + UI 接线（串口/监控/面板）
- [x] M4 V3 固件页 + 真机升级（IAP 直刷、APP→IAP→APP 两条路径均已验证）
- [x] M4.1 启动自动烧录（自动端口/版本探测，复用 IAP；`--auto-flash-v3` 或同名固件）
- [ ] M5 功能补齐（CH343 提权安装 / 在线固件 / PDF 静默导出 / V1-V2 回归）
- [x] M6 首个 Tauri 发布包：`ysc_8k_tauri/release/YSC_8K_Setup_2.0.0.exe`
  （5,615,816 B，sha256 `1ECC861D95174E054C651237EA09CBAFC0752F43EF0F93A4D6876770A1E22E82`；
  单 exe，无子进程/管道）

## 5. 已知边界

- V3 在线版本目录尚未发布：先手动加密镜像，不阻塞烧录；V3 页已显示提示。
- PDF 导出在 Tauri 首版仍返回“暂未接入”；后续用 WebView2 PrintToPdf 或系统打印。
- CH343 提权安装 / 在线固件下载尚未迁入 Tauri（UI 有明确定位，后续补）。
- Tauri 依赖 WebView2；NSIS 使用 `downloadBootstrapper`，离线环境需预装。
- Windows-only；不迁移 Linux/macOS。
- 旧 Electron 在当前终端环境可能受 `ELECTRON_RUN_AS_NODE=1` 影响；仅回归时清掉该变量，
  新 Tauri 不受影响。
