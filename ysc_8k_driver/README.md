# ysc_8k_driver

Win32 tray application (C++17 / VS 2019 v142 / x64) that owns **all** serial
I/O for the YSC 8K host software. The Electron app (`../ysc_8k_electron/`)
talks to it over a named pipe — it never opens a COM port directly.

## Module layout

| File | Role |
|---|---|
| `main.cpp` | App entry, tray icon, named-pipe command dispatch (`OnPipeCommand`) |
| `serial_port.{h,cpp}` | `SerialPort` class — overlapped COM I/O + `<START><len><END>` framing for the v1 single-MCU UART path |
| `device_context.{h,cpp}` | **多设备层**：最多两个槽位、两路串口并存、角色自动识别（鼠标机/键盘机）、按角色路由 |
| `command_bridge.{h,cpp}` | JSON command builders (`SendJumpIAP` = `{"cmd":50}`, `SendUploadStatus`, debug capture …)；每条命令带 `DeviceRole` 决定发往哪台设备 |
| `iap_upgrader.{h,cpp}` | **v1** IAP upgrade — single UART MCU, baud probe + switch to 1.5 Mbaud |
| `towmcu_cdc.{h,cpp}` | **v2** SetupAPI CDC port enumeration (VID `0x1A86`/PID `0xFE0C` + USB iSerialNumber) |
| `towmcu_iap_upgrader.{h,cpp}` | **v2** IAP upgrade over USB-CDC (dual-MCU board) |
| `pipe_server.{h,cpp}` | Named-pipe server `\\.\pipe\ysc_8k_driver` |
| `kmboxnet_server.{h,cpp}` | kmbox network UDP bridge |
| `monitor_push.{h,cpp}` | 鼠标 + 键盘物理状态汇聚；合成 kmboxnet monitor 包 |

## 双设备（一鼠标机 + 一键盘机）

同一台 PC 接两台 YSC：一台上游插鼠标（透传鼠标数据）、一台上游插键盘（透传键盘数据）。
驱动把两台设备抽象成两个"槽位"，并按**角色**而不是 COM 号标识（换 USB 口 COM 号会变）。

```
槽位 0  role=mouse  ← 鼠标类命令（cmd 30-33/35/36-41/51-53）
槽位 1  role=kbd    ← 键盘类命令（cmd 45-48）
```

角色**自动识别**：驱动向上游设备发 `cmd 715`（取 {vid,pid,product}）与 `cmd 711`
（取上游 CONFIG 描述符 hex），后者含每个 HID 接口的 `bInterfaceProtocol`
（`0x01`=键盘、`0x02`=鼠标）——这是判定角色的权威依据（只看 vid/pid 无法区分
同一只键鼠复合接收器）。识别不出时前端可手动指定。单设备场景下
`SlotByRole` 会退回"第一个已连接设备"，旧行为完全不变。

### 管道命令（新增）

| 命令 | 说明 |
|---|---|
| `{"type":"device_list"}` | 返回两个槽位的角色/端口/连接态/上游身份（事件 `device_list`） |
| `{"type":"device_attach","dev":0,"port":"COM7","baud":3000000,"role":"mouse"}` | 挂载到槽位；`role` 省略=自动探测。事件 `device_attached` |
| `{"type":"device_detach","dev":1}` | 卸载槽位。事件 `device_detached` |
| `{"type":"device_set_role","dev":1,"role":"kbd"}` | 手动指定角色（人工优先于自动识别） |
| `{"type":"get_kbd_state"}` | 键盘物理按键快照。事件 `kbd_state` |
| `{"type":"kbd_key","kc":4,"down":1}` | 键盘单键注入（HID 键码）。事件 `kbd_result` |
| `{"type":"kbd_type_string","s":"hello"}` | 逐字符打字符串（≤128B） |
| `{"type":"kbd_release_all"}` | 释放所有注入键 |

`get_state` / `get_monitor` 事件新增 `mouse`（valid/buttons/x/y）与
`kbd`（valid/mod/count/overflow/keys）字段。

### 键盘物理状态

固件在物理按键变化时上报
`{"code":200,"message":"kbd","data":"{\"mod\":N,\"n\":M,\"ov\":K,\"k\":[..]}"}`
（`mod` = 修饰键位图 bit0..7 = HID 0xE0..0xE7；`k` = 非修饰 HID 键码，升序）。
驱动侧 `MonitorPush` 按 `message` 分流：`mouse` 进鼠标侧、`kbd` 进键盘侧，
再合成一个 kmboxnet `monitor_packet_t` 推给已注册客户端 —— 因此 Net API
无需改动即可同时读到键鼠数据。

> 注：`KBD_MON_KEYS_MAX`（驱动）与固件 `KBD_STATE_KEYS_MAX` 必须一致，
> 两仓库已解耦不能互相 include，改一侧需同步另一侧。

## Build

From a VS 2019 (or VS 2022 with v142 installed) Developer Prompt — x64:

```
msbuild ysc_8k_driver.vcxproj /p:Configuration=Release /p:Platform=x64
```

Output: `work\Release\ysc_8k_driver.exe` (the `OutDir` override at vcxproj:84).
`setupapi.lib` and `cfgmgr32.lib` are already in `AdditionalDependencies`.

## ysc_sdk.dll — 给其它语言调用的驱动 SDK

不想在 易语言 / Python / C# / Delphi 里重复实现串口驱动？用 `ysc_sdk.dll`。
它把连接串口、列出串口、切换波特率、全部 YSC 协议命令封装成 27 个 C 导出函数，
其它语言直接加载 DLL 即可。

- 源码：`ysc_sdk.h` / `ysc_sdk.cpp`（串口逻辑严格对照 `serial_port.cpp`，
  枚举复用 `towmcu_cdc.cpp`）。工程：`ysc_sdk.sln` / `ysc_sdk.vcxproj`
  （工具集 v142/v143/v145 任选，`DynamicLibrary`，静态 CRT `/MT` 无运行库依赖）。

```
msbuild ysc_sdk.sln /p:Configuration=Release /p:Platform=x64   # -> bin\x64\Release\ysc_sdk.dll
msbuild ysc_sdk.sln /p:Configuration=Release /p:Platform=x86   # -> bin\Win32\Release\ysc_sdk.dll（32 位易语言）
```

完整 API、调用约定、各语言示例见 `../Example/`（Python 已含覆盖全部 27 个接口、
真机验证的自动化测试 `test_dll.py`）。


## v2 (towmcu dual-MCU) firmware update — IPC protocol

The Electron app sends newline-delimited JSON `{"type":"<cmd>", ...}\n` over
`\\.\pipe\ysc_8k_driver`. Responses come back as JSON events of the form
`{"type":"<event>", ...}`. The v2 flow uses these five commands:

### 1. List towmcu CDC ports

```jsonc
{"type":"towmcu_list_ports"}
```
Reply:
```jsonc
{"type":"towmcu_ports","ports":[
  {"port":"COM24","serial":"TOWMCULEFT","side":"Left","desc":"YSC USB CDC (COM24)"},
  {"port":"COM28","serial":"TOWMCURIGHT","side":"Right","desc":"YSC USB CDC (COM28)"}
]}
```
`side` is `"Left"` / `"Right"` (APP mode) / `"IAP"` (bootloader, serial `TOWMCUIAP`).

### 2. Query device version / mode

```jsonc
{"type":"towmcu_query_version","port":"COM24"}
```
Reply:
```jsonc
{"type":"towmcu_version","port":"COM24","version":"ysc-towmcu-L v1.0","mode":"APP"}
```
`mode` is `"APP"` (running application) or `"IAP"` (sitting in the bootloader,
version `YSC-IAP`). Status lines also arrive on `iap2_log`.

### 3. Enter download mode (send cmd:50)

```jsonc
{"type":"towmcu_enter_iap","port":"COM24"}
```
Sends `{"cmd":50}`; the device clears its anti-brick flag, writes
`BKP_DR1=0xA5A5`, resets, and re-enumerates as `TOWMCUIAP`. The driver polls
`EnumTowmcuPorts()` for up to 10 s and reports the IAP port on `iap2_log`.
The COM number **may change** — call `towmcu_list_ports` again afterwards.

### 4. Start firmware upgrade

```jsonc
{"type":"towmcu_start","port":"COM24","path":"D:\\path\\to\\convert\\left\\ysc_left_app.bin"}
```
The driver works out whether the device is in APP or IAP mode, enters IAP if
needed, finds the IAP port, then runs **ERASE → PROGRAM (60 B chunks) →
VERIFY (60 B chunks) → END**. `port` may be either the APP port (the driver
will enter IAP itself) or the IAP port directly. Progress streams on:

```jsonc
{"type":"iap2_log","message":"…","cls":"info"}            // cls: info|warn|err|ok
{"type":"iap2_progress","current":42,"total":600,"status":"编程 42/600"}
{"type":"iap2_done","success":true,"error":""}
```

### 5. Cancel

```jsonc
{"type":"towmcu_cancel"}
```

### File dialog (Electron only)

The Electron main process exposes an `iap2:openFile` IPC handler
(`src/main/index.ts`) that shows a `.bin` picker for the Left or Right file
and returns `{path, info}`. This is **not** part of the wire protocol — it is
Electron-side. Pipe clients supply the path directly in `towmcu_start`.

## Raw CDC frames (for direct serial testing, bypassing the driver)

Frame markers are ASCII `<START>` (7 B) and `<END>` (5 B). Two framing styles:

**JSON control frames** (no checksum, no cmd/len byte):
```
<START> <BE total_len u16> <UTF-8 JSON body> <END>
total_len = 7 + 2 + len(json) + 5
```
- Query version: `{"cmd":132}` (0x84)
- Jump to IAP:   `{"cmd":50}`

**Binary IAP frames** (with cmd/len/rev/checksum):
```
<START> <BE total_len u16> <cmd u8> <len u8> [rev 4B zeros, ERASE/VERIFY only]
        <data..> <LE cksum u16> <END>
cksum = LE u16 sum of (cmd + len + rev + data bytes)
```
- PROGRAM chunk (0x80): `<START><BE len><0x80><0x3C><60 B data><LE cksum><END>`
- ERASE       (0x81):    `<START><BE len><0x81><0x00><00 00 00 00><LE cksum><END>`
- VERIFY chunk(0x82):    `<START><BE len><0x82><0x3C><00 00 00 00><60 B data><LE cksum><END>`
- END         (0x83):    `<START><BE len><0x83><0x00><LE cksum><END>`  (NO response)

Each binary frame except END is ack'd with a 2-byte payload `[0x00][status]`
(`status == 0` = OK). Open the CDC port at any valid baud (CDC ignores it;
115200 is conventional) — see `towmcu_iap_upgrader.cpp` for the exact
`COMMTIMEOUTS` + polling-reader pattern.

The encrypted `.bin` is a single continuous AES-128-CTR stream produced by
`convert/{left,right}/FirmwareEncryptionTool.exe` (key `YSC_V2_8K_2026`). The
host slices it into 60-byte chunks — it does **not** re-encrypt per chunk.

## Firmware auto-update (probe latest from backend)

The Electron app (`../ysc_8k_electron/`) has a **「检查更新」(Check for Updates)**
button in both firmware panels that probes a central backend for the latest release
and auto-downloads it — no manual file pick. This is pure HTTP in the Electron main
process (`src/main/firmware-updater.ts`); the C++ driver is **not** involved.

- **Backend**: `firmware_server/` (Spring Boot, mirrors `shifei/kfc_v3`), deployed
  at `http://ysc-order-2026.top/fwapi/` (Nginx → loopback:8082).
- **Check URL** (hardcoded in `firmware-updater.ts`, override via `YSC_FW_BASE_URL`):
  ```
  GET http://ysc-order-2026.top/fwapi/api/firmware/{deviceType}/latest
  ```
- **deviceType catalog**:
  | deviceType | Panel | Source encrypted `.bin` |
  |---|---|---|
  | `ysc_v2_8k_mouse` | v1 FirmwarePanel | `ysc_8k_conver_mouse_hard/*.bin`(固件仓库的加密工具产出) |
  | `ysc_towmcu_left` | v2 TowmcuFirmwarePanel | `convert/left/ysc_left_app.bin` |
  | `ysc_towmcu_right` | v2 TowmcuFirmwarePanel | `convert/right/ysc_right_app.bin` |
- **Cache dir**: `%APPDATA%\YSC 8K 驱动\firmware-cache\` — downloads land here as
  `<deviceType>_v<version>.bin`; `installed.json` records the last-installed version
  per deviceType (drives the `isNewer` comparison; the device's own version string is
  display-only, not used for compare — it's non-semver on the single-MCU product).
- **Download** is sha256-verified against the backend's hash before being offered
  to `iap_start`/`towmcu_start`.

Command-line test against the backend (no UI needed):

```bash
# health
curl http://ysc-order-2026.top/fwapi/api/firmware/health
# latest for a deviceType
curl http://ysc-order-2026.top/fwapi/api/firmware/ysc_towmcu_left/latest
# download (sha256 in X-Firmware-Sha256 header)
curl -OJ http://ysc-order-2026.top/fwapi/api/firmware/download/ysc_towmcu_left_v1.0.1.bin
```

See `../ysc_8k_electron/docs/firmware-update-check.md` for the end-user guide.
