# ysc_8k_driver

Win32 tray application (C++17 / VS 2019 v142 / x64) that owns **all** serial
I/O for the YSC 8K host software. The Electron app (`../ysc_8k_electron/`)
talks to it over a named pipe — it never opens a COM port directly.

## Module layout

| File | Role |
|---|---|
| `main.cpp` | App entry, tray icon, named-pipe command dispatch (`OnPipeCommand`) |
| `serial_port.{h,cpp}` | `SerialPort` class — overlapped COM I/O + `<START><len><END>` framing for the v1 single-MCU UART path |
| `command_bridge.{h,cpp}` | JSON command builders (`SendJumpIAP` = `{"cmd":50}`, `SendUploadStatus`, debug capture …) |
| `iap_upgrader.{h,cpp}` | **v1** IAP upgrade — single UART MCU, baud probe + switch to 1.5 Mbaud |
| `towmcu_cdc.{h,cpp}` | **v2** SetupAPI CDC port enumeration (VID `0x1A86`/PID `0xFE0C` + USB iSerialNumber) |
| `towmcu_iap_upgrader.{h,cpp}` | **v2** IAP upgrade over USB-CDC (dual-MCU board) |
| `pipe_server.{h,cpp}` | Named-pipe server `\\.\pipe\ysc_8k_driver` |
| `kmboxnet_server.{h,cpp}` | kmbox network UDP bridge |
| `monitor_push.{h,cpp}` | Mouse-report monitor |

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
