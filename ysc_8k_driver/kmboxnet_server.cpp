#include "kmboxnet_server.h"
#include "command_bridge.h"
#include "main.h"
#include "debug_logger.h"
#include <cstring>
#include <cstdio>

KmboxnetServer::KmboxnetServer()
    : m_sock(INVALID_SOCKET)
    , m_port(0)
    , m_hThread(NULL)
    , m_running(false)
    , m_hasMonitorClient(false) {
    InitializeCriticalSection(&m_csMonitor);
    memset(&m_monitorClient, 0, sizeof(m_monitorClient));
}

KmboxnetServer::~KmboxnetServer() {
    Stop();
    DeleteCriticalSection(&m_csMonitor);
}

bool KmboxnetServer::Start(uint16_t port) {
    if (m_running) return false;

    m_sock = socket(AF_INET, SOCK_DGRAM, 0);
    if (m_sock == INVALID_SOCKET) return false;

    sockaddr_in addr = {};
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = INADDR_ANY;
    addr.sin_port = htons(port);

    if (bind(m_sock, (sockaddr*)&addr, sizeof(addr)) == SOCKET_ERROR) {
        closesocket(m_sock);
        m_sock = INVALID_SOCKET;
        return false;
    }

    // 3s receive timeout for responsive shutdown
    DWORD timeout = 3000;
    setsockopt(m_sock, SOL_SOCKET, SO_RCVTIMEO, (char*)&timeout, sizeof(timeout));

    m_port = port;
    m_running = true;
    m_hThread = CreateThread(NULL, 0, RecvThreadProc, this, 0, NULL);
    if (!m_hThread) {
        closesocket(m_sock);
        m_sock = INVALID_SOCKET;
        m_running = false;
        return false;
    }
    SetThreadPriority(m_hThread, THREAD_PRIORITY_ABOVE_NORMAL);

    return true;
}

void KmboxnetServer::Stop() {
    m_running = false;
    if (m_sock != INVALID_SOCKET) {
        closesocket(m_sock);
        m_sock = INVALID_SOCKET;
    }
    if (m_hThread) {
        WaitForSingleObject(m_hThread, 5000);
        CloseHandle(m_hThread);
        m_hThread = NULL;
    }
    EnterCriticalSection(&m_csMonitor);
    m_hasMonitorClient = false;
    LeaveCriticalSection(&m_csMonitor);
}

bool KmboxnetServer::IsRunning() const { return m_running; }
uint16_t KmboxnetServer::GetPort() const { return m_port; }

bool KmboxnetServer::HasMonitorClient() const {
    return m_hasMonitorClient;
}

void KmboxnetServer::SendMonitorData(const monitor_packet_t &pkt) {
    EnterCriticalSection(&m_csMonitor);
    if (!m_hasMonitorClient) {
        LeaveCriticalSection(&m_csMonitor);
        return;
    }
    sockaddr_in dest = m_monitorClient;
    LeaveCriticalSection(&m_csMonitor);

    sendto(m_sock, (const char*)&pkt, sizeof(pkt), 0,
           (sockaddr*)&dest, sizeof(dest));
}

DWORD WINAPI KmboxnetServer::RecvThreadProc(LPVOID param) {
    static_cast<KmboxnetServer*>(param)->RecvLoop();
    return 0;
}

void KmboxnetServer::RecvLoop() {
    uint8_t buf[1080]; // max: header(16) + payload(1024)
    sockaddr_in fromAddr = {};
    int fromLen = sizeof(fromAddr);

    while (m_running) {
        int n = recvfrom(m_sock, (char*)buf, sizeof(buf), 0,
                         (sockaddr*)&fromAddr, &fromLen);
        if (n <= 0) continue;
        if (n < (int)sizeof(kmboxnet_header_t)) continue;

        HandlePacket(buf, n, fromAddr);
    }
}

void KmboxnetServer::HandlePacket(const uint8_t *data, int len,
                                  const sockaddr_in &fromAddr) {
    kmboxnet_header_t hdr;
    memcpy(&hdr, data, sizeof(hdr));

    char addrStr[INET_ADDRSTRLEN];
    inet_ntop(AF_INET, &fromAddr.sin_addr, addrStr, sizeof(addrStr));
    DebugLog("UDP recv cmd=0x%08X mac=0x%08X idx=%u from=%s:%d len=%d",
             hdr.cmd, hdr.mac, hdr.indexpts,
             addrStr, ntohs(fromAddr.sin_port), len);
    DebugLogger::Log("UDP RECV cmd=0x%08X idx=%u from=%s:%d len=%d",
             hdr.cmd, hdr.indexpts,
             addrStr, ntohs(fromAddr.sin_port), len);

    switch (hdr.cmd) {
    case CMD_CONNECT:
        DebugLogger::Log("UDP CMD_CONNECT -> ACK");
        SendResponse(fromAddr, hdr);
        break;

    case CMD_MOUSE_MOVE: {
        if (len < (int)(sizeof(kmboxnet_header_t) + sizeof(soft_mouse_t))) break;
        soft_mouse_t mouse;
        memcpy(&mouse, data + sizeof(kmboxnet_header_t), sizeof(mouse));
        DebugLogger::Log("UDP CMD_MOUSE_MOVE x=%d y=%d", mouse.x, mouse.y);
        CommandBridge::SendMouseMove(mouse.x, mouse.y);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_MOUSE_AUTOMOVE: {
        if (len < (int)(sizeof(kmboxnet_header_t) + sizeof(soft_mouse_t))) break;
        soft_mouse_t mouse;
        memcpy(&mouse, data + sizeof(kmboxnet_header_t), sizeof(mouse));
        // rand field contains time in ms, estimate step count
        int steps = (hdr.rand > 0) ? (int)(hdr.rand / 16) : 1;
        if (steps < 1) steps = 1;
        if (steps > 1000) steps = 1000;
        DebugLogger::Log("UDP CMD_MOUSE_AUTOMOVE x=%d y=%d steps=%d", mouse.x, mouse.y, steps);
        CommandBridge::SendMouseMove(mouse.x, mouse.y, steps);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_BAZER_MOVE: {
        if (len < (int)(sizeof(kmboxnet_header_t) + sizeof(soft_mouse_t))) break;
        soft_mouse_t mouse;
        memcpy(&mouse, data + sizeof(kmboxnet_header_t), sizeof(mouse));
        int steps = (hdr.rand > 0) ? (int)(hdr.rand / 16) : 1;
        if (steps < 1) steps = 1;
        if (steps > 1000) steps = 1000;
        DebugLogger::Log("UDP CMD_BAZER_MOVE x=%d y=%d steps=%d", mouse.x, mouse.y, steps);
        CommandBridge::SendMoveTow(mouse.x, mouse.y, steps);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_MOUSE_LEFT:
    case CMD_MOUSE_RIGHT:
    case CMD_MOUSE_MIDDLE: {
        if (len < (int)(sizeof(kmboxnet_header_t) + sizeof(soft_mouse_t))) break;
        soft_mouse_t mouse;
        memcpy(&mouse, data + sizeof(kmboxnet_header_t), sizeof(mouse));
        DebugLog("  raw button=0x%08X x=%d y=%d", mouse.button, mouse.x, mouse.y);
        uint8_t btnMask = 0;
        bool pressed = false;
        const char *btnName = "UNKNOWN";
        if (hdr.cmd == CMD_MOUSE_LEFT) {
            btnMask = 1;
            pressed = (mouse.button & 1) != 0;
            btnName = "LEFT";
        } else if (hdr.cmd == CMD_MOUSE_RIGHT) {
            btnMask = 2;
            pressed = (mouse.button & 2) != 0;
            btnName = "RIGHT";
        } else if (hdr.cmd == CMD_MOUSE_MIDDLE) {
            btnMask = 4;
            pressed = (mouse.button & 4) != 0;
            btnName = "MIDDLE";
        }
        DebugLogger::Log("UDP CMD_MOUSE_%s pressed=%d btn=0x%08X", btnName, (int)pressed, mouse.button);
        CommandBridge::SendMouseButton(btnMask, pressed);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_MOUSE_WHEEL: {
        if (len < (int)(sizeof(kmboxnet_header_t) + sizeof(soft_mouse_t))) break;
        soft_mouse_t mouse;
        memcpy(&mouse, data + sizeof(kmboxnet_header_t), sizeof(mouse));
        DebugLogger::Log("UDP CMD_MOUSE_WHEEL wheel=%d", mouse.wheel);
        CommandBridge::SendMouseWheel(mouse.wheel);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_KEYBOARD_ALL: {
        if (len < (int)(sizeof(kmboxnet_header_t) + sizeof(soft_keyboard_t))) break;
        soft_keyboard_t kb;
        memcpy(&kb, data + sizeof(kmboxnet_header_t), sizeof(kb));
        DebugLogger::Log("UDP CMD_KEYBOARD_ALL ctrl=0x%02X", kb.ctrl);
        CommandBridge::SendKeyboard(kb.ctrl, kb.button);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_MONITOR: {
        uint16_t clientPort = (uint16_t)(hdr.rand & 0xFFFF);
        DebugLogger::Log("UDP CMD_MONITOR port=%u", clientPort);
        EnterCriticalSection(&m_csMonitor);
        if (clientPort == 0) {
            m_hasMonitorClient = false;
            CommandBridge::SendUploadStatus(false);
        } else {
            m_monitorClient = fromAddr;
            m_monitorClient.sin_port = htons(clientPort);
            m_hasMonitorClient = true;
            CommandBridge::SendUploadStatus(true);
        }
        LeaveCriticalSection(&m_csMonitor);
        SendResponse(fromAddr, hdr);
        break;
    }

    default:
        // Unknown command, still acknowledge
        SendResponse(fromAddr, hdr);
        break;
    }
}

void KmboxnetServer::SendResponse(const sockaddr_in &dest,
                                  const kmboxnet_header_t &reqHdr) {
    kmboxnet_header_t resp = reqHdr;
    resp.rand = 0; // 0 = success
    sendto(m_sock, (char*)&resp, sizeof(resp), 0,
           (sockaddr*)&dest, sizeof(dest));
}
