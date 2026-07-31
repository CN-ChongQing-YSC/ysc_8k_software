#ifndef MAIN_H
#define MAIN_H

#define WIN32_LEAN_AND_MEAN
#include <winsock2.h>
#include <windows.h>
#include <shellapi.h>
#include <stdint.h>

// Tray icon message
#define WM_TRAYICON             (WM_USER + 1)
#define WM_DEBUG_LOG            (WM_USER + 2)

// Menu command IDs
#define IDM_DISCONNECT_SERIAL   2001
#define IDM_NET_SERVICE         2002
#define IDM_EXIT                2006
#define IDM_DEBUG_LOG           2007
#define IDM_COM_PORT_BASE       3000
#define IDM_COM_PORT_MAX        3099
#define IDM_BAUDRATE_BASE       4000
#define IDM_BAUDRATE_MAX        4008

// Supported baudrates (same as firmware)
static const DWORD SUPPORTED_BAUDS[] = {
    115200, 230400, 460800, 921600, 1000000, 1500000, 2000000, 3000000, 4000000
};
#define SUPPORTED_BAUD_COUNT 9

// Defaults
#define DEFAULT_SERIAL_BAUD     0  // 0 = auto-detect
#define KMBOXNET_DEFAULT_PORT   5251

struct AppState {
    HINSTANCE       hInstance       = NULL;
    HWND            hwndMain        = NULL;
    NOTIFYICONDATAW nid             = {};
    HANDLE          hMutex          = NULL;
    HANDLE          hStopEvent      = NULL;
    CRITICAL_SECTION csSerialWrite  = {};

    // Serial port
    bool            serialConnected = false;
    wchar_t         serialPortName[16] = {};
    DWORD           serialBaudRate  = DEFAULT_SERIAL_BAUD;
    HANDLE          hSerialPort     = NULL;
    HANDLE          hSerialReadThread = NULL;

    // kmboxnet UDP server
    bool            netServerRunning = false;
    SOCKET          udpSock         = INVALID_SOCKET;
    HANDLE          hNetThread      = NULL;
    uint16_t        netPort         = KMBOXNET_DEFAULT_PORT;

    // Debug window
    HWND            hwndDebug       = NULL;
    HWND            hwndDebugEdit   = NULL;
    bool            debugWindowVisible = false;

    // Named pipe server (for Electron UI)
    void           *pipeServer      = nullptr;
};

class SerialPort;
class PipeServer;

extern SerialPort g_serial;
extern PipeServer g_pipeServer;

extern AppState g_app;

// Debug log (appends to debug window)
void DebugLog(const char *fmt, ...);

LRESULT CALLBACK WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam);
void HandleMenuCommand(UINT cmdId);

#endif // MAIN_H
