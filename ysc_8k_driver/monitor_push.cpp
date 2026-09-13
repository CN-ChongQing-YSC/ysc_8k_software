#include "monitor_push.h"
#include "kmboxnet_server.h"
#include "kmboxnet_proto.h"
#include "main.h"
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

void MonitorPush::Init(KmboxnetServer *server) {
    s_server = server;
    if (!s_csInit) {
        InitializeCriticalSection(&s_cs);
        s_csInit = true;
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
        int b = 0, x = 0, y = 0;
        if (!ExtractJsonInt(innerJson, "b", b)) return;
        if (!ExtractJsonInt(innerJson, "x", x)) return;
        if (!ExtractJsonInt(innerJson, "y", y)) return;

        EnterCriticalSection(&s_cs);
        s_mouse.buttons = b;
        s_mouse.x = x;
        s_mouse.y = y;
        s_mouse.wheel = 0;
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
    pkt.mouse.wheel     = 0;

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
    if (!s_server->HasMonitorClient()) return;

    monitor_packet_t pkt = BuildMonitorPacket();
    s_server->SendMonitorData(pkt);
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
