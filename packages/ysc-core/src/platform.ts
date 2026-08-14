/**
 * 平台探测。决定使用哪套 DeviceTransport 适配器，以及哪些主机侧能力可用。
 *
 * - 'electron': renderer 通过 window.driverApi 经命名管道与 ysc_8k_driver.exe 通信，
 *   驱动独占所有 COM I/O。kmboxnet UDP 桥、CH343 提权安装等主机能力可用。
 * - 'web': 浏览器内通过 navigator.serial (Web Serial API) 直连 CDC 串口。
 *   主机侧能力（UDP 监听、提权安装、SetupAPI 检测）不可用，需桌面降级提示。
 */

export type Platform = 'web' | 'electron';

function detectOnce(): Platform {
  // Electron renderer 暴露 window.driverApi（preload contextBridge 注入）
  if (typeof window !== 'undefined' && (window as any).driverApi) {
    return 'electron';
  }
  // 进程级 uelectron 注入变量（vite-plugin-electron-renderer / 预加载）
  if (typeof navigator !== 'undefined' && (navigator as any).userAgent?.includes('Electron')) {
    return 'electron';
  }
  return 'web';
}

let _platform: Platform | null = null;

export function getPlatform(): Platform {
  if (_platform === null) _platform = detectOnce();
  return _platform;
}

/** Web Serial 是否可用（仅 Chromium 系浏览器 + 安全源）。 */
export function isWebSerialSupported(): boolean {
  return typeof navigator !== 'undefined' && 'serial' in navigator;
}

/** 主机侧能力是否可用（仅 Electron）。kmboxnet UDP / CH343 提权安装依赖此项。 */
export function isHostCapable(): boolean {
  return getPlatform() === 'electron';
}
