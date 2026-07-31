#include "serial_port.h"
#include "main.h"
#include "debug_logger.h"
#include <cstring>
#include <cwchar>
#include <cstdio>

// Helper: build a version query frame
static int BuildVersionFrame(uint8_t *buf) {
    const char *json = "{\"cmd\":132}";
    int jsonLen = (int)strlen(json);
    uint16_t packetLen = (uint16_t)(14 + jsonLen);
    int pos = 0;
    memcpy(buf + pos, "<START>", 7); pos += 7;
    buf[pos++] = (uint8_t)(packetLen >> 8);
    buf[pos++] = (uint8_t)(packetLen & 0xFF);
    memcpy(buf + pos, json, jsonLen); pos += jsonLen;
    memcpy(buf + pos, "<END>", 5); pos += 5;
    return pos;
}

// Helper: check if a raw buffer contains a valid frame with "code":200
static bool HasValidResponse(const uint8_t *rx, int rxLen) {
    for (int i = 0; i <= rxLen - 14; i++) {
        if (memcmp(rx + i, "<START>", 7) != 0) continue;
        if (i + 9 > rxLen) continue;
        uint16_t totalLen = ((uint16_t)rx[i + 7] << 8) | rx[i + 8];
        int endPos = i + totalLen;
        if (endPos > rxLen || endPos < 5) continue;
        if (memcmp(rx + endPos - 5, "<END>", 5) != 0) continue;
        int payloadStart = i + 9;
        int payloadLen = totalLen - 14;
        if (payloadLen <= 0 || payloadStart + payloadLen > rxLen) continue;
        // Null-terminate and check for success code
        char tmp[256];
        if (payloadLen >= (int)sizeof(tmp)) continue;
        memcpy(tmp, rx + payloadStart, payloadLen);
        tmp[payloadLen] = '\0';
        if (strstr(tmp, "\"code\":200") || strstr(tmp, "\"code\": 200"))
            return true;
    }
    return false;
}

SerialPort::SerialPort()
    : m_hPort(NULL)
    , m_baudRate(0)
    , m_portName{}
    , m_hReadThread(NULL)
    , m_hStopEvent(NULL)
    , m_rxState(RX_IDLE)
    , m_rxLen(0)
    , m_expectedLen(0)
    , m_callback(nullptr)
    , m_callbackUserData(nullptr)
    , m_queueHead(0)
    , m_queueTail(0)
    , m_queueCount(0)
    , m_hWriteThread(NULL)
    , m_writeRunning(false)
    , m_hReadOL(NULL)
    , m_hWriteOL(NULL) {
    InitializeCriticalSection(&m_csWrite);
    InitializeCriticalSection(&m_csQueue);
    InitializeConditionVariable(&m_cvNotEmpty);
}

SerialPort::~SerialPort() {
    Disconnect();
    DeleteCriticalSection(&m_csQueue);
    DeleteCriticalSection(&m_csWrite);
}

bool SerialPort::Connect(const wchar_t *portName, uint32_t baudRate) {
    if (m_hPort) Disconnect();

    wchar_t fullPath[64];
    swprintf_s(fullPath, L"\\\\.\\%s", portName);

    m_hPort = CreateFileW(fullPath,
        GENERIC_READ | GENERIC_WRITE, 0, NULL,
        OPEN_EXISTING, FILE_FLAG_OVERLAPPED, NULL);
    if (m_hPort == INVALID_HANDLE_VALUE) {
        m_hPort = NULL;
        return false;
    }

    DCB dcb = {};
    dcb.DCBlength = sizeof(dcb);
    if (!GetCommState(m_hPort, &dcb)) { Disconnect(); return false; }
    dcb.BaudRate = baudRate;
    dcb.ByteSize = 8;
    dcb.Parity   = NOPARITY;
    dcb.StopBits = ONESTOPBIT;
    dcb.fDtrControl = DTR_CONTROL_ENABLE;
    dcb.fRtsControl = RTS_CONTROL_ENABLE;
    if (!SetCommState(m_hPort, &dcb)) { Disconnect(); return false; }

    COMMTIMEOUTS timeouts = {};
    timeouts.ReadIntervalTimeout        = 1;
    timeouts.ReadTotalTimeoutConstant   = 10;
    timeouts.WriteTotalTimeoutConstant  = 100;
    SetCommTimeouts(m_hPort, &timeouts);

    m_baudRate = baudRate;
    m_rxState  = RX_IDLE;
    m_rxLen    = 0;
    wcscpy_s(m_portName, portName);

    m_hReadOL = CreateEvent(NULL, TRUE, FALSE, NULL);
    m_hWriteOL = CreateEvent(NULL, TRUE, FALSE, NULL);
    return true;
}

void SerialPort::Disconnect() {
    StopReadThread();
    if (m_hPort) {
        CloseHandle(m_hPort);
        m_hPort = NULL;
    }
    if (m_hReadOL) { CloseHandle(m_hReadOL); m_hReadOL = NULL; }
    if (m_hWriteOL) { CloseHandle(m_hWriteOL); m_hWriteOL = NULL; }
    m_baudRate = 0;
}

bool SerialPort::IsConnected() const { return m_hPort != NULL; }
uint32_t SerialPort::GetBaudRate() const { return m_baudRate; }
const wchar_t* SerialPort::GetPortName() const { return m_portName; }

uint32_t SerialPort::DetectBaudrate(const wchar_t *portName) {
    uint8_t frame[64];
    int frameLen = BuildVersionFrame(frame);

    wchar_t fullPath[64];
    swprintf_s(fullPath, L"\\\\.\\%s", portName);

    // Build converging-inward probe order: 4M → 115200 → 3M → 230400 → 2M → 460800 → ...
    int probeOrder[SUPPORTED_BAUD_COUNT];
    int left = 0;
    int right = SUPPORTED_BAUD_COUNT - 1;
    int pos = 0;
    bool highNext = true;
    while (left <= right) {
        probeOrder[pos++] = highNext ? right-- : left++;
        highNext = !highNext;
    }

    for (int oi = 0; oi < SUPPORTED_BAUD_COUNT; oi++) {
        uint32_t baud = SUPPORTED_BAUDS[probeOrder[oi]];

        HANDLE hPort = CreateFileW(fullPath,
            GENERIC_READ | GENERIC_WRITE, 0, NULL,
            OPEN_EXISTING, 0, NULL);
        if (hPort == INVALID_HANDLE_VALUE) continue;

        DCB dcb = {};
        dcb.DCBlength = sizeof(dcb);
        GetCommState(hPort, &dcb);
        dcb.BaudRate = baud;
        dcb.ByteSize = 8;
        dcb.Parity   = NOPARITY;
        dcb.StopBits = ONESTOPBIT;
        dcb.fDtrControl = DTR_CONTROL_ENABLE;
        dcb.fRtsControl = RTS_CONTROL_ENABLE;
        if (!SetCommState(hPort, &dcb)) {
            CloseHandle(hPort);
            continue;
        }

        COMMTIMEOUTS timeouts = {};
        timeouts.ReadIntervalTimeout      = 1;
        timeouts.ReadTotalTimeoutConstant = 50;
        SetCommTimeouts(hPort, &timeouts);

        PurgeComm(hPort, PURGE_RXCLEAR | PURGE_TXCLEAR);
        Sleep(10);

        // Send version query
        DWORD written;
        WriteFile(hPort, frame, frameLen, &written, NULL);

        // Read response for ~200ms
        uint8_t rxBuf[256];
        int rxLen = 0;
        DWORD t0 = GetTickCount();
        while (GetTickCount() - t0 < 200 && rxLen < (int)sizeof(rxBuf)) {
            DWORD bytesRead = 0;
            if (!ReadFile(hPort, rxBuf + rxLen, sizeof(rxBuf) - rxLen,
                          &bytesRead, NULL))
                break;
            if (bytesRead > 0) {
                rxLen += bytesRead;
                if (HasValidResponse(rxBuf, rxLen)) {
                    CloseHandle(hPort);
                    return baud;
                }
            }
        }

        CloseHandle(hPort);
    }

    return 0;
}

bool SerialPort::SwitchBaudrate(uint32_t newBaud) {
    if (!m_hPort) return false;

    // Validate baudrate
    bool valid = false;
    for (int i = 0; i < SUPPORTED_BAUD_COUNT; i++) {
        if (SUPPORTED_BAUDS[i] == newBaud) { valid = true; break; }
    }
    if (!valid) return false;

    // Send set baudrate command
    char json[64];
    snprintf(json, sizeof(json), "{\"cmd\":133,\"baud\":%u}", newBaud);
    if (!SendJsonCommand(json)) return false;

    // Wait for device to process
    Sleep(200);

    // Stop read thread, close port
    StopReadThread();
    CloseHandle(m_hPort);
    m_hPort = NULL;

    // Reopen at new baudrate
    Sleep(100);
    if (!Connect(m_portName, newBaud)) return false;

    // Verify connection at new baudrate
    uint8_t frame[64];
    int frameLen = BuildVersionFrame(frame);

    EnterCriticalSection(&m_csWrite);
    {
        OVERLAPPED ov = {};
        ov.hEvent = m_hWriteOL;
        DWORD written = 0;
        BOOL ok = WriteFile(m_hPort, frame, frameLen, &written, &ov);
        if (!ok && GetLastError() == ERROR_IO_PENDING)
            GetOverlappedResult(m_hPort, &ov, &written, TRUE);
    }
    LeaveCriticalSection(&m_csWrite);

    // Temporarily increase read timeout for verification
    COMMTIMEOUTS timeouts = {};
    timeouts.ReadIntervalTimeout      = 1;
    timeouts.ReadTotalTimeoutConstant = 200;
    SetCommTimeouts(m_hPort, &timeouts);

    uint8_t rxBuf[256];
    int rxLen = 0;
    DWORD t0 = GetTickCount();
    while (GetTickCount() - t0 < 500 && rxLen < (int)sizeof(rxBuf)) {
        DWORD bytesRead = 0;
        OVERLAPPED ov = {};
        ov.hEvent = m_hReadOL;
        BOOL ok = ReadFile(m_hPort, rxBuf + rxLen, sizeof(rxBuf) - rxLen,
                           &bytesRead, &ov);
        if (!ok) {
            if (GetLastError() != ERROR_IO_PENDING) break;
            DWORD wait = WaitForSingleObject(m_hReadOL, 300);
            if (wait != WAIT_OBJECT_0) break;
            if (!GetOverlappedResult(m_hPort, &ov, &bytesRead, FALSE)) break;
        }
        if (bytesRead > 0) {
            rxLen += bytesRead;
            if (HasValidResponse(rxBuf, rxLen)) {
                // Restore normal timeouts
                timeouts.ReadTotalTimeoutConstant = 10;
                SetCommTimeouts(m_hPort, &timeouts);
                // Restart read thread
                m_rxState = RX_IDLE;
                m_rxLen = 0;
                return true;
            }
        }
    }

    // Verification failed
    Disconnect();
    return false;
}

bool SerialPort::SendRaw(const uint8_t *data, int len) {
    if (!m_hPort || !data || len <= 0) return false;
    return EnqueueFrame(data, len);
}

bool SerialPort::SendJsonCommand(const char *json) {
    if (!m_hPort || !json) return false;

    int jsonLen = (int)strlen(json);
    uint16_t packetLen = (uint16_t)(HEADER_LEN + jsonLen + END_MARKER_LEN);

    uint8_t buf[FRAME_SZ];
    int pos = 0;
    memcpy(buf + pos, START_MARKER, START_MARKER_LEN); pos += START_MARKER_LEN;
    buf[pos++] = (uint8_t)(packetLen >> 8);
    buf[pos++] = (uint8_t)(packetLen & 0xFF);
    memcpy(buf + pos, json, jsonLen);                   pos += jsonLen;
    memcpy(buf + pos, END_MARKER, END_MARKER_LEN);      pos += END_MARKER_LEN;

    return EnqueueFrame(buf, pos);
}

void SerialPort::SetResponseCallback(ResponseCallback cb, void *userData) {
    m_callback = cb;
    m_callbackUserData = userData;
}

bool SerialPort::StartReadThread(HANDLE hStopEvent) {
    if (!m_hPort || m_hReadThread) return false;
    m_hStopEvent = hStopEvent;
    m_rxState = RX_IDLE;
    m_rxLen   = 0;

    if (!StartWriteThread()) return false;

    m_hReadThread = CreateThread(NULL, 0, ReadThreadProc, this, 0, NULL);
    if (!m_hReadThread) {
        StopWriteThread();
        return false;
    }
    SetThreadPriority(m_hReadThread, THREAD_PRIORITY_ABOVE_NORMAL);
    return true;
}

void SerialPort::StopReadThread() {
    StopWriteThread();
    if (m_hReadThread) {
        WaitForSingleObject(m_hReadThread, 2000);
        CloseHandle(m_hReadThread);
        m_hReadThread = NULL;
    }
}

DWORD WINAPI SerialPort::WriteThreadProc(LPVOID param) {
    static_cast<SerialPort*>(param)->WriteLoop();
    return 0;
}

bool SerialPort::StartWriteThread() {
    if (m_hWriteThread) return false;

    EnterCriticalSection(&m_csQueue);
    m_queueHead = 0;
    m_queueTail = 0;
    m_queueCount = 0;
    LeaveCriticalSection(&m_csQueue);

    m_writeRunning = true;
    m_hWriteThread = CreateThread(NULL, 0, WriteThreadProc, this, 0, NULL);
    if (!m_hWriteThread) return false;
    SetThreadPriority(m_hWriteThread, THREAD_PRIORITY_ABOVE_NORMAL);
    return true;
}

void SerialPort::StopWriteThread() {
    if (!m_hWriteThread) return;
    m_writeRunning = false;
    WakeConditionVariable(&m_cvNotEmpty);
    WaitForSingleObject(m_hWriteThread, 2000);
    CloseHandle(m_hWriteThread);
    m_hWriteThread = NULL;
}

bool SerialPort::EnqueueFrame(const uint8_t *data, int len) {
    if (len > FRAME_SZ) return false;

    EnterCriticalSection(&m_csQueue);

    if (m_queueCount == QUEUE_CAP) {
        m_queueTail = (m_queueTail + 1) % QUEUE_CAP;
        m_queueCount--;
    }

    FrameSlot &slot = m_writeQueue[m_queueHead];
    memcpy(slot.data, data, len);
    slot.len = len;
    m_queueHead = (m_queueHead + 1) % QUEUE_CAP;
    m_queueCount++;

    WakeConditionVariable(&m_cvNotEmpty);
    LeaveCriticalSection(&m_csQueue);
    return true;
}

void SerialPort::WriteLoop() {
    uint8_t buf[FRAME_SZ];
    int len;

    while (m_writeRunning) {
        EnterCriticalSection(&m_csQueue);
        while (m_queueCount == 0 && m_writeRunning)
            SleepConditionVariableCS(&m_cvNotEmpty, &m_csQueue, 50);

        if (m_queueCount == 0) {
            LeaveCriticalSection(&m_csQueue);
            break;
        }

        FrameSlot &slot = m_writeQueue[m_queueTail];
        memcpy(buf, slot.data, slot.len);
        len = slot.len;
        m_queueTail = (m_queueTail + 1) % QUEUE_CAP;
        m_queueCount--;
        LeaveCriticalSection(&m_csQueue);

        if (m_hPort && len > 0) {
            OVERLAPPED ov = {};
            ov.hEvent = m_hWriteOL;
            DWORD written = 0;
            BOOL ok = WriteFile(m_hPort, buf, len, &written, &ov);
            if (!ok && GetLastError() == ERROR_IO_PENDING)
                GetOverlappedResult(m_hPort, &ov, &written, TRUE);
            DebugLogger::Log("SERIAL WRITE %d bytes (written=%d)", len, (int)written);
        }
    }

    // Drain remaining items
    while (m_hPort) {
        EnterCriticalSection(&m_csQueue);
        if (m_queueCount == 0) {
            LeaveCriticalSection(&m_csQueue);
            break;
        }
        FrameSlot &slot = m_writeQueue[m_queueTail];
        memcpy(buf, slot.data, slot.len);
        len = slot.len;
        m_queueTail = (m_queueTail + 1) % QUEUE_CAP;
        m_queueCount--;
        LeaveCriticalSection(&m_csQueue);

        OVERLAPPED ov = {};
        ov.hEvent = m_hWriteOL;
        DWORD written = 0;
        BOOL ok = WriteFile(m_hPort, buf, len, &written, &ov);
        if (!ok && GetLastError() == ERROR_IO_PENDING)
            GetOverlappedResult(m_hPort, &ov, &written, TRUE);
        DebugLogger::Log("SERIAL WRITE (drain) %d bytes", len);
    }
}

DWORD WINAPI SerialPort::ReadThreadProc(LPVOID param) {
    static_cast<SerialPort*>(param)->ReadLoop();
    return 0;
}

void SerialPort::ReadLoop() {
    uint8_t readBuf[512];
    int startMatchPos = 0;
    int errorCheckCounter = 0;

    while (m_hPort) {
        DWORD bytesRead = 0;
        OVERLAPPED ov = {};
        ov.hEvent = m_hReadOL;
        // Manual-reset event stays signaled after completion; reset before
        // each ReadFile so WaitForSingleObject blocks on the next I/O.
        ResetEvent(m_hReadOL);

        BOOL ok = ReadFile(m_hPort, readBuf, sizeof(readBuf), &bytesRead, &ov);
        if (!ok) {
            DWORD err = GetLastError();
            if (err != ERROR_IO_PENDING) break;

            HANDLE handles[2] = { m_hReadOL, m_hStopEvent };
            DWORD wait = WaitForMultipleObjects(
                m_hStopEvent ? 2 : 1, handles, FALSE, INFINITE);

            if (m_hStopEvent && wait == WAIT_OBJECT_0 + 1) {
                CancelIo(m_hPort);
                break;
            }
            if (wait == WAIT_OBJECT_0) {
                if (!GetOverlappedResult(m_hPort, &ov, &bytesRead, FALSE))
                    break;
            } else {
                break;
            }
        }

        if (bytesRead == 0) continue;

        // Periodically check for serial buffer overflow (every 200 reads)
        if (++errorCheckCounter >= 200) {
            errorCheckCounter = 0;
            DWORD dwErrors = 0;
            COMSTAT comStat = {};
            if (ClearCommError(m_hPort, &dwErrors, &comStat)) {
                if (dwErrors & CE_RXOVER) {
                    DebugLogger::Log("SERIAL RX OVERFLOW: input buffer overflow, %lu bytes queued",
                                     comStat.cbInQue);
                }
                if (dwErrors & CE_OVERRUN) {
                    DebugLogger::Log("SERIAL RX OVERRUN: hardware buffer overrun");
                }
                if (dwErrors & (CE_FRAME | CE_RXPARITY)) {
                    DebugLogger::Log("SERIAL RX ERROR: framing/parity error (0x%08lX)", dwErrors);
                }
            }
        }

        for (DWORD i = 0; i < bytesRead; i++) {
            uint8_t b = readBuf[i];

            switch (m_rxState) {
            case RX_IDLE:
                if (b == (uint8_t)START_MARKER[startMatchPos]) {
                    startMatchPos++;
                    if (startMatchPos == START_MARKER_LEN) {
                        startMatchPos = 0;
                        m_rxState = RX_HEADER;
                        m_rxLen = 0;
                    }
                } else {
                    if (startMatchPos > 0)
                        startMatchPos = (b == '<') ? 1 : 0;
                }
                break;

            case RX_HEADER:
                m_rxBuf[m_rxLen++] = b;
                if (m_rxLen == 2) {
                    m_expectedLen = ((uint16_t)m_rxBuf[0] << 8) | m_rxBuf[1];
                    int payloadLen = m_expectedLen - HEADER_LEN - END_MARKER_LEN;
                    if (payloadLen < 0 || payloadLen > (int)(sizeof(m_rxBuf) - END_MARKER_LEN)) {
                        m_rxState = RX_IDLE;
                        m_rxLen = 0;
                    } else {
                        m_rxLen = 0;
                        m_rxState = RX_PAYLOAD;
                    }
                }
                break;

            case RX_PAYLOAD: {
                m_rxBuf[m_rxLen++] = b;
                int payloadLen = m_expectedLen - HEADER_LEN - END_MARKER_LEN;
                if (m_rxLen == payloadLen + END_MARKER_LEN) {
                    if (memcmp(m_rxBuf + payloadLen, END_MARKER, END_MARKER_LEN) == 0) {
                        DebugLogger::Log("SERIAL RX frame: %d bytes payload", payloadLen);
                        if (payloadLen > 0 && payloadLen < 256) {
                            char tmp[257];
                            memcpy(tmp, m_rxBuf, payloadLen);
                            tmp[payloadLen] = '\0';
                            DebugLogger::Log("SERIAL RX data: %s", tmp);
                        }
                        if (payloadLen > 0 && m_callback)
                            m_callback(m_rxBuf, payloadLen, m_callbackUserData);
                    }
                    m_rxState = RX_IDLE;
                    m_rxLen = 0;
                }
                break;
            }
            }
        }
    }
}
