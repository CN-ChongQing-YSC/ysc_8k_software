/**
 * 导航结构（数据驱动双栏 NavRail + 视图路由）。两版共用。
 * 旧 ysc_8k_electron viewKey 尽量保留映射，便于 Phase 2 收尾时 Electron 接入。
 *
 * desktopOnly 项：网页版显示为禁用「仅桌面」（kmboxnet UDP / CH343 提权）。
 */
export interface NavItem {
  key: string;
  label: string;
  icon: IconKey;
  desktopOnly?: boolean;
}
export interface NavGroup {
  key: string;
  label: string;
  icon: IconKey;
  items: NavItem[];
}

export type IconKey =
  | 'device' | 'plug' | 'activity' | 'network'
  | 'keyboard' | 'macro' | 'jitter' | 'curve'
  | 'gamepad'
  | 'doc' | 'terminal'
  | 'chip' | 'dualchip'
  | 'bug' | 'wrench';

export const NAV: NavGroup[] = [
  {
    key: 'device',
    label: '设备',
    icon: 'device',
    items: [
      { key: 'device-card', label: '我的设备', icon: 'plug' },
      { key: 'monitor', label: '实时监控', icon: 'activity' },
      { key: 'kmnet', label: 'kmbox 网络', icon: 'network', desktopOnly: true },
    ],
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
    items: [
      { key: 'keyboard-tester', label: '键盘测试', icon: 'keyboard' },
      { key: 'command-tester', label: '命令测试', icon: 'terminal' },
      { key: 'docs', label: '协议文档', icon: 'doc' },
    ],
  },
  {
    key: 'firmware',
    label: '固件',
    icon: 'chip',
    items: [
      { key: 'firmware-v1', label: '固件更新 (v1)', icon: 'chip' },
      { key: 'firmware-v2', label: '固件更新 (v2 双 MCU)', icon: 'dualchip' },
    ],
  },
  {
    key: 'advanced',
    label: '高级',
    icon: 'bug',
    items: [
      { key: 'debug', label: 'HID 抓包', icon: 'bug' },
      { key: 'ch343', label: 'CH343 驱动', icon: 'wrench', desktopOnly: true },
    ],
  },
];

/** 扁平化所有 viewKey（含 desktopOnly），供视图合法性校验。 */
export const ALL_VIEW_KEYS: string[] = NAV.flatMap((g) => g.items.map((i) => i.key));

/** 默认视图。 */
export const DEFAULT_VIEW = 'device-card';
