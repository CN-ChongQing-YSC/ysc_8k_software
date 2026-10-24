# ysc_8k_tauri — YSC 8K 纯 Rust 桌面驱动

Tauri v2 单 exe 桌面应用：Rust 宿主进程内完成串口/协议/IAP，WebView2 渲染现有 YSC UI。
不再使用 Electron、Node 运行时、C++ 驱动子进程或命名管道。

## 结构

```text
ysc_8k_tauri/
  web/                     Vue UI 入口（复用 @ysc/ui + @ysc/core）
    src/main.ts            挂载 App、安装 driverApi shim、接共享 store
    src/driver-api-shim.ts window.driverApi 兼容层（Tauri invoke/listen）
    src/desktop-shell.ts   无边框窗口拖拽/双击最大化
  src-tauri/               Rust 宿主
    src/lib.rs             Tauri builder / 托盘 / 单实例 / 命令表
    src/sink.rs            HostSink → "ysc:driver-event"
    src/notifier.rs        串口/槽位丢失、托盘提示
    src/host_loop.rs       100ms 探针、1s 端口变化广播
    src/commands.rs        ysc_* Tauri 命令
    capabilities/          事件监听 + 文件对话框权限
  tools/build.ps1          NSIS 构建 + 归档 + sha256
```

Rust 核心在 `../ysc_8k_driver_rs/crates/ysc_host`（`device` / `commands` / `iap` /
`iap2` / `kmbox` / `guard` / `monitor`），协议与 V2/V3 IAP 不重写。

## 开发 / 构建

```powershell
cd D:\project\ysc_project\ysc_8k_software\ysc_8k_tauri
npm install
npm --prefix web install

npm run dev              # Vite + Tauri 开发
npm --prefix web run typecheck
npm run build:app:debug  # target/debug/ysc_8k_desktop.exe（GUI）
npm run build:app        # NSIS 安装包
```

根目录也可用：

```powershell
npm run dev:tauri
npm run build:tauri      # -> ysc_8k_tauri/release/YSC_8K_Setup_<ver>.exe
```

## V3 固件页

侧栏「固件 → 固件更新 (YSC V3)」只显示 `side="V3"`（`YSCV3-CDC`）端口：

- 查询版本：`YSC-V3-IAP` / `YSC-V3-APP <date>`，显示 IAP/APP 模式。
- 「进入下载模式」：APP 下发送 cmd50；双片 3 阶段握手真机可能耗时 10–20s，
  宿主最多等待 30s，期间不要拔线。失败时 UI 会明确报错，不再误报已选中新串口。
- 手动选择加密镜像后升级；V3 END 后等待 `YSC-V3-APP`，否则 `APP_NOT_BOOTED`。
- 在线目录未发布时明确提示使用手动镜像，不静默失败。

### 启动自动烧录

1. 把 V3 加密镜像命名为 `v3_firmware.bin`，放在 `ysc_8k_desktop.exe` 同目录；
   或启动参数 `--auto-flash-v3=<path>` 显式指定。
2. 启动后自动切到 V3 页，等待 `YSCV3-CDC` 出现；端口就绪（能应答 cmd132）后
   自动执行 APP→IAP→烧录→回 APP，日志/进度与手动升级一致。
3. 端口刚枚举但设备未就绪时会等待重试，不会立刻报“无法与设备通信”。
4. 自动模式会一直等待设备插入；不需要自动烧录时删掉文件或加 `--no-auto-flash`。

真机验收记录：`YSC-V3-IAP` → 写入 `ysc_v3_8k_passthrough_app_encrypted.bin` →
`YSC-V3-APP Oct 21 2026`；当前 APP 模式页面见 `docs/v3-release.png`。

## 已知未完成（M5）

- CH343 驱动提权安装（当前返回“暂未接入”）。
- 在线固件目录/下载（当前返回空/提示手动文件）。
- PDF 静默导出（当前返回“暂未接入”）。
- V1/V2 面板的完整回归（串口/监控/V3 已测）。

旧 Electron/C++ 栈冻结在 `1.17.0`，只读保留；新功能只进本目录。
