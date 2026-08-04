// ysc_sdk.h — YSC 8K 驱动 SDK（C ABI，DLL 导出）
// ============================================================================
// 这是一个独立、自包含的动态链接库（ysc_sdk.dll），封装了 ysc_8k_driver 里
// 经过验证的串口连接、波特率枚举/自动探测/切换、以及全部 YSC 协议命令。
//
// 易语言 / Python / C# / Delphi / VB 等任何语言都不需要再重复实现驱动逻辑，
// 只要加载本 DLL、调用下面这些 C 接口即可完成：
//   1) 列出串口（YSC 设备 / 全部 COM）
//   2) 连接串口（指定波特率 或 0=自动探测）
//   3) 切换波特率
//   4) 发送任意 YSC 协议命令（同步等待返回）或原始字节
//
// 协议帧格式（与 ysc_8k_driver/serial_port.cpp 完全一致，无校验）：
//   <START> <总长度 u16 大端> <UTF-8 JSON 负载> <END>
//   总长度 = 7 + 2 + len(json) + 5
//
// 线程安全：每个 YscDevice 句柄内部自带串行化锁；同一个句柄可在多线程中
// 调用，但不同句柄彼此独立（可同时连接多个 dongle）。
//
// 调用约定：使用 stdcall（WINAPI）—— 易语言/VB6/Delphi 的默认约定，
// 无需在调用方做任何设置即可直接调用。详见下方 YSC_CALL 注释。
// 本头文件按标准 C 导出（extern "C"）。
// ============================================================================

#ifndef YSC_SDK_H
#define YSC_SDK_H

#include <stdint.h>

// 导出宏
//   x64：用 __declspec(dllexport)，导出名天然干净（Ysc_Xxx）。
//   x86：不 dllexport（否则会多出一组 _Ysc_Xxx@N 修饰名），改由 ysc_sdk.def
//        导出干净的 Ysc_Xxx 名。两种平台最终都只暴露干净的 Ysc_Xxx。
#if defined(_MSC_VER)
    #ifdef YSC_SDK_EXPORTS          // 编译 DLL 本体时由工程宏定义
        #ifdef _WIN64
            #define YSC_API __declspec(dllexport)
        #else
            #define YSC_API          // x86 由 ysc_sdk.def 导出
        #endif
    #else
        #define YSC_API __declspec(dllimport)
    #endif
#else
    #define YSC_API
#endif

// 调用约定：使用 stdcall（WINAPI）。
//   - 易语言 Dll命令、VB6、Delphi 默认就是 stdcall，可直接调用，无需额外设置。
//   - 32 位(x86)下 stdcall 会把导出名修饰成 _Ysc_Xxx@N，因此工程里带 ysc_sdk.def
//     把导出名还原成干净的 Ysc_Xxx（.def 仅用于 Win32 配置）。
//   - 64 位(x64)只有一种调用约定，stdcall 与 cdecl 二进制等价，导出名天然干净。
#ifndef YSC_CALL
    #if defined(_WIN32) || defined(_WIN64)
        #define YSC_CALL __stdcall
    #else
        #define YSC_CALL
    #endif
#endif

#ifdef __cplusplus
extern "C" {
#endif

// 不透明连接句柄。一次 Ysc_Connect 返回一个，对应一个串口。
typedef struct YscDevice YscDevice;

// 返回值约定（带 out 缓冲区的查询类接口）：
//   > 0  ：成功，写入 outBuf 的字节数（不含末尾 '\0'）
//   = 0  ：成功但负载为空，或等待超时无返回
//   < 0  ：错误（调用 Ysc_LastError 取错误信息）
//   若返回值 > bufSize-1，表示缓冲区不足、内容被截断（snprintf 语义）。

// ----------------------------------------------------------------------------
// 0. 信息
// ----------------------------------------------------------------------------

// 返回 SDK 版本号字符串，例如 "1.0.0"
YSC_API const char* YSC_CALL Ysc_SdkVersion(void);

// 返回线程局部的最近一次错误描述（不可 free）。无错误时返回 ""。
YSC_API const char* YSC_CALL Ysc_LastError(void);

// 返回本机支持的波特率列表（静态数组），通过 *count 返回元素个数。
// 当前为：115200 230400 460800 921600 1000000 1500000 2000000 3000000 4000000
YSC_API const uint32_t* YSC_CALL Ysc_SupportedBaudrates(int *count);

// ----------------------------------------------------------------------------
// 1. 串口枚举
// ----------------------------------------------------------------------------

// 枚举本机 YSC towmcu CDC 串口（USB VID=0x1A86 / PID=0xFE0C）。
// 把结果以 JSON 数组写入 outBuf，形如：
//   [{"port":"COM7","serial":"TOWMCULEFT","side":"Left","desc":"USB Serial Device (COM7)"}, ...]
// 没有 YSC 设备时写入 "[]"。返回值见顶部“返回值约定”。
YSC_API int YSC_CALL Ysc_ListPorts(char *outBuf, int bufSize);

// 枚举本机全部 COM 串口（不区分厂商），写入 JSON 数组：
//   [{"port":"COM3","desc":"..."},{"port":"COM7","desc":"..."}]
YSC_API int YSC_CALL Ysc_ListAllComPorts(char *outBuf, int bufSize);

// ----------------------------------------------------------------------------
// 2. 连接生命周期
// ----------------------------------------------------------------------------

// 打开一个串口连接。
//   portName  ："COM7" 等
//   baudRate  ：波特率；传 0 表示自动探测（依次试探 SUPPORTED_BAUDS）
//   errBuf    ：可选，失败时写入错误描述（可传 NULL/0）
// 成功返回句柄，失败返回 NULL（见 errBuf / Ysc_LastError）。
YSC_API YscDevice* YSC_CALL Ysc_Connect(const char *portName,
                                        uint32_t baudRate,
                                        char *errBuf, int errBufSize);

// 关闭并释放连接（传 NULL 安全）。
YSC_API void YSC_CALL Ysc_Disconnect(YscDevice *dev);

// 自动探测某串口当前硬件波特率（不打开长连接）。
// 返回探测到的波特率，失败返回 0。
YSC_API uint32_t YSC_CALL Ysc_DetectBaudrate(const char *portName);

// 是否已连接。
YSC_API int YSC_CALL Ysc_IsConnected(YscDevice *dev);

// 当前波特率（未连接返回 0）。
YSC_API uint32_t YSC_CALL Ysc_GetBaudrate(YscDevice *dev);

// 当前串口名写入 outBuf，返回值见顶部约定。
YSC_API int YSC_CALL Ysc_GetPortName(YscDevice *dev, char *outBuf, int bufSize);

// ----------------------------------------------------------------------------
// 3. 波特率切换
// ----------------------------------------------------------------------------

// 切换波特率：发送 {"cmd":133,"baud":N} -> 关闭 -> 以新波特率重开 -> 用
// {"cmd":132} 版本查询校验。成功返回 1，失败返回 0。
// newBaud 必须在 Ysc_SupportedBaudrates 列表内。
YSC_API int YSC_CALL Ysc_SwitchBaudrate(YscDevice *dev, uint32_t newBaud);

// ----------------------------------------------------------------------------
// 4. 核心：发送命令 / 原始数据
// ----------------------------------------------------------------------------

// 发送一帧 JSON 命令，并同步等待下一条带帧响应（timeoutMs 毫秒）。
// 把响应负载（不含 <START>/<END>/长度）写入 outBuf（'\0' 结尾）。返回值见顶部约定。
// timeoutMs<=0 表示使用默认 1000ms。
YSC_API int YSC_CALL Ysc_SendCommand(YscDevice *dev, const char *json,
                                      int timeoutMs, char *outBuf, int bufSize);

// 发送一帧 JSON 命令但不等待返回（用于鼠标移动等即发即弃命令）。成功返回 1。
YSC_API int YSC_CALL Ysc_SendCommandNoWait(YscDevice *dev, const char *json);

// 发送原始字节（不加帧），用于 MAKCU 文本协议。成功返回 1。
YSC_API int YSC_CALL Ysc_SendRaw(YscDevice *dev, const uint8_t *data, int len);

// 查询设备版本（发送 {"cmd":132}），响应写入 outBuf。返回值见顶部约定。
YSC_API int YSC_CALL Ysc_QueryVersion(YscDevice *dev, int timeoutMs,
                                      char *outBuf, int bufSize);

// ----------------------------------------------------------------------------
// 5. 常用 YSC 命令的便捷封装
//    命令码与 command_bridge.cpp 一致。移动/按键类（无可靠返回）用 NoWait；
//    查询类（有返回）等待响应并写入 outBuf。
// ----------------------------------------------------------------------------

// 鼠标：cmd 30 绝对移动 / 31 相对移动 / 33 按键
YSC_API int YSC_CALL Ysc_MouseMove(YscDevice *dev, int x, int y, int steps);
YSC_API int YSC_CALL Ysc_MouseMoveTow(YscDevice *dev, int x, int y, int steps);
YSC_API int YSC_CALL Ysc_MouseButton(YscDevice *dev, uint8_t buttonMask, int pressed);

// 键盘：cmd 45 单键注入（kc 为 HID 键码 0x04-0xE7，修饰键 0xE0-0xE7）/ 46 释放全部 /
//       47 打印一段混合大小写 ASCII 字符串（固件按 CapsLock 状态自动决定大小写）
YSC_API int YSC_CALL Ysc_KeyboardKey(YscDevice *dev, uint8_t keycode, int down);
YSC_API int YSC_CALL Ysc_KeyboardReleaseAll(YscDevice *dev);
// s 必须非空且 <= 128 字节；空或超长返回 0（见 Ysc_LastError）。NoWait 即发即弃。
YSC_API int YSC_CALL Ysc_KeyboardTypeString(YscDevice *dev, const char *s);

// 上报状态开关：cmd 34
YSC_API int YSC_CALL Ysc_UploadStatus(YscDevice *dev, int enable);

// 跳转 IAP 升级模式：cmd 50
YSC_API int YSC_CALL Ysc_JumpIAP(YscDevice *dev);

// 手柄↔鼠标映射（cmd 100/101/102/104，固件负责持久化）
YSC_API int YSC_CALL Ysc_GamepadGetConfig(YscDevice *dev, int timeoutMs,
                                          char *outBuf, int bufSize);
// configJson 为映射对象，如 {"axis":[...]}，会被包进 {"cmd":100,"data":{...}}
YSC_API int YSC_CALL Ysc_GamepadSetConfig(YscDevice *dev, const char *configJson);
YSC_API int YSC_CALL Ysc_GamepadEnable(YscDevice *dev, int on);
YSC_API int YSC_CALL Ysc_GamepadReset(YscDevice *dev);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // YSC_SDK_H
