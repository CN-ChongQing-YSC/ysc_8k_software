#ifndef IAP_UPGRADER_H
#define IAP_UPGRADER_H

#include <cstdint>
#include <string>
#include <vector>

class PipeServer;

class IAPUpgrader {
public:
    // 手动选本地文件路径（向后兼容：renderer 端 openIAPFile 仍走此入口）
    // targetBaud: 切换到的目标波特率；0 = 保持探测到的当前波特率不切换。
    static void Start(const char *firmwarePath, PipeServer *pipe, uint32_t targetBaud);
    // 在线下载直传：接受内存中的固件字节（管道 Base64 解码后调用）
    static void Start(const uint8_t *data, size_t len, PipeServer *pipe, uint32_t targetBaud);
    static void Cancel();
    static bool IsRunning();

private:
    static void Worker(std::vector<uint8_t> firmware, PipeServer *pipe, uint32_t targetBaud);
    static volatile bool s_running;
    static volatile bool s_cancel;
};

#endif // IAP_UPGRADER_H
