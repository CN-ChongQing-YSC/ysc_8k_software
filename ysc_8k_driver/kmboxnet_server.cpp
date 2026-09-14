#include "kmboxnet_server.h"
#include "device_context.h"
#include "command_bridge.h"
#include "main.h"
#include "pipe_server.h"   // g_pipeServer.SendEvent（NET 活动预览推送）
#include "debug_logger.h"
#include <cstring>
#include <cstdio>

/* 前向声明：Stop() 里要清掉 NET 活动统计/按键影子状态，定义在文件后半部分 */
static void NetApplyButtonMaskReset(void);
static void NetStat_Reset(void);

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
    NetApplyButtonMaskReset();   /* 重启服务后按键影子状态与盒子可能不一致，丢掉 */
    NetStat_Reset();   /* 每次启动服务从零开始统计，UI 预览不残留上次的会话 */
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

    /* 服务停了，预览里的"客户端在线/计数器"要一起清掉并通知前端 */
    NetApplyButtonMaskReset();
    NetStat_Reset();
    char body[1536];
    if (GetStatJson(body, sizeof(body)) > 0 && g_pipeServer.HasClient())
        g_pipeServer.SendEvent("kmnet_stat", body);
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

static bool NetApplyButtonMask(uint8_t want) {
    want &= 0x1F;
    if (s_net_btn_valid && want == s_net_btn_mask) return true;   /* 状态已一致也算成功 */
    if (CommandBridge::SendMouseButtonMask(want, (DeviceRole)DEV_ROLE_MOUSE)) {
        s_net_btn_mask  = want;
        s_net_btn_valid = 1;
        return true;
    }
    return false;
}

static void NetApplyButtonMaskReset(void) {
    s_net_btn_mask  = 0;
    s_net_btn_valid = 0;
}

/* ===== UI 预览：NET 服务活动统计 =====
 * 「键鼠集火」面板要能一眼看出：服务有没有被连上、最近都调了什么命令、命令有没有
 * 真的转到盒子上。做法：
 *   - 每条命令都累加计数并写进一个 10 条的环形记录（UI 只做"简单预览"）；
 *   - 高频鼠标命令的**推送**限速 NET_PUSH_MIN_MS，连接/监控/屏蔽/重启等低频但
 *     重要的命令立即推；
 *   - UI 另有 get_kmnet_stat 主动拉取兜底（首屏、漏推、重连后）。
 * ring 记录里的 ok=1 表示命令确实下发到了目标盒子的串口；ok=0 表示没解析到
 * 目标设备（两台都连着而角色没指定时会被拒），这时前端显示为"未转接"。 */
#define NET_RING_MAX     10
#define NET_PUSH_MIN_MS  200
#define NET_CLIENT_ACTIVE_MS 10000   /* 10s 内有命令即视为"客户端在线" */

typedef struct {
    uint32_t seq;
    uint32_t ms;          /* 距本次服务启动的毫秒 */
    uint8_t  kind;        /* 0=鼠标 1=键盘 2=控制 3=未识别 */
    uint8_t  ok;
    char     name[12];
    char     detail[40];
} NetCmdRec;

static CRITICAL_SECTION s_csNet;
static bool       s_csNetInit = false;
static uint32_t   s_netTotal = 0;
static uint32_t   s_netKindCnt[4] = {0, 0, 0, 0};
static NetCmdRec  s_netRing[NET_RING_MAX];
static int        s_netRingCount = 0;   /* 已写入条数（<= NET_RING_MAX） */
static int        s_netRingHead  = 0;   /* 下一个写入位置 */
static uint32_t   s_netSeq = 0;
static char       s_clientIp[32] = "";
static uint16_t   s_clientPort = 0;
static ULONGLONG  s_clientSince = 0;
static ULONGLONG  s_clientLast  = 0;
static uint32_t   s_clientSessions = 0;
static ULONGLONG  s_netLastPush = 0;
static ULONGLONG  s_netStartTick = 0;

static void NetStat_EnsureCs(void) {
    if (!s_csNetInit) {
        InitializeCriticalSection(&s_csNet);
        s_csNetInit = true;
    }
}

static void NetStat_Reset(void) {
    NetStat_EnsureCs();
    EnterCriticalSection(&s_csNet);
    s_netTotal = 0;
    memset(s_netKindCnt, 0, sizeof(s_netKindCnt));
    memset(s_netRing, 0, sizeof(s_netRing));
    s_netRingCount = 0;
    s_netRingHead  = 0;
    s_netSeq = 0;
    s_clientIp[0] = '\0';
    s_clientPort = 0;
    s_clientSince = s_clientLast = 0;
    s_clientSessions = 0;
    s_netStartTick = GetTickCount64();
    LeaveCriticalSection(&s_csNet);
}

/* 调用方需持有 s_csNet */
static int NetStat_BuildLocked(char *out, int outSize) {
    ULONGLONG now = GetTickCount64();
    unsigned long long sinceStart = now - s_netStartTick;
    int n = snprintf(out, outSize,
        "\"total\":%u,\"c\":{\"mouse\":%u,\"kbd\":%u,\"ctrl\":%u,\"other\":%u},"
        "\"client\":{\"ip\":\"%s\",\"port\":%u,\"sessions\":%u,\"seenMs\":%u,\"active\":%d},"
        "\"ring\":[",
        (unsigned)s_netTotal,
        (unsigned)s_netKindCnt[0], (unsigned)s_netKindCnt[1],
        (unsigned)s_netKindCnt[2], (unsigned)s_netKindCnt[3],
        s_clientIp, (unsigned)s_clientPort, (unsigned)s_clientSessions,
        s_clientLast ? (unsigned)(now - s_clientLast) : 0u,
        (s_clientLast && (now - s_clientLast) < NET_CLIENT_ACTIVE_MS) ? 1 : 0);
    if (n <= 0 || n >= outSize) return 0;
    (void)sinceStart;

    for (int i = 0; i < s_netRingCount; i++) {
        int idx = (s_netRingHead - 1 - i + NET_RING_MAX * 2) % NET_RING_MAX;
        NetCmdRec *r = &s_netRing[idx];
        if (n >= outSize - 160) break;
        n += snprintf(out + n, outSize - n,
                      "%s{\"i\":%u,\"ms\":%u,\"k\":%u,\"ok\":%u,\"n\":\"%s\",\"d\":\"%s\"}",
                      i ? "," : "", (unsigned)r->seq, (unsigned)r->ms,
                      (unsigned)r->kind, (unsigned)r->ok, r->name, r->detail);
        if (n >= outSize) return outSize - 1;
    }
    /* 载荷是**纯字段列表**（不含外层大括号）—— SendEvent 会包成
     * {"type":"kmnet_stat",<载荷>}。这里只补 ring 数组的 ']'，
     * 多打一个 '}' 会让整条事件变成非法 JSON，前端 JSON.parse 失败后
     * 整个事件被静默丢弃（表现就是"API 活动面板永远空白"）。 */
    n += snprintf(out + n, outSize - n, "]");
    return (n < outSize) ? n : (outSize - 1);
}

/* 记录一条命令；needPush=高频鼠标类=false（限速），低频重要命令=true（立即推）。 */
static void NetStat_Record(const char *name, uint8_t kind, bool ok,
                           const char *detail, bool needPush) {
    NetStat_EnsureCs();
    EnterCriticalSection(&s_csNet);
    s_netTotal++;
    if (kind < 4) s_netKindCnt[kind]++;

    NetCmdRec *r = &s_netRing[s_netRingHead];
    r->seq = ++s_netSeq;
    r->ms  = (uint32_t)(GetTickCount64() - s_netStartTick);
    r->kind = kind;
    r->ok   = ok ? 1 : 0;
    snprintf(r->name, sizeof(r->name), "%s", name ? name : "?");
    snprintf(r->detail, sizeof(r->detail), "%s", detail ? detail : "");
    s_netRingHead = (s_netRingHead + 1) % NET_RING_MAX;
    if (s_netRingCount < NET_RING_MAX) s_netRingCount++;

    ULONGLONG now = GetTickCount64();
    bool allow = needPush || (now - s_netLastPush) >= NET_PUSH_MIN_MS;
    char body[1536];
    int n = 0;
    if (allow) n = NetStat_BuildLocked(body, sizeof(body));
    if (allow) s_netLastPush = now;
    LeaveCriticalSection(&s_csNet);

    if (allow && n > 0 && g_pipeServer.HasClient())
        g_pipeServer.SendEvent("kmnet_stat", body);
}

/* 每个入包都调用：维护"客户端在线/最近见过"信息，换客户端时立即推一次。 */
static void NetStat_TouchClient(const char *ip, uint16_t port) {
    NetStat_EnsureCs();
    bool changed = false;
    EnterCriticalSection(&s_csNet);
    if (strncmp(s_clientIp, ip ? ip : "", sizeof(s_clientIp) - 1) != 0 || s_clientPort != port) {
        snprintf(s_clientIp, sizeof(s_clientIp), "%s", ip ? ip : "");
        s_clientPort = port;
        s_clientSince = GetTickCount64();
        s_clientSessions++;
        changed = true;
    }
    s_clientLast = GetTickCount64();
    char body[1536];
    int n = 0;
    if (changed) n = NetStat_BuildLocked(body, sizeof(body));
    LeaveCriticalSection(&s_csNet);

    if (changed && n > 0 && g_pipeServer.HasClient()) {
        g_pipeServer.SendEvent("kmnet_stat", body);
        s_netLastPush = GetTickCount64();
    }
}

int KmboxnetServer::GetStatJson(char *out, int outSize) const {
    if (!out || outSize <= 2) return 0;
    NetStat_EnsureCs();
    EnterCriticalSection(&s_csNet);
    int n = NetStat_BuildLocked(out, outSize);
    LeaveCriticalSection(&s_csNet);
    if (n <= 0) { out[0] = '\0'; return 0; }
    return n;
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

    /* UI 预览用：记录客户端在线信息（换客户端/首次连接会立即推一次） */
    NetStat_TouchClient(addrStr, (uint16_t)ntohs(fromAddr.sin_port));
    char det[40];

    switch (hdr.cmd) {
    case CMD_CONNECT:
        DebugLogger::Log("UDP CMD_CONNECT -> ACK");
        NetStat_Record("connect", 2, true, "ACK", true);
        SendResponse(fromAddr, hdr);
        break;

    case CMD_MOUSE_MOVE: {
        if (len < (int)(sizeof(kmboxnet_header_t) + sizeof(soft_mouse_t))) break;
        soft_mouse_t mouse;
        memcpy(&mouse, data + sizeof(kmboxnet_header_t), sizeof(mouse));
        DebugLogger::Log("UDP CMD_MOUSE_MOVE x=%d y=%d", mouse.x, mouse.y);
        bool ok = CommandBridge::SendMouseMove(mouse.x, mouse.y, 1, (DeviceRole)DEV_ROLE_MOUSE);
        snprintf(det, sizeof(det), "x=%d y=%d", mouse.x, mouse.y);
        NetStat_Record("move", 0, ok, det, false);
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
        bool ok = CommandBridge::SendMouseMove(mouse.x, mouse.y, steps, (DeviceRole)DEV_ROLE_MOUSE);
        snprintf(det, sizeof(det), "x=%d y=%d c=%d", mouse.x, mouse.y, steps);
        NetStat_Record("automove", 0, ok, det, false);
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
        bool ok = CommandBridge::SendMoveTow(mouse.x, mouse.y, steps, (DeviceRole)DEV_ROLE_MOUSE);
        snprintf(det, sizeof(det), "x=%d y=%d c=%d", mouse.x, mouse.y, steps);
        NetStat_Record("beizer", 0, ok, det, false);
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
        bool ok = NetApplyButtonMask((uint8_t)mouse.button);
        snprintf(det, sizeof(det), "%s b=0x%02X", btnName, (unsigned)(mouse.button & 0x1F));
        NetStat_Record("button", 0, ok, det, false);
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
        bool ok = NetApplyButtonMask((uint8_t)mouse.button);
        if (mouse.x || mouse.y)
            ok = CommandBridge::SendMouseMove(mouse.x, mouse.y, 1, (DeviceRole)DEV_ROLE_MOUSE) && ok;
        if (mouse.wheel)
            ok = CommandBridge::SendMouseWheel(mouse.wheel, (DeviceRole)DEV_ROLE_MOUSE) && ok;
        snprintf(det, sizeof(det), "wh=%d b=0x%02X", mouse.wheel, (unsigned)(mouse.button & 0x1F));
        NetStat_Record((mouse.x || mouse.y) ? "all" : "wheel", 0, ok, det, false);
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
        bool ok = CommandBridge::SendMouseMask(m, (DeviceRole)DEV_ROLE_MOUSE);
        snprintf(det, sizeof(det), "m=0x%02X", (unsigned)m);
        NetStat_Record((hdr.cmd == CMD_UNMASK_ALL) ? "unmask_all" : "mask_mouse", 2, ok, det, true);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_REBOOT: {
        /* 双设备下客户端把它当成"那台盒子"，两台都发（单设备时第二次是同一个口，
         * 失败也无害）。 */
        DebugLogger::Log("UDP CMD_REBOOT -> mouse+kbd");
        bool ok1 = CommandBridge::SendReboot((DeviceRole)DEV_ROLE_MOUSE);
        bool ok2 = CommandBridge::SendReboot((DeviceRole)DEV_ROLE_KBD);
        NetStat_Record("reboot", 2, ok1 || ok2, "mouse+kbd", true);
        SendResponse(fromAddr, hdr);
        break;
    }

    case CMD_KEYBOARD_ALL: {
        if (len < (int)(sizeof(kmboxnet_header_t) + sizeof(soft_keyboard_t))) break;
        soft_keyboard_t kb;
        memcpy(&kb, data + sizeof(kmboxnet_header_t), sizeof(kb));
        int nkeys = 0;
        for (int i = 0; i < 10; i++) if (kb.button[i]) nkeys++;
        DebugLogger::Log("UDP CMD_KEYBOARD_ALL ctrl=0x%02X", kb.ctrl);
        bool ok = CommandBridge::SendKeyboard(kb.ctrl, kb.button);
        snprintf(det, sizeof(det), "ctrl=0x%02X n=%d", (unsigned)kb.ctrl, nkeys);
        NetStat_Record("keyboard", 1, ok, det, false);
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
        snprintf(det, sizeof(det), "port=%u", (unsigned)clientPort);
        NetStat_Record(clientPort ? "monitor_on" : "monitor_off", 2, true, det, true);
        SendResponse(fromAddr, hdr);
        break;
    }

    default:
        /* 未实现的命令仍然 ACK（官方库不校验），但要在预览里点名 ——
         * enc_* / debug / setconfig / lcd 这些目前一律落到这里，
         * 靠这条记录才能一眼看出"客户端调了个我们没实现的命令"。 */
        snprintf(det, sizeof(det), "cmd=0x%08X", hdr.cmd);
        NetStat_Record("unsupported", 3, false, det, true);
        DebugLogger::Log("UDP unsupported cmd=0x%08X (ACK only)", hdr.cmd);
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
