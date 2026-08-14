/**
 * 版本字符串解析与侧别分类（cmd 132 回读）。
 *
 * 网页版 navigator.serial 不暴露 USB iSerialNumber，无法像 C++ 那样靠
 * TOWMCULEFT/TOWMCURIGHT/TOWMCUIAP 分类。改用 cmd 132 的版本字符串分类：
 *   "ysc-towmcu-L v1.0" → Left
 *   "ysc-towmcu-R v1.0" → Right
 *   "YSC-IAP ..."       → IAP（烧录模式）
 * 对应 towmcu_iap_upgrader.cpp:372 QueryVersionOnPort 的判别逻辑。
 */
import type { TowmcuSide } from '../transport/types';

export interface VersionInfo {
  raw: string;
  side: TowmcuSide;
  isIap: boolean;
  /** 提取出的版本号（如 "1.0"、"2.14.0"）；提取失败则回退原文。 */
  clean: string;
}

export function parseVersion(raw: string): VersionInfo {
  const s = (raw || '').trim();
  const lower = s.toLowerCase();
  let side: TowmcuSide = '';
  if (lower.includes('iap')) side = 'IAP';
  else if (lower.includes('towmcu-l') || lower.includes('left')) side = 'Left';
  else if (lower.includes('towmcu-r') || lower.includes('right')) side = 'Right';
  const m = s.match(/v?\d+\.\d+(?:\.\d+)?/i);
  return { raw: s, side, isIap: side === 'IAP', clean: m ? m[0] : s };
}
