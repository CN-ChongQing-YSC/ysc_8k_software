// firmware-updater.ts — probe the YSC firmware backend for updates + cache downloads.
//
// Talks to the firmware_server Spring Boot service (see firmware_server/README.md).
// Pure HTTP + filesystem — no serial I/O, no C++ driver involvement. Used by the
// Electron main process; exposed to the renderer via ipcMain handlers in index.ts.
//
// Version strategy: the device-side version string is messy (towmcu = "ysc-towmcu-L
// v1.0", single-MCU mouse = __DATE__ macro). We do NOT compare against it. Instead
// we record the last successfully-installed backend version per deviceType in
// installed.json, and compare that to the backend's latest semver.

import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import { app } from 'electron';

// Base URL of the firmware backend. Override for local dev via YSC_FW_BASE_URL.
// Production: Nginx on ysc-order-2026.top proxies /fwapi/ -> loopback:8082.
// HTTP for now (no HTTPS on the host yet); downloads are sha256-verified.
export const FIRMWARE_BASE_URL =
  process.env.YSC_FW_BASE_URL || 'http://ysc-order-2026.top/fwapi';

export interface LatestInfo {
  deviceType: string;
  version: string;        // semver, e.g. "1.0.1"
  filename: string;       // storage name on the server
  downloadUrl: string;    // full URL
  size: number;           // bytes
  sha256: string;
  releaseNotes?: string;
  releaseDate?: string;
  minHardware?: string;
  isNewer: boolean;       // backend version > locally-installed version
}

/** A pickable entry in the version list (GET /api/firmware/list/{dt}). */
export interface ListedVersion extends LatestInfo {
  isLatest: boolean;      // first in the version-desc list
  isInstalled: boolean;   // matches the locally-recorded installed version
}

/** Shape returned by the backend FirmwareDto. */
interface FirmwareDtoShape {
  version: string; filename: string; size: number; sha256: string;
  releaseNotes?: string; releaseDate?: string; minHardware?: string;
}

function dtoToInfo(dto: FirmwareDtoShape, deviceType: string): LatestInfo {
  return {
    deviceType,
    version: dto.version,
    filename: dto.filename,
    downloadUrl: `${FIRMWARE_BASE_URL}/api/firmware/download/${encodeURIComponent(dto.filename)}`,
    size: dto.size,
    sha256: dto.sha256,
    releaseNotes: dto.releaseNotes,
    releaseDate: dto.releaseDate,
    minHardware: dto.minHardware,
    isNewer: false,   // set by caller
  };
}

// ── cache + installed-record paths ──

function cacheDir(): string {
  const dir = path.join(app.getPath('userData'), 'firmware-cache');
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

function installedPath(): string {
  return path.join(cacheDir(), 'installed.json');
}

interface InstalledMap { [deviceType: string]: string }

function readInstalled(): InstalledMap {
  try {
    const raw = fs.readFileSync(installedPath(), 'utf-8');
    const obj = JSON.parse(raw);
    return typeof obj === 'object' && obj ? obj as InstalledMap : {};
  } catch {
    return {};
  }
}

function writeInstalled(map: InstalledMap): void {
  try {
    fs.writeFileSync(installedPath(), JSON.stringify(map, null, 2), 'utf-8');
  } catch (e) {
    console.error('[firmware-updater] write installed.json failed:', e);
  }
}

export function getInstalledVersion(deviceType: string): string | null {
  return readInstalled()[deviceType] || null;
}

export function recordInstalled(deviceType: string, version: string): void {
  const map = readInstalled();
  map[deviceType] = version;
  writeInstalled(map);
}

// ── version compare (port of firmware_server VersionComparator) ──

function normalizeVer(v: string): string[] {
  if (!v) return ['0'];
  return v.trim().replace(/^v/i, '').split('.');
}

function segValue(seg: string): number {
  const m = seg.match(/^(\d+)/);
  return m ? parseInt(m[1], 10) : 0;
}

/** Returns -1 if a<b, 1 if a>b, 0 if equal. Strips leading v, splits on '.', compares numeric segments. */
export function compareVersions(a: string, b: string): number {
  const sa = normalizeVer(a);
  const sb = normalizeVer(b);
  const len = Math.max(sa.length, sb.length);
  for (let i = 0; i < len; i++) {
    const va = i < sa.length ? segValue(sa[i]) : 0;
    const vb = i < sb.length ? segValue(sb[i]) : 0;
    if (va !== vb) return va < vb ? -1 : 1;
  }
  return 0;
}

// ── check latest ──

/**
 * Query the backend for the latest firmware of `deviceType`.
 * Returns null if the backend has no firmware for this deviceType (404),
 * or if the request fails (network error).
 */
export async function checkLatest(deviceType: string): Promise<LatestInfo | null> {
  const url = `${FIRMWARE_BASE_URL}/api/firmware/${encodeURIComponent(deviceType)}/latest`;
  let resp: Response;
  try {
    resp = await fetch(url, { method: 'GET' });
  } catch (e) {
    console.error('[firmware-updater] checkLatest fetch failed:', e);
    return null;
  }
  if (resp.status === 404) return null;
  if (!resp.ok) {
    console.error(`[firmware-updater] checkLatest HTTP ${resp.status} for ${deviceType}`);
    return null;
  }
  const dto = await resp.json() as FirmwareDtoShape;
  const installed = getInstalledVersion(deviceType);
  const info = dtoToInfo(dto, deviceType);
  info.isNewer = installed ? compareVersions(info.version, installed) > 0 : true;
  return info;
}

/**
 * List all `listed=true` firmwares for `deviceType` (version-descending).
 * Each entry is enriched with `isLatest` (first in list) and `isInstalled`
 * (matches the locally-recorded installed version) so the renderer can badge
 * rows without a second IPC round-trip.
 */
export async function listVersions(deviceType: string): Promise<ListedVersion[]> {
  const url = `${FIRMWARE_BASE_URL}/api/firmware/list/${encodeURIComponent(deviceType)}`;
  let resp: Response;
  try {
    resp = await fetch(url);
  } catch (e) {
    console.error('[firmware-updater] listVersions fetch failed:', e);
    return [];
  }
  if (!resp.ok) return [];
  const data = await resp.json() as { firmwares?: FirmwareDtoShape[] };
  const installed = getInstalledVersion(deviceType);
  const list = data.firmwares || [];
  return list.map((dto, i) => {
    const info = dtoToInfo(dto, deviceType);
    info.isNewer = installed ? compareVersions(info.version, installed) > 0 : true;
    return {
      ...info,
      isLatest: i === 0,   // backend returns version-desc
      isInstalled: installed ? compareVersions(info.version, installed) === 0 : false,
    };
  });
}

// ── download (with sha256 verify + progress) ──

export interface DownloadResult { buffer: Buffer; info: string }

/**
 * Download `info` into an in-memory Buffer (no disk cache). Verifies sha256
 * before returning; throws on mismatch. The buffer is then Base64-encoded
 * by the caller and shipped to the C++ driver through the named pipe, so the
 * firmware never touches disk between download and flashing.
 */
export async function download(
  info: LatestInfo,
  onProgress: (received: number, total: number, percent: number) => void,
): Promise<DownloadResult> {
  const resp = await fetch(info.downloadUrl);
  if (!resp.ok || !resp.body) {
    throw new Error(`下载失败 HTTP ${resp.status}`);
  }
  const total = Number(resp.headers.get('content-length') || info.size || 0);
  const reader = resp.body.getReader();
  const chunks: Buffer[] = [];
  let received = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(Buffer.from(value));
    received += value.length;
    onProgress(received, total, total > 0 ? Math.round((received / total) * 100) : 0);
  }
  const buf = Buffer.concat(chunks);

  // sha256 verify
  const hash = crypto.createHash('sha256').update(buf).digest('hex');
  if (hash !== info.sha256) {
    throw new Error(`sha256 校验失败 (期望 ${info.sha256.slice(0, 12)}…, 实际 ${hash.slice(0, 12)}…)`);
  }

  const sizeStr = buf.length >= 1024 ? `${(buf.length / 1024).toFixed(1)} KB` : `${buf.length} 字节`;
  return { buffer: buf, info: `${info.filename} — ${sizeStr}` };
}
