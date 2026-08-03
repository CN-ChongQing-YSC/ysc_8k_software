import { contextBridge, ipcRenderer } from 'electron';

export interface DebugResponse {
  code: number;
  message: string | null;
  data: string | null;
}

export interface DebugCmdResult {
  cmd: string;
  ok: boolean;
  error?: string;
}

function driverSend(type: string, params?: Record<string, unknown>): void {
  ipcRenderer.invoke('driver:send', type, params);
}

contextBridge.exposeInMainWorld('driverApi', {
  // Send command to C++ driver
  send: (type: string, params?: Record<string, unknown>) => driverSend(type, params),

  // Check pipe connection
  isConnected: (): Promise<boolean> => ipcRenderer.invoke('driver:isConnected'),

  // Listen for driver events
  on(event: string, callback: (data: any) => void): () => void {
    const handler = (_e: any, data: any) => callback(data);
    ipcRenderer.on(event, handler);
    return () => ipcRenderer.removeListener(event, handler);
  },

  // Remove listener
  off(event: string, callback?: (data: any) => void): void {
    if (callback) {
      ipcRenderer.removeListener(event, callback as any);
    } else {
      ipcRenderer.removeAllListeners(event);
    }
  },

  getAppVersion: (): Promise<string> => ipcRenderer.invoke('app:getVersion'),

  openIAPFile: (): Promise<{ path: string; info: string } | null> =>
    ipcRenderer.invoke('iap:openFile'),

  // v2 towmcu panel — browse for a Left/Right encrypted .bin independently.
  openTowmcuFile: (side: 'left' | 'right'): Promise<{ path: string; info: string } | null> =>
    ipcRenderer.invoke('iap2:openFile', side),

  // ── firmware auto-update: probe backend for latest + download to cache ──
  // Inbound events firmware:download_progress / firmware:download_done arrive
  // via the generic on()/off() below — no extra preload wiring needed.
  checkFirmwareLatest: (deviceType: string) =>
    ipcRenderer.invoke('firmware:checkLatest', { deviceType }),
  listFirmwareVersions: (deviceType: string) =>
    ipcRenderer.invoke('firmware:listVersions', { deviceType }),
  downloadFirmware: (info: unknown) =>
    ipcRenderer.invoke('firmware:download', { info }),
  recordFirmwareInstalled: (deviceType: string, version: string) =>
    ipcRenderer.invoke('firmware:recordInstalled', { deviceType, version }),

  // Export documentation HTML as a PDF (hidden window + printToPDF + save dialog).
  exportPdf: (html: string, filename?: string) =>
    ipcRenderer.invoke('export:pdf', { html, filename }),

  // ---- Debug capture API ----
  debugEnter: ()                       => driverSend('debug_enter'),
  debugExit: ()                        => driverSend('debug_exit'),
  debugStatus: ()                      => driverSend('debug_status'),
  debugGetDevDescr: ()                 => driverSend('debug_get_dev_descr'),
  debugGetCfgDescr: ()                 => driverSend('debug_get_cfg_descr'),
  debugGetRepDescr: (offset = 0, max = 200) =>
    driverSend('debug_get_rep_descr', { offset, max }),
  debugGetReport: (idx: number)        => driverSend('debug_get_report', { idx }),
  debugClearReports: ()                => driverSend('debug_clear_reports'),
  debugGetDeviceInfo: ()               => driverSend('debug_get_device_info'),

  // ── CH343 / WCH 驱动检测 + 一键安装 ──
  // 检测结果经 ch343_driver_status 事件回推（on/off 监听）。
  checkCh343Driver: ()                 => driverSend('check_ch343_driver'),
  // 以管理员权限启动随包 CH343SER.EXE 安装器（用户在 GUI 里点「安装」）。
  installCh343Driver: (): Promise<{ ok: boolean; error?: string }> =>
    ipcRenderer.invoke('driver:installCh343Driver'),
});
