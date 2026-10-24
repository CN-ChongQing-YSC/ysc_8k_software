# ysc_8k_driver_rs — YSC 8K 驱动的 Rust 移植版

这是 `../ysc_8k_driver/`（C++17 / Win32）的**功能性 Rust 移植**。对外契约完全不变：

- 同一个可执行文件名与投放路径：`../ysc_8k_driver/work/Release/ysc_8k_driver.exe`
- 同一条命名管道：`\\.\pipe\ysc_8k_driver`
- 同一套逐行 JSON `{"type":...}` 事件与全部命令
- 同一个托盘程序、单实例互斥体、调试窗口、串口独占、kmboxnet UDP 服务、v1/v2/v3 固件升级
- 同一套 C ABI：`ysc_sdk.dll`（x64 + x86，36 个 `Ysc_*` 导出）

Electron 前端、网页版、Python/C#/易语言示例都**不需要改一行代码**。

---

## 1. 目录

```text
ysc_8k_driver_rs/
├── Cargo.toml                  workspace
├── build.ps1                   构建 + 投放到 Electron/示例期望的位置
├── crates/
│   ├── ysc_core/               协议 / 串口 / 枚举 / 公共工具
│   │   ├── frame.rs            <START><len><payload><END> 编解码 + 收帧状态机
│   │   ├── serial.rs           重叠串口 I/O、收数看门狗、探测/切换、同步 API
│   │   ├── enumerate.rs        注册表 COM 口 / SetupAPI CDC / CH343 驱动健康
│   │   └── consts.rs           波特率表、角色、帧常量、驱动版本
│   ├── ysc_driver/             托盘驱动 exe
│   │   ├── pipe.rs             命名管道服务（重叠 I/O）
│   │   ├── commands.rs         全部管道命令（对照 main.cpp::OnPipeCommand）
│   │   ├── device.rs           双槽位、按角色路由、715/711 角色识别
│   │   ├── monitor.rs          鼠标/键盘物理状态聚合 + kmnet 监控包
│   │   ├── guard.rs            物理护栏校准
│   │   ├── kmbox.rs            kmboxnet UDP 服务 + NET 活动统计
│   │   ├── iap.rs / iap2.rs    v1 / v2+v3 固件升级
│   │   └── tray.rs             托盘、隐藏窗口、调试窗口、热插拔、消息循环
│   └── ysc_sdk/                ysc_sdk.dll（C ABI）
├── reference/                  本地保存的 C++ 参照二进制（gitignore）
└── tools/                      差分探针（管道 / 串口 / UDP / IAP）
```

### C++ → Rust 对照

| C++ | Rust | 说明 |
|---|---|---|
| `main.cpp` | `main.rs` + `tray.rs` + `commands.rs` | 入口、消息循环、管道命令分发 |
| `serial_port.cpp` | `ysc_core/serial.rs` | 重叠 I/O、帧状态机、异步写队列、看门狗 |
| `device_context.cpp` | `device.rs` | 两槽位、角色识别（715/711 + 数据流兜底） |
| `command_bridge.cpp` | `command_bridge.rs` | 固件命令 JSON、按角色解析串口 |
| `monitor_push.cpp` | `monitor.rs` | 鼠标/键盘状态、变化即推 |
| `human_guard.cpp` | `guard.rs` | 反向间隔直方图、p5/median/suggest |
| `kmboxnet_server.cpp` | `kmbox.rs` | 16B 头 + soft_mouse/keyboard、ACK、统计 |
| `iap_upgrader.cpp` | `iap.rs` | ERASE/PROGRAM/VERIFY/END、3 次重试 |
| `towmcu_iap_upgrader.cpp` | `iap2.rs` | CDC 轮询读、APP→IAP 重枚举；V2 towmcu + V3 YSC-V3 共用 |
| `pipe_server.cpp` | `pipe.rs` | 单客户端、字节流、逐行 JSON |
| `debug_logger.cpp` | `debug_log.rs` | 文件日志 + 调试窗口 |
| `towmcu_cdc.cpp` | `ysc_core/enumerate.rs` | SetupAPI 枚举 / 驱动健康 |
| `ysc_sdk.cpp` | `ysc_sdk/lib.rs` | 36 个 C ABI 导出（x64/x86） |

---

## 2. 构建

```powershell
# 首次需要 32 位 SDK 目标
rustup target add i686-pc-windows-msvc

cd ysc_8k_driver_rs
pwsh -File .\build.ps1          # release：exe + x64/x86 SDK，并投放
pwsh -File .\build.ps1 -DebugBuild   # dev：带控制台，便于看 panic
pwsh -File .\build.ps1 -NoCopy  # 只构建
```

投放位置（与仓库原契约一致）：

| 产物 | 目标 |
|---|---|
| `target\release\ysc_8k_driver.exe` | `..\ysc_8k_driver\work\Release\ysc_8k_driver.exe` |
| `target\release\ysc_sdk.dll` | `..\ysc_8k_driver\bin\x64\Release\ysc_sdk.dll` |
| `target\i686-pc-windows-msvc\release\ysc_sdk.dll` | `..\ysc_8k_driver\bin\Win32\Release\ysc_sdk.dll`、`..\Example\E\ysc_sdk.dll` |

只做编译检查：

```powershell
cargo check --workspace
cargo test --workspace        # 19 个单元测试
```

> 驱动版本号在编译期读取仓库根 `VERSION`（`ysc_core::consts::driver_version()`），
> 不再需要像 C++ 那样手工改 `main.cpp`。发版时仍按 README 同步 `VERSION` 与各
> `package.json`。

---

## 3. 验证

### 3.1 与 C++ 参照驱动的差分对照（真实运行）

`tools/` 里的探针均以同一脚本、同一顺序分别打到 Rust 与 C++ 参照驱动，归一化
（去掉 `session` / `elapsed` / `ms` / `seenMs` 等时间字段）后逐事件比较。

| 探针 | 覆盖 | 结果 |
|---|---|---|
| `pipe_probe.py` | 24 条命令：版本/状态/端口/设备槽/监控/键盘/NET/护栏/串口错误/IAP 错误/debug | **事件集合完全一致**（23 vs 23 唯一事件） |
| `serial_probe.py` | 空闲 COM 口：打开、状态、切波特率失败、断开、自动探测失败 | 一致（Rust 的事件到达更及时） |
| `device_probe.py` | 槽位挂载失败、参数校验、角色、卸下 | **逐事件一致** |
| `kmnet_probe.py` | 16 种 UDP 命令：ACK 头、统计计数、ring 记录 | 协议/统计一致（ACK 时延受测试端不读管道影响，双方同现象） |
| `inject_probe.py` | 无硬件注入：鼠标/键盘监控、715/711、debug 转发、物理护栏完整采样 | **20 个事件 / 14 个唯一事件逐项一致** |
| `kmmon_probe.py` | 注册 kmboxnet 监控客户端 → 注入鼠标/键盘 → 收 20B 包 | **包字节完全一致**（`01030a00fbff010002...`） |
| `iap_probe.py` | v1/v2 文件缺失、Base64 错误、版本查询、进入 IAP 失败 | 一致 |
| v1 IAP 擦除超时 | 真实串口上跑完整 ERASE 重试链 | 日志与 `iap_done` 一致 |

运行方式（驱动已启动）：

```powershell
python tools\pipe_probe.py
python tools\serial_probe.py COM237
python tools\device_probe.py
python tools\kmnet_probe.py
python tools\inject_probe.py
python tools\kmmon_probe.py
python tools\iap_probe.py
```

参照二进制在 `reference/`（本地保留，不入库）；C++ 源码仍在 `../ysc_8k_driver/`。

### 3.2 SDK

- x64/x86 两个 DLL 的 `dumpbin /exports` 都是 **36 个干净的 `Ysc_*` 名称**，
  与 C++ 参照 DLL 的导出面一致。
- `Example/python/test_dll.py --dll ...`：**PASS=8 FAIL=0**。
- 与 C++ x64 DLL 逐项对照：版本、9 档波特率、`Ysc_ListPorts`、
  `Ysc_ListAllComPorts`（5 个口及描述）、错误字符串、空闲 COM 上
  `Connect/IsConnected/GetBaudrate/GetPortName/QueryVersion 超时/SwitchBaudrate 失败`
  全部一致。
- 32 位调用已用 MSVC x86 宿主实测（`LoadLibrary` + 6 个导出）。

### 3.3 Electron 端到端

把 release exe 投放到 `ysc_8k_driver/work/Release/` 后直接启动现有 Electron：

- Electron 主进程 spawn 的正是 Rust 驱动（进程路径已确认）；
- 界面正常渲染，标题栏显示 `v1.17.0`；
- 点开串口下拉框，列表由驱动的 `ports_list` 事件填充：
  `COM1 / COM48 / COM237 / COM531 / COM535`，与探针结果一致。

验证时会本地生成 `electron_window.png` / `electron_dropdown.png`（gitignore，不随仓库分发）。

打包链路也已验证：`cd ysc_8k_electron && npm run pack` 生成的
`release/win-unpacked/resources/ysc_8k_driver.exe` 与
`ysc_8k_driver/work/Release/ysc_8k_driver.exe` **SHA-256 完全相同**
，说明 electron-builder 的 extraResources 契约未变。

### 3.4 单元测试

`cargo test --workspace`：

- `ysc_core`：帧编解码、IAP 帧、收帧状态机、端口解析、探测顺序、固件族识别（17）
- `ysc_driver`：715/711 解析与角色判定、数据流兜底、监控帧解析、
  20 字节 kmnet 监控包、护栏反向间隔/分位数、V3 端口判定（10）

### 3.5 运行环境

- exe / DLL 使用**静态 MSVC CRT**（`.cargo/config.toml` 的 `+crt-static`），
  目标机不需要安装 VC++ Redistributable；
- 只依赖 Windows 自带的 `ws2_32` / `user32` / `setupapi` / `winmm` 等系统 DLL。

### 3.6 YSC V3 接入

Rust 驱动在不改前端契约的前提下接入 YSC V3（双 CH32V317，`YSC-V3-*` 身份串）：

- `ysc_core::family` 识别 `YSC-V3-APP <date>` / `YSC-V3-IAP` / 旧 `XFBOX-*`；
  `YSC-IAP` / `ysc-towmcu-*` 仍走 V2。
- `enumerate` 把 CDC 序列号 `YSCV3-CDC`（及旧 `XFBOX-CDC`）标记为 `side="V3"`。
- 升级复用现有 `towmcu_*` 命令与 `iap2_*` 事件；V3 与 V2 由版本串自动分派。
  V3 的 IAP/APP 共用序列号，cmd50 后按 cmd132 版本串重新确认端口。
- V3 在 END 后必须等设备重新枚举回 `YSC-V3-APP` 才报成功；等不到则报
  `APP_NOT_BOOTED`，用于识别“密钥不匹配但 PROM/VERIFY 通过”的镜像。
- 现有 V2 面板的“手动选择文件/升级”按钮可直接用于 V3（端口显示 `[V3]`）；
  V3 的在线版本目录（`ysc_towmcu_left/right`）暂未接入。

---

## 4. 有意差异（均为修正，不改变契约）

1. **JSON 转义**：C++ 在 `iap_log` / `send_result` 等动态串里直接拼路径或命令，
   路径含 `\` 时会输出非法 JSON；Rust 统一做 JSON 转义。字段与语义不变。
2. **槽位串口失联**：C++ 只处理单设备 `g_serial` 的 `WM_SERIAL_LOST`，双设备槽位
   掉线后仍显示已连接；Rust 增加 `WM_SLOT_LOST`，按槽位清理并推 `device_lost`。
3. **`device_attach` 的 baud 回显**：显式波特率握手失败、退回自动探测时，C++ 事件
   仍回显"请求的波特率"；Rust 回显实际使用的波特率（`device_list` 也一致）。
4. **kmboxnet 停止**：C++ 用 `closesocket` 打断 `recvfrom`；Rust 用 100ms 读超时轮询，
   `kmnet_stop` 最坏多等 100ms，行为等价。
5. **版本号单一真源**：驱动版本从 `VERSION` 编译期注入，避免 C++ 里两处手改漂移。

---

## 5. 已知验证边界

本机没有 YSC 真机（唯一同 VID/PID 的 `COM531` 是另一产品 XF Hub，正被 `xf-hub`
占用），因此以下**依赖固件应答**的路径只做了逻辑级/错误路径验证，未做真机烧录：

- 监控帧 / 键盘帧的实时解析（已用合成帧单测）
- 物理护栏的真实采样与建议值（已用合成反向序列单测）
- IAP 成功烧录全流程（帧构造、重试、超时失败路径已对照 C++）
- 角色自动识别拿到真实 715/711 回包后的判定（解析与判定已单测）

接上 YSC 设备后，建议按 `tools/` 探针再跑一轮真机验收。
