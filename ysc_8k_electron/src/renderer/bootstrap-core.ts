/**
 * Electron renderer 启动：把共享核心 @ysc/core 接到现有 driverApi 之上。
 *
 * - 构造 DriverPipeTransport（包裹 window.driverApi），订阅同名管道事件并翻译为
 *   统一 DeviceEvent，喂给 useDeviceStore。
 * - 构造 YscDevice（传输无关命令层）。
 *
 * 关键：DriverPipeTransport 是“观察者”——它与 App.vue 里既有的 api.on 监听器
 * 并行监听同一批事件，互不干扰。因此现有面板/处理器零改动，store 同步得到一份
 * 准确的连接/端口/监控镜像，供共享 UI（NavRail / TopBar / DeviceCard）使用。
 */
import { DriverPipeTransport, YscDevice, useDeviceStore } from '@ysc/core';

export function bootstrapCore(): void {
  const api = (window as any).driverApi;
  if (!api) {
    // preload 尚未注入；Electron 下不应发生，防御性返回
    return;
  }
  const store = useDeviceStore();
  const transport = new DriverPipeTransport(api);
  const device = new YscDevice(transport);
  store.attach(device);
  void store.refreshPorts();
}
