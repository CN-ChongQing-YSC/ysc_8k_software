/**
 * 网页版启动：构造 Web Serial 传输适配器 + YscDevice，注入 Pinia store。
 * 浏览器不支持 Web Serial 时只置空，App.vue 显示降级横幅。
 */
import { WebSerialTransport } from '@ysc/core/transport';
import { YscDevice } from '@ysc/core/device';
import { useDeviceStore } from '@ysc/core/store';
import { isWebSerialSupported } from '@ysc/core/platform';

export function bootstrap(): void {
  const store = useDeviceStore();
  if (!isWebSerialSupported()) {
    return; // UI 负责提示
  }
  const transport = new WebSerialTransport();
  const device = new YscDevice(transport);
  store.attach(device);
  void store.refreshPorts();
}
