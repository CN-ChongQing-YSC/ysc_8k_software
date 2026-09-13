#ifndef DEVICE_CONTEXT_H
#define DEVICE_CONTEXT_H

// 多 YSC v1 设备管理：一个"透传鼠标"设备 + 一个"透传键盘"设备各自独占一路串口。
//
// 背景：单设备时代驱动只有 g_serial 一个 SerialPort，命令也无设备维度。要在
// 同一台 PC 上同时接两台 YSC（一台上游插鼠标、一台上游插键盘）就需要：
//   1) 两路串口各自连接、读写、断线重连互不干扰；
//   2) 命令按"角色"路由：鼠标命令发给鼠标机，键盘命令发给键盘机，
//      调用方（UI / Net API）不必知道设备的存在；
//   3) 上行数据带上设备标识，UI 才能分别显示两台的真实物理状态。
//
// 角色而不是 COM 号做标识：用户换 USB 口时 COM 号会变，角色是需求本体。

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>

class SerialPort;

enum DeviceRole {
    DEV_ROLE_NONE  = 0,
    DEV_ROLE_MOUSE = 1,   // 上游插的是鼠标（透传鼠标数据）
    DEV_ROLE_KBD   = 2,   // 上游插的是键盘（透传键盘数据）
};

#define DEV_MAX_SLOTS 2

// 单台设备的运行时状态
struct DeviceSlot {
    SerialPort *serial        = nullptr;
    wchar_t     port[16]      = {};
    DWORD       baud          = 0;
    bool        connected     = false;
    bool        autoDetected  = false;   // 角色是自动识别（true）还是用户手动指定
    int         role          = DEV_ROLE_NONE;
    int         probeState    = 0;       // 0=未探测 1=探测中 2=已判定
    DWORD       probeDeadline = 0;       // GetTickCount() 超时
    // 上游设备身份（探测得到，UI 展示用）
    int         vid           = -1;
    int         pid           = -1;
    char        product[48]   = {};
    int         ifaceMask     = 0;       // bit0=见到 keyboard iface, bit1=见到 mouse iface

    bool InUse() const { return serial != nullptr; }
};

class DeviceContext {
public:
    // 返回 slot 下标（>=0），失败返回 -1
    static int  Attach(int slot, const wchar_t *port, DWORD baud);
    static void Detach(int slot);
    static void DetachAll();

    static DeviceSlot *Get(int slot);
    static int  Count();                       // 已使用槽位数

    // 按角色取设备；role 不存在时退回"第一个已连接设备"，保证单设备场景零回归
    static int  SlotByRole(int role);
    static SerialPort *SerialForRole(int role);
    static SerialPort *SerialForSlot(int slot);

    // 角色识别（自动）：对已连接的、角色未知的设备发 cmd 715/711 探测上游插的是什么。
    // 必须在主循环/管道线程周期性调用（内部有超时与节流）。
    static void ProbeTick();

    // UI 手动指定角色（autoDetected 置 false，人工优先）
    static bool SetRole(int slot, int role);

    // 串口数据回调：区分是哪个槽来的
    typedef void (*DataCallback)(int slot, const uint8_t *data, int len, void *userData);
    static void SetDataCallback(DataCallback cb, void *userData);

    // 串口掉线通知（读线程异常退出）时按 slot 回调
    typedef void (*LostCallback)(int slot, void *userData);
    static void SetLostCallback(LostCallback cb, void *userData);

    // 由 WM_SERIAL_LOST 处理：判断是哪个 slot 的 reader 掉了
    static int  SlotByReaderGen(LONG gen);
};

#endif // DEVICE_CONTEXT_H
