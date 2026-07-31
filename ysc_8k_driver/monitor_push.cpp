#include "monitor_push.h"
#include "kmboxnet_server.h"
#include "kmboxnet_proto.h"
#include "main.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>

static KmboxnetServer *s_server = nullptr;
static volatile long s_buttons = 0;
static volatile long s_x = 0;
static volatile long s_y = 0;
static volatile long s_wheel = 0;

void MonitorPush::Init(KmboxnetServer *server) {
    s_server = server;
}

MonitorState MonitorPush::GetLatest() {
    return MonitorState{ (int)s_buttons, (int)s_x, (int)s_y, (int)s_wheel };
}

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

void MonitorPush::OnSerialResponse(const uint8_t *data, int len, void *userData) {
    (void)userData;
    if (!s_server) return;

    char json[1024];
    if (len >= (int)sizeof(json)) return;
    memcpy(json, data, len);
    json[len] = '\0';

    int code = 0;
    if (!ExtractJsonInt(json, "code", code) || code != 200) return;

    char innerJson[512];
    if (!ExtractDataString(json, innerJson, sizeof(innerJson))) return;

    int b = 0, x = 0, y = 0;
    if (!ExtractJsonInt(innerJson, "b", b)) return;
    if (!ExtractJsonInt(innerJson, "x", x)) return;
    if (!ExtractJsonInt(innerJson, "y", y)) return;

    s_buttons = b;
    s_x = x;
    s_y = y;
    s_wheel = 0;

    // Push to kmboxnet client only if connected and state changed
    if (!s_server->HasMonitorClient()) return;

    static int last_b = -1;
    if (b == last_b) return;
    last_b = b;

    monitor_packet_t pkt = {};
    pkt.mouse.report_id = 0x01;
    pkt.mouse.buttons   = (uint8_t)b;
    pkt.mouse.x         = (int16_t)x;
    pkt.mouse.y         = (int16_t)y;
    pkt.mouse.wheel     = 0;

    pkt.kb.report_id = 0x02;

    s_server->SendMonitorData(pkt);
}
