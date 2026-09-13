#include "monitor_push.h"
#include "kmboxnet_server.h"
#include "kmboxnet_proto.h"
#include "main.h"
#include "pipe_server.h"   // g_pipeServer.SendEvent（UI 推送）
#include <cstdio>
#include <cstdlib>
#include <cstring>

static KmboxnetServer *s_server = nullptr;

// 双设备监控状态：鼠标机填 s_mouse，键盘机填 s_kbd。
// CRITICAL_SECTION 保护 —— 两路串口读线程会并发进入 OnSerialResponse。
static CRITICAL_SECTION s_cs;
static bool             s_csInit = false;

static MonitorState    s_mouse;
static KbdMonitorState s_kbd;

// 上一次推送给 kmboxnet 客户端的内容，用于变化检测（避免无变化时刷包）
static bool             s_haveLast = false;
static MonitorState     s_lastMouse;
static KbdMonitorState  s_lastKbd;

/* ===== UI（Electron）推送 =====
 * 背景：前端原先每 100ms 发一次 get_monitor 拉状态，高回报率下看着有延时。
 * 改成"状态一变就推"，但**不能**把 8K 鼠标的每一帧都推给前端（8000 次/秒的
 * 管道写 + JSON 解析 + Vue 更新会真的卡）。所以在驱动里做一次合并：
 *   - PushIfChanged() 只负责把 UiPushThread 叫醒（SetEvent，开销极小）；
 *   - UiPushThread 取的是"当前最新快照"，并按 UI_PUSH_MIN_INTERVAL_MS 限速；
 *   - 被限速挡下的变化不会丢 —— 事件会在睡梦期间再次被置位，醒来后推最新的。
 * 于是：按键/键盘变化最坏晚 UI_PUSH_MIN_INTERVAL_MS 到达前端，位移按 ~100Hz
 * 刷新，空闲时线程阻塞在事件上、零 CPU（不轮询、不定时器）。 */
#define UI_PUSH_MIN_INTERVAL_MS 8u    /* ≈125Hz 上限；进程已 timeBeginPeriod(1) */
static HANDLE       s_hUiWake   = NULL;   /* 自动复位事件：有新状态待推送 */
static HANDLE       s_hUiThread = NULL;
static volatile LONG s_uiThreadStop = 0;
static ULONGLONG    s_lastUiPushMs = 0;   /* 仅 UiPushThread 访问 */

static DWORD WINAPI UiPushThreadProc(LPVOID) {
    while (!s_uiThreadStop) {
        DWORD w = WaitForSingleObject(s_hUiWake, 200);
        if (s_uiThreadStop) break;
        if (w != WAIT_OBJECT_0) continue;

        ULONGLONG dt = GetTickCount64() - s_lastUiPushMs;
        if (dt < UI_PUSH_MIN_INTERVAL_MS)
            Sleep((DWORD)(UI_PUSH_MIN_INTERVAL_MS - dt));
        if (s_uiThreadStop) break;

        s_lastUiPushMs = GetTickCount64();
        if (g_pipeServer.HasClient()) {
            char body[512];
            MonitorPush::SerializeUiJson(body, sizeof(body));
            g_pipeServer.SendEvent("monitor_data", body);
        }
    }
    return 0;
}

void MonitorPush::Shutdown() {
    if (s_hUiThread) {
        InterlockedExchange(&s_uiThreadStop, 1);
        if (s_hUiWake) SetEvent(s_hUiWake);
        WaitForSingleObject(s_hUiThread, 2000);
        CloseHandle(s_hUiThread);
        s_hUiThread = NULL;
    }
    if (s_hUiWake) {
        CloseHandle(s_hUiWake);
        s_hUiWake = NULL;
    }
}

void MonitorPush::Init(KmboxnetServer *server) {
    s_server = server;
    if (!s_csInit) {
        InitializeCriticalSection(&s_cs);
        s_csInit = true;
    }
    if (!s_hUiWake) {
        s_hUiWake = CreateEvent(NULL, FALSE, FALSE, NULL);   /* 自动复位 */
    }
    if (!s_hUiThread && s_hUiWake) {
        s_uiThreadStop = 0;
        s_hUiThread = CreateThread(NULL, 0, UiPushThreadProc, NULL, 0, NULL);
    }
}

// ---------- 极简 JSON 取字段（沿用原实现的字符串扫描风格，不引第三方库） ----------

static bool ExtractJsonInt(const char *json, const char *key, int &out) {
    char search[64];
    snprintf(search, sizeof(search), "\"%s\":", key);
    const char *p = strstr(json, search);
    if (!p) return false;
    p += strlen(search);
    while (*p == ' ') p++;
    if (*p == '-' || (*p >= '0' && *p <= '9')) {
        out = atoi(p);
        return true;
    }
    return false;
}

static bool ExtractDataString(const char *json, char *out, int outSize) {
    const char *key = "\"data\":\"";
    const char *p = strstr(json, key);
    if (!p) return false;
    p += strlen(key);

    int i = 0;
    while (*p && *p != '"' && i < outSize - 1) {
        if (*p == '\\' && *(p + 1)) {
            p++;
        }
        out[i++] = *p++;
    }
    out[i] = '\0';
    return i > 0;
}

// 取 "message":"<value>" 的值（不分配内存）
static bool ExtractMessage(const char *json, char *out, int outSize) {
    const char *key = "\"message\":\"";
    const char *p = strstr(json, key);
    if (!p) return false;
    p += strlen(key);
    int i = 0;
    while (*p && *p != '"' && i < outSize - 1) out[i++] = *p++;
    out[i] = '\0';
    return i > 0;
}

// 解析 "k":[a,b,c] 数组（可能为空）
static int ParseKeyArray(const char *json, uint8_t *out, int maxOut) {
    const char *p = strstr(json, "\"k\":[");
    if (!p) return 0;
    p += 5;
    int n = 0;
    while (*p && *p != ']') {
        while (*p == ' ' || *p == ',') p++;
        if (*p == ']' || !*p) break;
        if (*p >= '0' && *p <= '9') {
            int v = atoi(p);
            if (n < maxOut) out[n++] = (uint8_t)v;
            while (*p >= '0' && *p <= '9') p++;
        } else {
            p++;
        }
    }
    return n;
}

// ---------- 状态比较 ----------

static bool MouseEq(const MonitorState &a, const MonitorState &b) {
    return a.buttons == b.buttons && a.x == b.x && a.y == b.y && a.valid == b.valid;
}

static bool KbdEq(const KbdMonitorState &a, const KbdMonitorState &b) {
    if (a.mod != b.mod || a.key_count != b.key_count ||
        a.overflow != b.overflow || a.valid != b.valid)
        return false;
    return memcmp(a.key, b.key, sizeof(a.key)) == 0;
}

// ---------- 入口 ----------

void MonitorPush::OnSerialResponse(const uint8_t *data, int len, void *userData) {
    (void)userData;
    if (!s_server) return;

    char json[1024];
    if (len >= (int)sizeof(json)) return;
    memcpy(json, data, len);
    json[len] = '\0';

    int code = 0;
    if (!ExtractJsonInt(json, "code", code) || code != 200) return;

    char msg[24] = {};
    ExtractMessage(json, msg, sizeof(msg));

    char innerJson[512];
    if (!ExtractDataString(json, innerJson, sizeof(innerJson))) return;

    if (strcmp(msg, "kbd") == 0) {
        // ---- 键盘物理状态（来自键盘设备） ----
        KbdMonitorState st;
        int mod = 0, n = 0, ov = 0;
        if (!ExtractJsonInt(innerJson, "mod", mod)) return;
        ExtractJsonInt(innerJson, "n", n);
        ExtractJsonInt(innerJson, "ov", ov);
        st.mod = mod & 0xFF;
        st.overflow = ov;
        int got = ParseKeyArray(innerJson, st.key, KBD_MON_KEYS_MAX);
        if (n > 0 && n < got) got = n;          // 以服务端的 n 为准做上限
        st.key_count = got;
        st.valid = 1;

        EnterCriticalSection(&s_cs);
        s_kbd = st;
        LeaveCriticalSection(&s_cs);
    } else {
        // ---- 鼠标状态（来自鼠标设备；旧固件不带 message 字段，走这里） ----
        int b = 0, x = 0, y = 0, wh = 0;
        if (!ExtractJsonInt(innerJson, "b", b)) return;
        if (!ExtractJsonInt(innerJson, "x", x)) return;
        if (!ExtractJsonInt(innerJson, "y", y)) return;
        /* "wh" 是物理滚轮增量，新固件才带；老固件缺这个字段时按 0 处理
         * （否则 monitor_mouse_wheel 会一直读到脏值）。 */
        ExtractJsonInt(innerJson, "wh", wh);

        EnterCriticalSection(&s_cs);
        s_mouse.buttons = b;
        s_mouse.x = x;
        s_mouse.y = y;
        s_mouse.wheel = wh;
        s_mouse.valid = 1;
        LeaveCriticalSection(&s_cs);
    }

    PushIfChanged();
}

// ---------- 读取 ----------

MonitorState MonitorPush::GetLatest() {
    MonitorState m;
    if (s_csInit) EnterCriticalSection(&s_cs);
    m = s_mouse;
    if (s_csInit) LeaveCriticalSection(&s_cs);
    return m;
}

KbdMonitorState MonitorPush::GetKeyboard() {
    KbdMonitorState k;
    if (s_csInit) EnterCriticalSection(&s_cs);
    k = s_kbd;
    if (s_csInit) LeaveCriticalSection(&s_cs);
    return k;
}

monitor_packet_t MonitorPush::BuildMonitorPacket() {
    MonitorState    m;
    KbdMonitorState k;
    if (s_csInit) EnterCriticalSection(&s_cs);
    m = s_mouse;
    k = s_kbd;
    if (s_csInit) LeaveCriticalSection(&s_cs);

    monitor_packet_t pkt = {};
    pkt.mouse.report_id = 0x01;
    pkt.mouse.buttons   = (uint8_t)m.buttons;
    pkt.mouse.x         = (int16_t)m.x;
    pkt.mouse.y         = (int16_t)m.y;
    pkt.mouse.wheel     = (int16_t)m.wheel;

    // 键盘字段：硬件期望 12 字节 keyboard_report_t（mod + 10 键）。
    // 固件侧最多给 6 个非修饰键码，其余位置留 0。
    pkt.kb.report_id = 0x02;
    pkt.kb.buttons   = (uint8_t)k.mod;
    if (k.valid) {
        int n = k.key_count;
        if (n > (int)sizeof(pkt.kb.data)) n = (int)sizeof(pkt.kb.data);
        for (int i = 0; i < n; i++) pkt.kb.data[i] = k.key[i];
    }
    return pkt;
}

void MonitorPush::PushIfChanged() {
    if (!s_server) return;

    MonitorState    m;
    KbdMonitorState k;
    if (s_csInit) EnterCriticalSection(&s_cs);
    m = s_mouse;
    k = s_kbd;
    bool changed = !s_haveLast || !MouseEq(m, s_lastMouse) || !KbdEq(k, s_lastKbd);
    if (changed) {
        s_lastMouse = m;
        s_lastKbd   = k;
        s_haveLast  = true;
    }
    if (s_csInit) LeaveCriticalSection(&s_cs);

    if (!changed) return;

    /* kmboxnet UDP 监控客户端：沿用旧行为（无客户端时空转） */
    if (s_server && s_server->HasMonitorClient()) {
        monitor_packet_t pkt = BuildMonitorPacket();
        s_server->SendMonitorData(pkt);
    }

    /* Electron 前端：变化即推，替代前端每 100ms 轮询 get_monitor。
     * 这里只叫醒推送线程（SetEvent ≈ 一次原子操作），真正的限速/合并/JSON
     * 组装都在 UiPushThread 里做，绝不在串口读线程上做重活。 */
    if (s_hUiWake) SetEvent(s_hUiWake);
}

void MonitorPush::ResetSide(int isKeyboard) {
    if (s_csInit) EnterCriticalSection(&s_cs);
    if (isKeyboard) {
        s_kbd = KbdMonitorState();
    } else {
        s_mouse = MonitorState();
    }
    s_haveLast = false;   // 断开后允许下一次推送，避免被变化检测吞掉
    if (s_csInit) LeaveCriticalSection(&s_cs);
}

// ---------- 管道事件用的 JSON 片段 ----------

void MonitorPush::SerializeMouseJson(char *out, int outSize) {
    MonitorState m = GetLatest();
    snprintf(out, outSize,
             "\"mouse\":{\"valid\":%d,\"buttons\":%d,\"x\":%d,\"y\":%d}",
             m.valid, m.buttons, m.x, m.y);
}

void MonitorPush::SerializeKbdJson(char *out, int outSize) {
    KbdMonitorState k = GetKeyboard();

    int n = snprintf(out, outSize,
                     "\"kbd\":{\"valid\":%d,\"mod\":%d,\"count\":%d,\"overflow\":%d,\"keys\":[",
                     k.valid, k.mod, k.key_count, k.overflow);
    for (int i = 0; i < k.key_count && n > 0 && n < outSize; i++)
        n += snprintf(out + n, outSize - n, "%s%u", i ? "," : "", (unsigned)k.key[i]);
    if (n > 0 && n < outSize)
        snprintf(out + n, outSize - n, "]}");
}

/* monitor_data 的载荷：与 get_monitor 的应答逐字节同构
 * （buttons/x/y/wheel + kbd 对象 + mouseValid），这样前端的同一个监听器
 * 既能吃轮询应答、也能吃推送。 */
void MonitorPush::SerializeUiJson(char *out, int outSize) {
    MonitorState m = GetLatest();
    char kbdPart[256];
    SerializeKbdJson(kbdPart, sizeof(kbdPart));
    snprintf(out, outSize,
             "\"buttons\":%d,\"x\":%d,\"y\":%d,\"wheel\":%d,%s,\"mouseValid\":%d",
             m.buttons, m.x, m.y, m.wheel, kbdPart, m.valid);
}
