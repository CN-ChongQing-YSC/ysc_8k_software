#include "pipe_server.h"
#include <cstdio>
#include <cstring>
#include <string>

PipeServer::PipeServer()
    : m_hThread(NULL)
    , m_hStopEvent(NULL)
    , m_running(false)
    , m_hasClient(false)
    , m_hPipe(INVALID_HANDLE_VALUE)
    , m_callback(nullptr)
    , m_callbackUserData(nullptr) {
    InitializeCriticalSection(&m_csWrite);
}

PipeServer::~PipeServer() {
    Stop();
    DeleteCriticalSection(&m_csWrite);
}

void PipeServer::Start() {
    if (m_running) return;
    m_running = true;
    m_hStopEvent = CreateEvent(NULL, TRUE, FALSE, NULL);
    m_hThread = CreateThread(NULL, 0, ServerThreadProc, this, 0, NULL);
}

void PipeServer::Stop() {
    m_running = false;
    if (m_hStopEvent) {
        SetEvent(m_hStopEvent);
    }
    if (m_hPipe != INVALID_HANDLE_VALUE) {
        CancelIo(m_hPipe);
    }
    if (m_hThread) {
        WaitForSingleObject(m_hThread, 5000);
        CloseHandle(m_hThread);
        m_hThread = NULL;
    }
    if (m_hStopEvent) {
        CloseHandle(m_hStopEvent);
        m_hStopEvent = NULL;
    }
}

bool PipeServer::HasClient() const {
    return m_hasClient;
}

void PipeServer::SetCommandCallback(CommandCallback cb, void *userData) {
    m_callback = cb;
    m_callbackUserData = userData;
}

void PipeServer::SendEvent(const char *type, const char *jsonBody) {
    if (!m_hasClient) return;

    char msg[PIPE_BUF_SIZE];
    int len;
    if (jsonBody && jsonBody[0])
        len = snprintf(msg, sizeof(msg), "{\"type\":\"%s\",%s}\n", type, jsonBody);
    else
        len = snprintf(msg, sizeof(msg), "{\"type\":\"%s\"}\n", type);

    if (len <= 0 || len >= (int)sizeof(msg)) return;

    EnterCriticalSection(&m_csWrite);
    if (m_hPipe != INVALID_HANDLE_VALUE) {
        DWORD written = 0;
        OVERLAPPED ol = {};
        ol.hEvent = CreateEvent(NULL, TRUE, FALSE, NULL);
        BOOL ok = WriteFile(m_hPipe, msg, (DWORD)len, &written, &ol);
        if (!ok && GetLastError() == ERROR_IO_PENDING) {
            WaitForSingleObject(ol.hEvent, 1000);
            GetOverlappedResult(m_hPipe, &ol, &written, TRUE);
        }
        CloseHandle(ol.hEvent);
    }
    LeaveCriticalSection(&m_csWrite);
}

DWORD WINAPI PipeServer::ServerThreadProc(LPVOID param) {
    static_cast<PipeServer*>(param)->ServerLoop();
    return 0;
}

void PipeServer::ServerLoop() {
    while (m_running) {
        // Create pipe instance
        HANDLE hPipe = CreateNamedPipeW(PIPE_NAME,
            PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1, PIPE_BUF_SIZE, PIPE_BUF_SIZE, 0, NULL);

        if (hPipe == INVALID_HANDLE_VALUE) {
            Sleep(1000);
            continue;
        }

        // Wait for client connection
        OVERLAPPED olConnect = {};
        olConnect.hEvent = CreateEvent(NULL, TRUE, FALSE, NULL);
        BOOL connected = ConnectNamedPipe(hPipe, &olConnect);
        DWORD err = GetLastError();

        if (!connected && err == ERROR_PIPE_CONNECTED) {
            // Client already connected before we called ConnectNamedPipe
            SetEvent(olConnect.hEvent);
        }

        HANDLE waitHandles[] = { olConnect.hEvent, m_hStopEvent };
        DWORD wait = WaitForMultipleObjects(2, waitHandles, FALSE, INFINITE);
        CloseHandle(olConnect.hEvent);

        if (wait == WAIT_OBJECT_0 + 1) {
            // Stop signal
            CloseHandle(hPipe);
            break;
        }

        // Client connected
        m_hPipe = hPipe;
        m_hasClient = true;

        // Read loop
        char buf[PIPE_BUF_SIZE];
        std::string lineBuf;

        while (m_running) {
            OVERLAPPED olRead = {};
            olRead.hEvent = CreateEvent(NULL, TRUE, FALSE, NULL);

            DWORD bytesRead = 0;
            BOOL ok = ReadFile(hPipe, buf, sizeof(buf) - 1, &bytesRead, &olRead);

            if (!ok) {
                DWORD readErr = GetLastError();
                if (readErr == ERROR_IO_PENDING) {
                    HANDLE readHandles[] = { olRead.hEvent, m_hStopEvent };
                    DWORD readWait = WaitForMultipleObjects(2, readHandles, FALSE, INFINITE);
                    if (readWait == WAIT_OBJECT_0 + 1) {
                        CancelIo(hPipe);
                        CloseHandle(olRead.hEvent);
                        break;
                    }
                    if (!GetOverlappedResult(hPipe, &olRead, &bytesRead, TRUE)) {
                        CloseHandle(olRead.hEvent);
                        break;
                    }
                } else {
                    CloseHandle(olRead.hEvent);
                    break;
                }
            }

            CloseHandle(olRead.hEvent);

            if (bytesRead == 0) break;

            buf[bytesRead] = '\0';
            lineBuf += buf;

            // Process complete lines
            size_t pos;
            while ((pos = lineBuf.find('\n')) != std::string::npos) {
                std::string line = lineBuf.substr(0, pos);
                lineBuf.erase(0, pos + 1);
                if (!line.empty() && m_callback) {
                    m_callback(line.c_str(), m_callbackUserData);
                }
            }
        }

        // Client disconnected
        m_hasClient = false;
        m_hPipe = INVALID_HANDLE_VALUE;
        FlushFileBuffers(hPipe);
        DisconnectNamedPipe(hPipe);
        CloseHandle(hPipe);
    }
}
