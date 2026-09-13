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
    // 用户是否手动指定过角色。不能用 (autoDetected==false) 反推 ——
    // 自动识别完成后也会把 autoDetected 置 true，反推会把"自动判定为模糊态"
    // 误当成"用户指定"，从而挡掉数据流判据。
    bool        roleManual    = false;
    int         role          = DEV_ROLE_NONE;
    int         probeState    = 0;       // 0=未探测 1=探测中 2=已判定
    /* 探测是否已有结论（成功判定 或 超时放弃）。
     * 必须与 probeState 分开：probeState 同时被当作"阶段"用时，
     * "已发出 711 等待回包"会与"已判定"撞成同一个值，导致回包被丢弃。 */
    uint8_t     probeDone     = 0;
    DWORD       probeDeadline = 0;       // GetTickCount() 超时
    // 上游设备身份（探测得到，UI 展示用）
    int         vid           = -1;
    int         pid           = -1;
    char        product[48]   = {};
    int         ifaceMask     = 0;       // bit0=见到 keyboard iface, bit1=见到 mouse iface
    // 数据流兜底判据：不依赖固件支持调试命令。旧固件不认 cmd 715/711 时，
    // 靠"设备实际上报的是鼠标还是键盘数据"也能定角色。
    uint8_t     sawMouseData  = 0;       // 收到过 message:"mouse"
    uint8_t     sawKbdData    = 0;       // 收到过 message:"kbd"
    // 探测两块回包是否到齐。不能只看"回包来过"：
    //   * 715(身份) 和 711(描述符) 是两条独立响应；
    //   * 设备可能仍处在上一次 debug_enter 留下的调试模式里，回包会先于
    //     本次探测就绪到达，导致只收到其中一块就下结论（vid/pid 为 0）。
    // 两块都到齐（或超时）才置 probeDone。
    uint8_t     probeGotInfo  = 0;       // 收到 715
    uint8_t     probeGotCfg   = 0;       // 收到 711
    // 描述符判据是否"分不出来"：同款透传盒子自身就带键盘+鼠标两个接口，
    // 两支盒子的 ifaceMask 都是 3，靠描述符**天然无法区分**上游插的是哪个。
    // 这种情况必须让位给"设备实际上报哪类数据"的判据。
    uint8_t     probeAmbiguous = 0;

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

    // 由数据流回调调用：记录该设备实际上报过哪类数据，作为角色识别的兜底判据。
    // kind: 0=鼠标数据, 1=键盘数据
    static void NoteDataKind(int slot, int kind);

    // UI 手动指定角色。role == DEV_ROLE_NONE 表示"交回自动识别"：
    // 会重新发起一轮探测（用户从下拉里选「自动」就是这个语义）。
    static bool SetRole(int slot, int role);

    /* 已连接设备切换波特率。
     * 串口两端都要改：SerialPort::SwitchBaudrate 先发 {"cmd":133,"baud":N} 让固件
     * 改 USART，再关掉重开主机侧句柄；随后必须重启读线程（切换过程停掉了它），
     * 否则设备显示"已连接"却再也收不到数据。放在 DeviceContext 内部做是因为
     * 数据回调（OnSerialDataForSlot）是本文件的私有静态函数，重挂回调需要它。
     * 成功后槽位 baud 更新、角色重新探测。 */
    static bool SwitchBaud(int slot, DWORD baud);

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
