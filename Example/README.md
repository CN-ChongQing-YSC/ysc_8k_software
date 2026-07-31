# Example — 各语言接入 ysc_sdk.dll 示例

本目录提供**多种语言**调用 `ysc_sdk.dll` 的示例。所有语言共用**同一个 DLL**，
都无需自己实现串口连接 / 波特率切换 / YSC 协议帧编解码——这些已被 `ysc_sdk.dll`
封装好（实现见 `ysc_8k_driver/ysc_sdk.cpp`，逻辑严格对照已验证的
`serial_port.cpp` / `command_bridge.cpp` / `towmcu_cdc.cpp`）。

## 目录结构

```
Example/
├── python/        # Python(ctypes) 封装 + 完整自动化测试（推荐先跑这个）
│   ├── ysc_sdk.py     # 面向对象的 ctypes 绑定
│   └── test_dll.py    # 完整测试 27 个导出接口（含/不含硬件均可）
├── 易语言/         # 32 位 / 64 位易语言接入（Dll命令声明 + 完整示例）
│   └── README.md
└── csharp/        # C# / .NET (P/Invoke) 接入
    ├── YscSdk.cs      # 封装类
    └── Program.cs     # 示例 Main
```

## DLL 在哪、用哪个

编译产物在 `ysc_8k_driver/bin/`：

| 架构 | 路径 | 给谁用 |
|---|---|---|
| 64 位 | `ysc_8k_driver/bin/x64/Release/ysc_sdk.dll` | Python(64位)、C#(64位)、易语言6.x(64位)、Node 等 |
| **32 位** | `ysc_8k_driver/bin/Win32/Release/ysc_sdk.dll` | **易语言(32位, 主流)**、VB6、Delphi(32位) |

> **位数必须匹配**：进程是几位，就只能加载几位的 DLL。
> DLL 用静态 CRT 编译（/MT），**目标机无需安装 VC 运行库**。

## 编译 DLL

Visual Studio（本仓库用 VS2022/18.x，工具集 v145）打开
`ysc_8k_driver/ysc_sdk.sln`，或命令行：

```bash
# 在 VS Developer Prompt 里
msbuild ysc_8k_driver/ysc_sdk.sln /p:Configuration=Release /p:Platform=x64
msbuild ysc_8k_driver/ysc_sdk.sln /p:Configuration=Release /p:Platform=x86   # 32 位易语言用
```

## 核心 API（C ABI，27 个导出函数）

| 类别 | 函数 |
|---|---|
| 信息 | `Ysc_SdkVersion` `Ysc_LastError` `Ysc_SupportedBaudrates` |
| 枚举 | `Ysc_ListPorts`（YSC 设备） `Ysc_ListAllComPorts`（全部 COM） |
| 连接 | `Ysc_Connect` `Ysc_Disconnect` `Ysc_IsConnected` `Ysc_GetBaudrate` `Ysc_GetPortName` `Ysc_DetectBaudrate` |
| 波特率 | `Ysc_SwitchBaudrate` |
| 核心发送 | `Ysc_SendCommand`(同步等返回) `Ysc_SendCommandNoWait` `Ysc_SendRaw` `Ysc_QueryVersion` |
| 鼠标 | `Ysc_MouseMove` `Ysc_MouseMoveTow` `Ysc_MouseButton` |
| 键盘 | `Ysc_KeyboardKey` `Ysc_KeyboardReleaseAll` |
| 其它 | `Ysc_UploadStatus` `Ysc_JumpIAP` |
| 手柄映射 | `Ysc_GamepadGetConfig` `Ysc_GamepadSetConfig` `Ysc_GamepadEnable` `Ysc_GamepadReset` |

详细签名见 `ysc_8k_driver/ysc_sdk.h`（带详尽中文注释）。

## 调用约定

DLL 统一用 **stdcall**（易语言/VB6/Delphi 默认），导出名干净（`Ysc_Connect` 等）：
- x64：stdcall 与 cdecl 二进制等价，任意语言直接调（Python `ctypes`、C# `StdCall` 都行）。
- x86（32 位易语言）：**易语言不用设"调用方式"**，默认即匹配；32 位导出名靠
  `ysc_sdk.def` 还原成干净名。若报"调用DLL命令后发现堆栈错误"，说明调用约定没对上——
  本 DLL 已统一 stdcall，按各语言默认即可。
- C# 用 `CallingConvention.StdCall`；Python 用 `WinDLL`（封装已处理好）。

## 协议帧（仅供理解，调用方无需关心）

```
<START> <总长度 u16 大端> <UTF-8 JSON 负载> <END>
总长度 = 7 + 2 + len(json) + 5
```
由 DLL 自动编解码。命令码对照 `ysc_8k_driver/command_bridge.cpp`。
