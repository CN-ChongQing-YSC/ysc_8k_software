/**
 * 图标 → Material Symbols 字形名。
 * 全站统一使用 Material Symbols Outlined（填充 currentColor），对齐 wooting.io 的图标语言
 * （线上实测：viewBox 0 -960 960 960、fill:currentColor、stroke:none、20px）。
 * 杜绝混用线条/填充/不同 viewBox 的多套图标。
 */
import type { IconKey } from './nav-config';

export const ICONS: Record<IconKey, string> = {
  device: 'devices',
  plug: 'cable',
  activity: 'monitoring',
  network: 'lan',
  keyboard: 'keyboard',
  macro: 'keyboard_command_key',
  jitter: 'graphic_eq',
  curve: 'ssid_chart',
  gamepad: 'sports_esports',
  doc: 'description',
  terminal: 'terminal',
  chip: 'memory',
  dualchip: 'developer_board',
  bug: 'bug_report',
  wrench: 'build',
  menu: 'menu',
  menuOpen: 'menu_open',
};
