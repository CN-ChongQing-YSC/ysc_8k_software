#ifndef MONITOR_PUSH_H
#define MONITOR_PUSH_H

#include <cstdint>

class KmboxnetServer;

struct MonitorState {
    int buttons = 0;
    int x = 0;
    int y = 0;
    int wheel = 0;
};

class MonitorPush {
public:
    static void Init(KmboxnetServer *server);

    // Called from serial read thread when framed data arrives
    static void OnSerialResponse(const uint8_t *data, int len, void *userData);

    // Returns the latest monitor state (thread-safe read)
    static MonitorState GetLatest();
};

#endif // MONITOR_PUSH_H
