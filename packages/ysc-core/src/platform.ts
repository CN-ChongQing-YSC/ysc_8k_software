/**
 * 平台探测。决定使用哪套 DeviceTransport 适配器，以及哪些主机侧能力可用。
 *
 * - 'tauri': 桌面单 exe，renderer 通过 window.driverApi 兼容层接 Tauri IPC，
 *   Rust 宿主进程内独占所有 COM I/O。
 * - 'electron': 旧桌面壳，经命名管道与独立 Rust/C++ 驱动通信（已冻结）。
 * - 'web': 浏览器内通过 navigator.serial (Web Serial API) 直连 CDC 串口。
 */

export type Platform = 'web' | 'electron' | 'tauri';

function detectOnce(): Platform {
  // Tauri 注入的全局对象（withGlobalTauri 关闭时仍然存在 __TAURI_INTERNALS__）。
  if (typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__) {
    return 'tauri';
  }
  // 桌面版 shim / Electron preload 都会暴露 window.driverApi。
  if (typeof window !== 'undefined' && (window as any).driverApi) {
    return 'electron';
  }
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

/** 主机侧能力是否可用（Tauri/Electron 桌面宿主）。 */
export function isHostCapable(): boolean {
  const p = getPlatform();
  return p === 'electron' || p === 'tauri';
}
