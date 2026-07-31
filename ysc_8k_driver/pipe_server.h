#ifndef PIPE_SERVER_H
#define PIPE_SERVER_H

#include <windows.h>

#define PIPE_NAME L"\\\\.\\pipe\\ysc_8k_driver"
#define PIPE_BUF_SIZE 4096

class PipeServer {
public:
    PipeServer();
    ~PipeServer();

    void Start();
    void Stop();

    // Send JSON event to connected Electron client (thread-safe).
    // jsonBody can be nullptr or empty for type-only events.
    void SendEvent(const char *type, const char *jsonBody);

    bool HasClient() const;

    typedef void (*CommandCallback)(const char *json, void *userData);
    void SetCommandCallback(CommandCallback cb, void *userData);

private:
    static DWORD WINAPI ServerThreadProc(LPVOID param);
    void ServerLoop();

    HANDLE          m_hThread;
    HANDLE          m_hStopEvent;
    volatile bool   m_running;
    volatile bool   m_hasClient;

    CRITICAL_SECTION m_csWrite;
    HANDLE          m_hPipe;

    CommandCallback m_callback;
    void           *m_callbackUserData;
};

#endif // PIPE_SERVER_H
