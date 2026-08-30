/**
 * Electron 版导航结构（沿用现有 viewKeys，套用共享 NavRail 组件）。
 * web 版用 @ysc/core/ui 的 NAV（新键）；Electron 用本表，避免重写既有面板的 viewKey。
 * desc = 页面作用说明（侧栏展开态显示）。
 */
import type { NavGroup } from '@ysc/core/ui';

export const ELECTRON_NAV: NavGroup[] = [
  {
    key: 'device',
    label: '设备',
    icon: 'device',
    items: [{ key: 'home', label: '主页 / 串口', icon: 'device', desc: '连接设备串口、KmNet 与实时监控' }],
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
    items: [{ key: 'docs', label: '文档 / 命令测试', icon: 'doc', desc: '协议文档与命令调试工具' }],
  },
  {
    key: 'firmware',
    label: '固件',
    icon: 'chip',
    items: [
      { key: 'firmware', label: '固件更新 (v1)', icon: 'chip', desc: '单 MCU 串口固件升级' },
      { key: 'towmcu-firmware', label: '固件更新 (v2 双 MCU)', icon: 'dualchip', desc: '双 MCU USB-CDC 固件升级' },
    ],
  },
  {
    key: 'advanced',
    label: '高级',
    icon: 'bug',
    items: [{ key: 'debug', label: 'HID 抓包', icon: 'bug', desc: 'USB HID 报文抓取分析' }],
  },
];
