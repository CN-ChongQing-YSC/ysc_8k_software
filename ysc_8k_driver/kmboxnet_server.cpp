#include "kmboxnet_server.h"
#include "device_context.h"
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

// 上一次下发给盒子的"整份按键位图"，用于跳过重复下发（kmNet_mouse_wheel 每滚
// 一格都会带着未变的 button 影子状态来一次）。只在发送成功时更新 —— 失败就保持
// 旧值，下次同状态再来时能重试，不会像"乐观更新"那样留下粘键。
static uint8_t s_net_btn_mask  = 0;
static uint8_t s_net_btn_valid = 0;

static void NetApplyButtonMask(uint8_t want) {
    want &= 0x1F;
    if (s_net_btn_valid && want == s_net_btn_mask) return;
    if (CommandBridge::SendMouseButtonMask(want, (DeviceRole)DEV_ROLE_MOUSE)) {
        s_net_btn_mask  = want;
        s_net_btn_valid = 1;
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
        CommandBridge::SendMouseMove(mouse.x, mouse.y, 1, (DeviceRole)DEV_ROLE_MOUSE);
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
        CommandBridge::SendMouseMove(mouse.x, mouse.y, steps, (DeviceRole)DEV_ROLE_MOUSE);
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
        CommandBridge::SendMoveTow(mouse.x, mouse.y, steps, (DeviceRole)DEV_ROLE_MOUSE);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_MOUSE_LEFT:
    case CMD_MOUSE_RIGHT:
    case CMD_MOUSE_MIDDLE: {
        if (len < (int)(sizeof(kmboxnet_header_t) + sizeof(soft_mouse_t))) break;
        soft_mouse_t mouse;
        memcpy(&mouse, data + sizeof(kmboxnet_header_t), sizeof(mouse));
        const char *btnName = (hdr.cmd == CMD_MOUSE_LEFT)  ? "LEFT"
                            : (hdr.cmd == CMD_MOUSE_RIGHT) ? "RIGHT" : "MIDDLE";
        DebugLogger::Log("UDP CMD_MOUSE_%s button=0x%02X", btnName,
                         (unsigned)(mouse.button & 0x1F));
        /* 官方库这几个函数都是"整份按键位图"语义：客户端本地维护 button 影子，
         * 每次发全量。kmNet_mouse_side1/side2 更是直接借 cmd_mouse_right 下发
         * bit3/bit4 —— 只解析单个 bit 会把它们丢掉、把 mouse_all 也做废。
         * 所以这里整体覆盖固件的 serial_button_mask（固件 cmd 61）。 */
        NetApplyButtonMask((uint8_t)mouse.button);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_MOUSE_WHEEL: {
        if (len < (int)(sizeof(kmboxnet_header_t) + sizeof(soft_mouse_t))) break;
        soft_mouse_t mouse;
        memcpy(&mouse, data + sizeof(kmboxnet_header_t), sizeof(mouse));
        /* 官方 kmNet_mouse_wheel() 和 kmNet_mouse_all() **共用这条命令**：
         * 后者带全字段（button + x + y + wheel），前者只有 wheel（button 是客户端
         * 当前影子状态，重复下发等于没变）。所以三样都按"有值才发"处理，
         * 否则 kmNet_mouse_all() 的按键和位移会整条丢掉。 */
        DebugLogger::Log("UDP CMD_MOUSE_WHEEL wheel=%d btn=0x%02X x=%d y=%d",
                         mouse.wheel, (unsigned)(mouse.button & 0x1F),
                         mouse.x, mouse.y);
        NetApplyButtonMask((uint8_t)mouse.button);
        if (mouse.x || mouse.y)
            CommandBridge::SendMouseMove(mouse.x, mouse.y, 1, (DeviceRole)DEV_ROLE_MOUSE);
        if (mouse.wheel)
            CommandBridge::SendMouseWheel(mouse.wheel, (DeviceRole)DEV_ROLE_MOUSE);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_MASK_MOUSE:
    case CMD_UNMASK_ALL: {
        /* 位图在 hdr.rand：官方 kmNet_mask_mouse_*() 每次下发的是客户端累积的
         * mask_keyboard_mouse_flag 低 8 位，kmNet_unmask_all() 发的是同一个 flag。
         * 高 8 位的 keyboard 屏蔽位固件不支持（集火用不到物理键盘屏蔽），忽略。 */
        uint8_t m = (uint8_t)(hdr.rand & 0xFF);
        DebugLogger::Log("UDP MASK mouse=0x%02X", (unsigned)m);
        CommandBridge::SendMouseMask(m, (DeviceRole)DEV_ROLE_MOUSE);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_REBOOT: {
        /* 双设备下客户端把它当成"那台盒子"，两台都发（单设备时第二次是同一个口，
         * 失败也无害）。 */
        DebugLogger::Log("UDP CMD_REBOOT -> mouse+kbd");
        CommandBridge::SendReboot((DeviceRole)DEV_ROLE_MOUSE);
        CommandBridge::SendReboot((DeviceRole)DEV_ROLE_KBD);
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
            CommandBridge::SendUploadStatus(false, (DeviceRole)DEV_ROLE_MOUSE);
        } else {
            m_monitorClient = fromAddr;
            m_monitorClient.sin_port = htons(clientPort);
            m_hasMonitorClient = true;
            CommandBridge::SendUploadStatus(true, (DeviceRole)DEV_ROLE_MOUSE);
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
