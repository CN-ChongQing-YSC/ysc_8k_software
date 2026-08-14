/**
 * @ysc/core —— YSC 8K 设备软件共享核心（Electron + 网页版共用）。
 *
 * 公共出口按子域分桶，消费方既可 `import { ... } from '@ysc/core'`，
 * 也可按需 `import { ... } from '@ysc/core/protocol'` 缩小打包范围。
 *
 * 详见根目录 CLAUDE.md 与计划文件。
 */

export * from './platform';

// 协议层（Phase 1）：帧编解码、接收状态机、校验和、命令码常量
export * from './protocol';

// 传输层（Phase 2/3）：DeviceTransport 接口 + DeviceEvent 联合 + 两套适配器
export * from './transport';

// 设备命令层（Phase 2/3）：YscDevice（传输无关）、固件烧录、版本探测、监控分发
export * from './device';

// 状态层（Phase 2）：Pinia useDeviceStore / useUiStore
export * from './store';

// UI 外壳（Phase 4）：AppShell / NavRail / TopBar / DeviceCard / DeviceCanvas / tokens / nav-config
export * from './ui';

// 国际化（Phase 0 已迁入）：useI18n / setLang / getLang
export * from './i18n';
