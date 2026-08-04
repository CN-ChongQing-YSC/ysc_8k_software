#ifndef SERIAL_PORT_H
#define SERIAL_PORT_H

#define WIN32_LEAN_AND_MEAN
#include <winsock2.h>
#include <windows.h>
#include "kmboxnet_proto.h"

// 读线程异常退出（串口物理断开/IO 错误）时 PostMessage 给主窗口的通知消息。
// wParam = 该 reader 启动时的代际（m_readerGen），主线程据此判断消息是否过期。
#define WM_SERIAL_LOST          (WM_USER + 3)

class SerialPort {
public:
    SerialPort();
    ~SerialPort();

    bool Connect(const wchar_t *portName, uint32_t baudRate);
    void Disconnect();
    bool IsConnected() const;
    uint32_t GetBaudRate() const;
    const wchar_t* GetPortName() const;

    // Auto-detect hardware baudrate by probing each rate with version query.
    // Returns detected baudrate, or 0 on failure.
    static uint32_t DetectBaudrate(const wchar_t *portName);

    // Switch to a new baudrate: sends CMD_SET_BAUDRATE to hardware,
    // then reopens the port at the new rate. Returns true on success.
    bool SwitchBaudrate(uint32_t newBaud);

    // Send JSON command wrapped in <START>...<END> framing
    bool SendJsonCommand(const char *json);

    // Send raw bytes directly (no framing)
    bool SendRaw(const uint8_t *data, int len);

    // Start/stop background read thread
    bool StartReadThread(HANDLE hStopEvent);
    void StopReadThread();

    // 读线程异常退出（串口物理断开/IO 错误）时，若设置了通知窗口则 PostMessage(WM_SERIAL_LOST)。
    // wParam = 该 reader 启动时的代际(m_readerGen)，主线程用它判断消息是否过期。
    void SetNotifyWindow(HWND h) { m_hNotifyWnd = h; }
    LONG ReaderGen() const { return m_readerGen; }   // 当前 reader 代际，供主线程比对

    // Callback for received framed data
    typedef void (*ResponseCallback)(const uint8_t *data, int len, void *userData);
    void SetResponseCallback(ResponseCallback cb, void *userData);

private:
    static DWORD WINAPI ReadThreadProc(LPVOID param);
    void ReadLoop();

    static DWORD WINAPI WriteThreadProc(LPVOID param);
    void WriteLoop();
    bool StartWriteThread();
    void StopWriteThread();
    bool EnqueueFrame(const uint8_t *data, int len);

    HANDLE  m_hPort;
    uint32_t m_baudRate;
    wchar_t m_portName[16];
    HANDLE  m_hReadThread;
    HANDLE  m_hStopEvent;

    // 读线程异常退出上报：通知窗口 + reader 代际（每次 StartReadThread 自增）
    HWND    m_hNotifyWnd;
    volatile LONG m_readerGen;   // 当前 reader 代际（Interlocked 自增）
    LONG    m_startedGen;        // 本次 reader 启动时的代际（供 ReadLoop 入口读取）
    volatile LONG m_disconnecting; // 1=正在/已经断开：Disconnect 互斥 + 读线程仅在 0 时上报

    // RX framing state machine
    enum RxState { RX_IDLE, RX_HEADER, RX_PAYLOAD };
    RxState m_rxState;
    uint8_t m_rxBuf[4096];
    int     m_rxLen;
    uint16_t m_expectedLen;

    // Callback
    ResponseCallback m_callback;
    void    *m_callbackUserData;

    // Write mutex (used during baudrate switch direct writes)
    CRITICAL_SECTION m_csWrite;

    // TX async write queue
    enum { QUEUE_CAP = 64, FRAME_SZ = 2048 };

    struct FrameSlot {
        uint8_t data[FRAME_SZ];
        int len;
    };

    FrameSlot m_writeQueue[QUEUE_CAP];
    int m_queueHead;
    int m_queueTail;
    int m_queueCount;
    CRITICAL_SECTION m_csQueue;
    CONDITION_VARIABLE m_cvNotEmpty;
    HANDLE m_hWriteThread;
    volatile bool m_writeRunning;

    // Overlapped I/O events
    HANDLE m_hReadOL;
    HANDLE m_hWriteOL;

    static constexpr const char *START_MARKER = "<START>";
    static constexpr const char *END_MARKER   = "<END>";
    static constexpr int START_MARKER_LEN     = 7;
    static constexpr int END_MARKER_LEN       = 5;
    static constexpr int HEADER_LEN           = START_MARKER_LEN + 2; // <START> + 2-byte length
};

#endif // SERIAL_PORT_H
