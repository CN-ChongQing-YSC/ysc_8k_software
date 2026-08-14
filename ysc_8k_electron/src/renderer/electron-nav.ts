/**
 * Electron 版导航结构（沿用现有 viewKeys，套用共享 NavRail 双栏组件）。
 * web 版用 @ysc/core/ui 的 NAV（新键）；Electron 用本表，避免重写既有面板的 viewKey。
 */
import type { NavGroup } from '@ysc/core/ui';

export const ELECTRON_NAV: NavGroup[] = [
  {
    key: 'device',
    label: '设备',
    icon: 'device',
    items: [{ key: 'home', label: '主页 / 串口', icon: 'device' }],
  },
  {
    key: 'input',
    label: '输入',
    icon: 'keyboard',
    items: [
      { key: 'macro', label: '宏', icon: 'macro' },
      { key: 'jitter', label: '抖动', icon: 'jitter' },
      { key: 'mouse-curve', label: '鼠标曲线', icon: 'curve' },
    ],
  },
  {
    key: 'mapping',
    label: '映射',
    icon: 'gamepad',
    items: [{ key: 'gamepad', label: '手柄→鼠标', icon: 'gamepad' }],
  },
  {
    key: 'test',
    label: '测试 / 文档',
    icon: 'doc',
    items: [{ key: 'docs', label: '文档 / 命令测试', icon: 'doc' }],
  },
  {
    key: 'firmware',
    label: '固件',
    icon: 'chip',
    items: [
      { key: 'firmware', label: '固件更新 (v1)', icon: 'chip' },
      { key: 'towmcu-firmware', label: '固件更新 (v2 双 MCU)', icon: 'dualchip' },
    ],
  },
  {
    key: 'advanced',
    label: '高级',
    icon: 'bug',
    items: [{ key: 'debug', label: 'HID 抓包', icon: 'bug' }],
  },
];
