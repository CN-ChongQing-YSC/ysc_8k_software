#ifndef MONITOR_PUSH_H
#define MONITOR_PUSH_H

#include <cstdint>
#include "kmboxnet_proto.h"

class KmboxnetServer;

// 鼠标监控快照（来自鼠标设备的 {"code":200,"message":"mouse","data":"{b,x,y}"}）
struct MonitorState {
    int buttons = 0;   // 按键位图（bit0=左,1=右,2=中,3=侧1,4=侧2）
    int x = 0;         // 本次 X 位移
    int y = 0;         // 本次 Y 位移
    int wheel = 0;     // 滚轮
    int valid = 0;     // 是否收到过鼠标数据
};

// 键盘物理状态快照（来自键盘设备）
// 消息形如 {"code":200,"message":"kbd","data":"{\"mod\":N,\"n\":M,\"ov\":K,\"k\":[..]}"}
//   mod = 修饰键位图（bit0..7 = HID 0xE0..0xE7）
//   n   = key[] 有效项数；ov = 超出 6 个未列出的键数
//   k   = 非修饰 HID 键码，升序
//
// 注：该常量与固件 KeyboardController 的 KBD_STATE_KEYS_MAX 必须一致，但两个
// 仓库已解耦、驱动不能 include 固件头，因此在此独立定义。改固件时同步改这里。
#define KBD_MON_KEYS_MAX 6

struct KbdMonitorState {
    int     mod = 0;
    int     key_count = 0;
    int     overflow = 0;
    uint8_t key[KBD_MON_KEYS_MAX] = {};
    int     valid = 0;      // 是否收到过键盘数据
};

class MonitorPush {
public:
    static void Init(KmboxnetServer *server);

    // Called from serial read thread when framed data arrives
    static void OnSerialResponse(const uint8_t *data, int len, void *userData);

    // Returns the latest mouse monitor state (thread-safe read). 保留旧名字，
    // 单设备时代它就是"全部监控状态"。
    static MonitorState GetLatest();
    static KbdMonitorState GetKeyboard();

    // 鼠标 + 键盘合成一个 kmboxnet monitor 包。键盘侧没数据时 kb 区为
    // report_id=0x02 其余全 0（与单设备时代的输出一致）。
    static monitor_packet_t BuildMonitorPacket();

    // 任一侧状态变化时把合成包推给已注册的 monitor 客户端（无客户端时空转）
    static void PushIfChanged();

    // 设备断开时清空对应侧（isKeyboard=0 清鼠标，1 清键盘），并允许下一次推送
    static void ResetSide(int isKeyboard);

    // 供管道事件使用的 JSON 片段（不含外层大括号）
    static void SerializeMouseJson(char *out, int outSize);
    static void SerializeKbdJson(char *out, int outSize);
};

#endif // MONITOR_PUSH_H
