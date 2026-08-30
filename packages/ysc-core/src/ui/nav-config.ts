/**
 * 导航结构（数据驱动双栏 NavRail + 视图路由）。两版共用。
 * 旧 ysc_8k_electron viewKey 尽量保留映射，便于 Phase 2 收尾时 Electron 接入。
 *
 * desktopOnly 项：网页版显示为禁用「仅桌面」（kmboxnet UDP / CH343 提权）。
 * desc 项：页面作用的一句话说明，侧栏展开态显示在名称下方（也用作悬停提示）。
 */
export interface NavItem {
  key: string;
  label: string;
  icon: IconKey;
  desc?: string;
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
  | 'bug' | 'wrench'
  | 'menu' | 'menuOpen';

export const NAV: NavGroup[] = [
  {
    key: 'device',
    label: '设备',
    icon: 'device',
    items: [
      { key: 'device-card', label: '我的设备', icon: 'plug', desc: '串口连接与设备信息' },
      { key: 'monitor', label: '实时监控', icon: 'activity', desc: '鼠标移动与按键实时回显' },
      { key: 'kmnet', label: 'kmbox 网络', icon: 'network', desc: '启动 kmbox 网络服务', desktopOnly: true },
    ],
  },
  {
    key: 'input',
    label: '输入',
    icon: 'keyboard',
    items: [
      { key: 'macro', label: '宏', icon: 'macro', desc: '配置 8 组按键宏（连点/压枪等）' },
      { key: 'jitter', label: '抖动', icon: 'jitter', desc: '反检测微抖参数（振幅/频率）' },
      { key: 'mouse-curve', label: '鼠标曲线', icon: 'curve', desc: '分段自定义鼠标加速曲线' },
      { key: 'mouse-interp', label: '鼠标插值', icon: 'curve', desc: '平滑插值降噪，轨迹更稳' },
    ],
  },
  {
    key: 'mapping',
    label: '映射',
    icon: 'gamepad',
    items: [{ key: 'gamepad', label: '手柄→鼠标', icon: 'gamepad', desc: '把手柄映射为键鼠输入' }],
  },
  {
    key: 'test',
    label: '测试 / 文档',
    icon: 'doc',
    items: [
      { key: 'keyboard-tester', label: '键盘测试', icon: 'keyboard', desc: '按键响应与录入测试' },
      { key: 'command-tester', label: '命令测试', icon: 'terminal', desc: '直接发送协议命令调试' },
      { key: 'docs', label: '协议文档', icon: 'doc', desc: '命令与协议格式说明' },
    ],
  },
  {
    key: 'firmware',
    label: '固件',
    icon: 'chip',
    items: [
      { key: 'firmware-v1', label: '固件更新 (v1)', icon: 'chip', desc: '单 MCU 串口固件升级' },
      { key: 'firmware-v2', label: '固件更新 (v2 双 MCU)', icon: 'dualchip', desc: '双 MCU USB-CDC 固件升级' },
    ],
  },
  {
    key: 'advanced',
    label: '高级',
    icon: 'bug',
    items: [
      { key: 'debug', label: 'HID 抓包', icon: 'bug', desc: 'USB HID 报文抓取分析' },
      { key: 'ch343', label: 'CH343 驱动', icon: 'wrench', desc: '安装 CH343 串口驱动', desktopOnly: true },
    ],
  },
];

/** 扁平化所有 viewKey（含 desktopOnly），供视图合法性校验。 */
export const ALL_VIEW_KEYS: string[] = NAV.flatMap((g) => g.items.map((i) => i.key));

/** 默认视图。 */
export const DEFAULT_VIEW = 'device-card';
