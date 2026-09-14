#ifndef KMBOXNET_SERVER_H
#define KMBOXNET_SERVER_H

#define WIN32_LEAN_AND_MEAN
#include <winsock2.h>
#include <ws2tcpip.h>
#include "kmboxnet_proto.h"

class KmboxnetServer {
public:
    KmboxnetServer();
    ~KmboxnetServer();

    bool Start(uint16_t port);
    void Stop();
    bool IsRunning() const;
    uint16_t GetPort() const;

    // Push monitor data to registered client
    void SendMonitorData(const monitor_packet_t &pkt);

    bool HasMonitorClient() const;

    // 供 UI 预览：客户端连接信息 + 命令调用统计 + 最近若干条调用记录。
    // 返回写入 out 的字符数（不含结尾 NUL）。线程安全。
    int  GetStatJson(char *out, int outSize) const;

private:
    static DWORD WINAPI RecvThreadProc(LPVOID param);
    void RecvLoop();
    void HandlePacket(const uint8_t *data, int len,
                      const sockaddr_in &fromAddr);
    void SendResponse(const sockaddr_in &dest,
                      const kmboxnet_header_t &reqHdr);

    SOCKET          m_sock;
    uint16_t        m_port;
    HANDLE          m_hThread;
    volatile bool   m_running;

    // Monitor client
    CRITICAL_SECTION m_csMonitor;
    sockaddr_in     m_monitorClient;
    bool            m_hasMonitorClient;
};

#endif // KMBOXNET_SERVER_H
