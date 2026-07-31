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
