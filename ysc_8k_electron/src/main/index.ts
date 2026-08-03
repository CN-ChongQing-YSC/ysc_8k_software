import { app, BrowserWindow, Menu, globalShortcut, ipcMain, dialog, screen } from 'electron';
import * as path from 'path';
import * as fs from 'fs';
import { spawn, ChildProcess } from 'child_process';
import { DriverPipe } from './driver-pipe';
import * as firmwareUpdater from './firmware-updater';

// Hide default menu bar
Menu.setApplicationMenu(null);

// Set AppUserModelId so Windows taskbar shows the correct icon
app.setAppUserModelId('com.ysc.8k-driver');

let mainWindow: BrowserWindow | null = null;
let driverProcess: ChildProcess | null = null;
const driver = new DriverPipe();

// 在线下载的固件暂存：deviceType -> { buffer, sha256 }。下载完成后存入这里，
// 渲染进程触发 iap_start_mem / towmcu_start_mem 时取出转 Base64 发给 C++ 驱动。
// 烧录完成后由新一次下载覆盖（固件几十~几百 KB，left+right 两个键峰值约 1MB）。
const pendingFirmware = new Map<string, { buffer: Buffer; sha256: string }>();

// --- Window state persistence ---
const stateFilePath = path.join(app.getPath('userData'), 'window-state.json');

function loadWindowState(): { x?: number; y?: number; width: number; height: number; maximized?: boolean } {
  try {
    const raw = fs.readFileSync(stateFilePath, 'utf-8');
    const state = JSON.parse(raw);
    if (state && typeof state.width === 'number' && typeof state.height === 'number') {
      // Ensure the saved position is still on a connected display
      if (typeof state.x === 'number' && typeof state.y === 'number') {
        const displays = screen.getAllDisplays();
        const visible = displays.some(d => {
          return state.x >= d.bounds.x - 50 && state.x <= d.bounds.x + d.bounds.width - 50 &&
                 state.y >= d.bounds.y - 50 && state.y <= d.bounds.y + d.bounds.height - 50;
        });
        if (!visible) return { width: state.width, height: state.height };
      }
      return state;
    }
  } catch {}
  return { width: 720, height: 560 };
}

function saveWindowState(): void {
  if (!mainWindow || mainWindow.isDestroyed()) return;
  try {
    const state: Record<string, number | boolean> = {};
    if (!mainWindow.isMaximized() && !mainWindow.isMinimized()) {
      const bounds = mainWindow.getBounds();
      state.x = bounds.x;
      state.y = bounds.y;
      state.width = bounds.width;
      state.height = bounds.height;
    } else {
      const saved = loadWindowState();
      state.width = saved.width;
      state.height = saved.height;
    }
    state.maximized = mainWindow.isMaximized();
    fs.writeFileSync(stateFilePath, JSON.stringify(state), 'utf-8');
  } catch {}
}

// --- Driver executable path ---
function getDriverExePath(): string {
  if (app.isPackaged) {
    return path.join(process.resourcesPath, 'ysc_8k_driver.exe');
  }
  return path.resolve(__dirname, '../../ysc_8k_driver/work/Release/ysc_8k_driver.exe');
}

// --- Spawn C++ driver process ---
let launchedDriver = false;

function startDriverProcess(): void {
  const exePath = getDriverExePath();
  driverProcess = spawn(exePath, [], {
    detached: false,
    windowsHide: false,
  });
  launchedDriver = true;

  driverProcess.on('exit', (code) => {
    console.log(`Driver process exited with code ${code}`);
    driverProcess = null;
  });

  driverProcess.on('error', (err) => {
    console.error('Failed to start driver process:', err);
  });
}

// --- Connect to driver with retries ---
async function connectWithRetries(maxRetries = 10, interval = 500): Promise<void> {
  for (let i = 0; i < maxRetries; i++) {
    try {
      await driver.connect(2000);
      return;
    } catch {
      await new Promise(r => setTimeout(r, interval));
    }
  }
  throw new Error('Failed to connect to driver after retries');
}

// --- Resolve icon path ---
function getIconPath(): string {
  if (app.isPackaged) {
    return path.join(process.resourcesPath, 'icon.ico');
  }
  return path.resolve(__dirname, '../resources/icon.ico');
}

// --- 随包 CH343 驱动安装器路径（CH343SER.EXE）---
function getCh343InstallerPath(): string {
  if (app.isPackaged) {
    return path.join(process.resourcesPath, 'ch343-driver', 'CH343SER.EXE');
  }
  return path.resolve(__dirname, '../resources/ch343-driver/CH343SER.EXE');
}

// --- Create main window ---
function createMainWindow(): void {
  const saved = loadWindowState();

  mainWindow = new BrowserWindow({
    width: saved.width,
    height: saved.height,
    ...(saved.x !== undefined && saved.y !== undefined ? { x: saved.x, y: saved.y } : {}),
    minWidth: 600,
    minHeight: 480,
    frame: false,
    resizable: true,
    backgroundColor: '#0f1117',
    icon: getIconPath(),
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  if (saved.maximized) {
    mainWindow.maximize();
  }

  if (process.env.VITE_DEV_SERVER_URL) {
    mainWindow.loadURL(process.env.VITE_DEV_SERVER_URL);
  } else {
    mainWindow.loadFile(path.join(__dirname, '../dist/index.html'));
  }

  mainWindow.on('closed', () => {
    saveWindowState();
    mainWindow = null;
  });

  // Save state on resize/move (debounced)
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  mainWindow.on('resize', () => {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(saveWindowState, 500);
  });
  mainWindow.on('move', () => {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(saveWindowState, 500);
  });

  // Disable default right-click context menu
  mainWindow.webContents.on('context-menu', (e) => {
    e.preventDefault();
  });

  // Dev-only: open DevTools via Ctrl+Shift+I or F12
  // (Menu.setApplicationMenu(null) above removes the default accelerator)
  if (!app.isPackaged) {
    mainWindow.webContents.on('before-input-event', (e, input) => {
      if (input.type !== 'keyDown') return;
      const key = input.key.toLowerCase();
      if (key === 'f12' || (input.control && input.shift && key === 'i')) {
        mainWindow?.webContents.toggleDevTools();
        e.preventDefault();
      } else if (input.control && input.shift && key === 'j') {
        mainWindow?.webContents.openDevTools({ mode: 'detach' });
        e.preventDefault();
      } else if (input.control && key === 'r') {
        mainWindow?.webContents.reload();
        e.preventDefault();
      }
    });
  }
}

// --- Forward driver events to renderer ---
function sendToRenderer(channel: string, data?: any): void {
  const wc = mainWindow?.webContents;
  if (wc && !wc.isDestroyed()) {
    try { wc.send(channel, data); } catch {}
  }
}

function setupDriverEvents(): void {
  const forwardEvents = [
    'serial_connected', 'serial_error', 'serial_disconnected',
    'baudrate_switched', 'baudrate_failed',
    'kmnet_started', 'kmnet_error', 'kmnet_stopped',
    'monitor_data',
    'local_ip', 'version', 'state', 'ports_list',
    'iap_log', 'iap_progress', 'iap_done',
    'towmcu_ports', 'towmcu_version',
    'iap2_log', 'iap2_progress', 'iap2_done',
    'ch343_driver_status',
    'debug_cmd_result', 'debug_response',
  ];

  for (const evt of forwardEvents) {
    driver.on(evt, (data: any) => sendToRenderer(evt, data));
  }

  driver.on('disconnected', () => sendToRenderer('disconnected'));
}

// --- IPC handlers ---
function setupIpc(): void {
  ipcMain.handle('driver:send', (_e, type: string, params?: Record<string, unknown>) => {
    if (type === 'window_minimize') {
      mainWindow?.minimize();
    } else if (type === 'window_maximize') {
      if (mainWindow?.isMaximized()) {
        mainWindow.unmaximize();
      } else {
        mainWindow?.maximize();
      }
    } else if (type === 'app_quit') {
      driver.disconnect();
      if (launchedDriver && driverProcess) {
        driverProcess.kill();
        driverProcess = null;
      }
      app.quit();
    } else if (type === 'iap_start_mem') {
      const dt = (params?.deviceType as string) || '';
      const baud = params?.baud as number | undefined;
      const entry = pendingFirmware.get(dt);
      if (!entry) {
        sendToRenderer('iap_log', { message: '未找到已下载固件，请重新下载', cls: 'err' });
        return;
      }
      driver.iapStartMem(entry.buffer, entry.sha256, baud);
    } else if (type === 'towmcu_start_mem') {
      const dt = (params?.deviceType as string) || '';
      const port = (params?.port as string) || '';
      const entry = pendingFirmware.get(dt);
      if (!entry) {
        sendToRenderer('iap2_log', { message: '未找到已下载固件，请重新下载', cls: 'err' });
        return;
      }
      driver.towmcuStartMem(port, entry.buffer, entry.sha256);
    } else {
      driver.send(type, params);
    }
  });

  ipcMain.handle('driver:isConnected', () => driver.isConnected);

  ipcMain.handle('app:getVersion', () => app.getVersion());

  ipcMain.handle('iap:openFile', async () => {
    const result = await dialog.showOpenDialog(mainWindow!, {
      title: '选择加密固件文件',
      filters: [{ name: 'Binary', extensions: ['bin'] }, { name: 'All', extensions: ['*'] }],
      properties: ['openFile'],
    });
    if (result.canceled || result.filePaths.length === 0) return null;
    const fs = await import('fs');
    const p = result.filePaths[0];
    const stat = fs.statSync(p);
    const name = p.split(/[/\\]/).pop() || p;
    const sizeStr = stat.size >= 1024 ? `${(stat.size / 1024).toFixed(1)} KB` : `${stat.size} 字节`;
    return { path: p, info: `${name} — ${sizeStr}` };
  });

  // v2 towmcu panel — browse for a Left/Right encrypted .bin independently.
  ipcMain.handle('iap2:openFile', async (_e, side: 'left' | 'right') => {
    const title = side === 'left' ? '选择左固件 (Left)' : '选择右固件 (Right)';
    const result = await dialog.showOpenDialog(mainWindow!, {
      title,
      filters: [
        { name: '加密固件 bin', extensions: ['bin'] },
        { name: 'All', extensions: ['*'] },
      ],
      properties: ['openFile'],
    });
    if (result.canceled || result.filePaths.length === 0) return null;
    const fs = await import('fs');
    const p = result.filePaths[0];
    const stat = fs.statSync(p);
    const name = p.split(/[/\\]/).pop() || p;
    const sizeStr = stat.size >= 1024 ? `${(stat.size / 1024).toFixed(1)} KB` : `${stat.size} 字节`;
    return { path: p, info: `${name} — ${sizeStr}` };
  });

  // ── firmware auto-update: probe backend for latest, download to cache ──
  // Pure HTTP, no C++ driver involvement. See firmware-updater.ts.
  ipcMain.handle('firmware:checkLatest', async (_e, { deviceType }) => {
    return await firmwareUpdater.checkLatest(deviceType);
  });

  ipcMain.handle('firmware:listVersions', async (_e, { deviceType }) => {
    return await firmwareUpdater.listVersions(deviceType);
  });

  ipcMain.handle('firmware:download', async (_e, { info }) => {
    // Fire-and-forget — progress/result arrive via sendToRenderer events so the
    // renderer stays responsive for large downloads.
    firmwareUpdater.download(info, (received, total, percent) => {
      sendToRenderer('firmware:download_progress', {
        deviceType: info.deviceType, received, total, percent,
      });
    }).then((result) => {
      // 暂存到内存，渲染进程触发 iap_start_mem / towmcu_start_mem 时取出。
      pendingFirmware.set(info.deviceType, { buffer: result.buffer, sha256: info.sha256 });
      sendToRenderer('firmware:download_done', {
        deviceType: info.deviceType, success: true, info: result.info,
      });
    }).catch((err: Error) => {
      sendToRenderer('firmware:download_done', {
        deviceType: info.deviceType, success: false, error: err.message || String(err),
      });
    });
    return null;
  });

  // Record the last successfully-installed version per deviceType (called by the
  // renderer after iap_done/iap2_done success). Drives the isNewer comparison.
  ipcMain.handle('firmware:recordInstalled', (_e, { deviceType, version }) => {
    firmwareUpdater.recordInstalled(deviceType, version);
    return null;
  });

  // ── Export documentation as PDF ──
  // The renderer builds a full HTML document (box usage + protocol commands +
  // statuses + v1/v2 consistency) and sends it here. We write it to a temp file
  // (more reliable than a giant data: URL), render it offscreen in a hidden
  // window, call printToPDF, then show a save dialog. Pure Electron, no deps.
  // Every step is appended to export-pdf.log in userData so runtime failures
  // can be diagnosed from the log file.
  const pdfLogPath = path.join(app.getPath('userData'), 'export-pdf.log');
  function logPdf(msg: string): void {
    const line = `[${new Date().toISOString()}] ${msg}\n`;
    try { fs.appendFileSync(pdfLogPath, line, 'utf-8'); } catch {}
    try { console.log('[export:pdf] ' + msg); } catch {}
  }
  ipcMain.handle('export:pdf', async (_e, { html, filename }: { html: string; filename?: string }) => {
    logPdf('handler entered; html=' + (html ? html.length : 0) + ' bytes; filename=' + (filename || '(default)'));
    let win: BrowserWindow | null = null;
    let tmpPath = '';
    try {
      tmpPath = path.join(app.getPath('temp'), 'ysc-pdf-' + Date.now() + '.html');
      fs.writeFileSync(tmpPath, html, 'utf-8');
      logPdf('temp html written: ' + tmpPath);

      win = new BrowserWindow({
        show: false,
        width: 900,
        height: 1200,
        backgroundColor: '#ffffff',
        webPreferences: { sandbox: true },
      });
      let loadError = '';
      win.webContents.on('did-fail-load', (_we, errorCode, errorDescription) => {
        loadError = `did-fail-load ${errorCode}: ${errorDescription}`;
        logPdf('did-fail-load: ' + loadError);
      });

      logPdf('loadFile...');
      await win.loadFile(tmpPath);
      logPdf('loadFile ok');
      await new Promise((r) => setTimeout(r, 200));
      if (loadError) throw new Error(loadError);

      logPdf('printToPDF...');
      const pdf = await win.webContents.printToPDF({ printBackground: true, pageSize: 'A4' });
      logPdf('printToPDF ok: ' + pdf.length + ' bytes');

      const parent = mainWindow && !mainWindow.isDestroyed() ? mainWindow : undefined;
      const result = await dialog.showSaveDialog(parent as BrowserWindow, {
        title: '导出为 PDF',
        defaultPath: filename || 'YSC-8K-使用与协议文档.pdf',
        filters: [{ name: 'PDF', extensions: ['pdf'] }],
      });
      if (result.canceled || !result.filePath) {
        logPdf('save dialog canceled');
        return { success: false, canceled: true, log: pdfLogPath };
      }
      fs.writeFileSync(result.filePath, pdf);
      logPdf('written: ' + result.filePath);
      return { success: true, path: result.filePath, log: pdfLogPath };
    } catch (err: any) {
      const msg = err?.message || String(err);
      const stack = err?.stack || '';
      logPdf('ERROR: ' + msg + '\n' + stack);
      return { success: false, error: msg, log: pdfLogPath };
    } finally {
      if (win) win.destroy();
      if (tmpPath) { try { fs.unlinkSync(tmpPath); } catch {} }
    }
  });

  // ── 一键安装 CH343 驱动：以管理员权限启动随包 CH343SER.EXE ──
  // 用 PowerShell Start-Process -Verb RunAs 触发 UAC；用户在 WCH 官方 GUI 里点「安装」。
  // 不等待 GUI 结束（Start-Process -Verb RunAs 立即返回）——前端稍后自行重新检测。
  ipcMain.handle('driver:installCh343Driver', async () => {
    const exe = getCh343InstallerPath();
    if (!fs.existsSync(exe)) {
      return { ok: false, error: 'CH343SER.EXE not found: ' + exe };
    }
    // 路径里可能有空格/CJK，用单引号传给 PowerShell，内部单引号翻倍转义。
    const psPath = "'" + exe.replace(/'/g, "''") + "'";
    return new Promise((resolve) => {
      const ps = spawn('powershell.exe', [
        '-NoProfile', '-WindowStyle', 'Hidden', '-Command',
        `Start-Process -FilePath ${psPath} -Verb RunAs`,
      ]);
      ps.on('error', (err) => resolve({ ok: false, error: err.message }));
      ps.on('close', (code) => {
        // code 0 = 已以管理员权限拉起（用户取消 UAC 时 Start-Process 抛错→非 0）。
        resolve(code === 0 ? { ok: true } : { ok: false, error: 'launch failed (code ' + code + ')' });
      });
    });
  });
}

// --- App lifecycle ---
app.whenReady().then(async () => {
  const gotLock = app.requestSingleInstanceLock();
  if (!gotLock) {
    app.quit();
    return;
  }

  try {
    await driver.connect(2000);
  } catch {
    startDriverProcess();
    try {
      await connectWithRetries(5, 500);
    } catch (err) {
      console.error('Could not connect to driver:', err);
    }
  }

  setupDriverEvents();
  createMainWindow();
  setupIpc();

  driver.getState();
  driver.getVersion();
});

app.on('window-all-closed', () => {
  app.quit();
});

app.on('before-quit', () => {
  driver.disconnect();
  if (launchedDriver && driverProcess) {
    driverProcess.kill();
    driverProcess = null;
  }
});

app.on('second-instance', () => {
  mainWindow?.show();
  mainWindow?.focus();
});
