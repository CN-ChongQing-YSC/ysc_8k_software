#ifndef HUMAN_GUARD_H
#define HUMAN_GUARD_H

#include <cstdint>

// 物理护栏（Physical Guard）—— 驱动侧“人类最快反向间隔”测量。
//
// 数据源：固件的物理鼠标监控帧
//   {"code":200,"message":null,"data":"{\"b\":..,\"x\":..,\"y\":..,\"wh\":..,\"t\":..}"}
// 固件在 1ms 合并窗口内累加物理 X/Y，并附带毫秒时基 t；老固件没有 t 时退回
// 主机 GetTickCount64()。测量只在校准期间进行（OnSerialFrame 非活动时零开销）。
//
// 每个轴独立维护方向状态机：
//   - 同向样本清零反向累计；
//   - 反向累计达到 HG_AMP 才认为方向翻转；
//   - 两次翻转之间的 dt 落入 [HG_MIN_MS, HG_MAX_MS] 才记录；
//   - 结束/查询时从直方图计算 min / p5 / median / suggest。
//
// 结果通过管道事件 "human_guard" 推给前端；命令由 main.cpp 的 OnPipeCommand 转发。
class HumanGuard {
public:
    static void Init();
    static void Shutdown();

    // 串口读线程：收到一帧就调用（内部先判断是否在校准，非活动直接返回）。
    static void OnSerialFrame(const uint8_t *data, int len);

    // 管道线程：开始一次测量。axis 取 "x"/"y"（缺省 x），duration 默认 5000ms。
    static bool Begin(const char *axis, int durationMs);

    // 管道线程：请求提前结束（工作线程结算并推 done）。
    static void Stop();

    // 管道线程：回当前状态/上次结果。
    static void SendStatus();

    // 设备断开/驱动退出：若在测量中，推 error 事件。
    static void Abort(const char *reason);

    // 清空状态与上次结果（不影响固件配置）。
    static void Reset();
};

#endif // HUMAN_GUARD_H
