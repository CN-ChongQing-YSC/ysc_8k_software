/**
 * C++ 驱动日志/错误文案的英文翻译层。
 *
 * C++ 端（ysc_8k_driver）所有面向用户的日志与错误都是中文字面量，协议里没有语言字段。
 * 主进程在把驱动事件转发给渲染进程之前，按当前界面语言把 `message` / `error` / `status`
 * 翻成英文，避免英文界面里冒出中文（升级日志、串口错误、未连接提示等）。
 *
 * 只处理上面这几个展示字段，不动端口名、版本串、product、cmd 等协议数据。
 * 未收录的长尾日志走词级兜底，保证英文模式下不残留中文。
 */
import { getMainLang } from './i18n';

const EXACT: Record<string, string> = {
  '未连接设备': 'Device not connected',
  '设备未连接': 'Device not connected',
  '设备已断开': 'Device disconnected',
  '无法检测到设备': 'Device not detected',
  '无法打开串口': 'Cannot open serial port',
  '无法连接设备': 'Cannot connect to the device',
  '无法打开 IAP 端口': 'Cannot open the IAP port',
  '无法与设备通信 — 请检查端口/连线': 'Cannot communicate with the device - check the port/cable',
  '波特率切换失败': 'Baudrate switch failed',
  '擦除: 响应超时': 'Erase: response timed out',
  '用户取消': 'Canceled by user',
  '未指定串口端口': 'Serial port not specified',
  '未指定端口或固件': 'Port or firmware not specified',
  '未指定端口或固件数据': 'Port or firmware data not specified',
  'Net 服务启动失败': 'Failed to start the NET service',
  '程序已在运行': 'Already running',
  '已进入下载模式': 'Entered download mode',
  '升级正在进行中': 'Upgrade already in progress',
  '升级正在进行中，无法查询版本': 'Upgrade in progress - cannot query the version',
  '未选择固件文件': 'No firmware file selected',
  '未收到固件数据': 'No firmware data received',
  '固件 Base64 解码失败': 'Firmware Base64 decode failed',
  '固件没数据': 'No firmware data',
  '固件数据为空': 'Firmware data is empty',
  '读取固件文件失败': 'Failed to read the firmware file',
  '无法打开固件文件': 'Cannot open the firmware file',
  '波特率切换成功': 'Baudrate switched',
  '探测波特率...': 'Probing baudrate...',
  '发送擦除命令...': 'Sending erase command...',
  '发送结束命令...': 'Sending end command...',
  '开始校验...': 'Verifying...',
  '固件升级完成! 设备将重启进入 APP': 'Firmware upgrade complete! The device will reboot into APP',
  '固件升级完成! 设备将跳转到 APP ...': 'Firmware upgrade complete! The device will jump to APP ...',
  '等待设备重启...': 'Waiting for the device to reboot...',
  '等待设备重启 ...': 'Waiting for the device to reboot...',
  '重新连接设备...': 'Reconnecting the device...',
  '已重新连接串口': 'Serial port reconnected',
  '自动重连失败，请手动连接': 'Auto-reconnect failed - please connect manually',
  '串口自动重连失败，请手动连接': 'Serial auto-reconnect failed - please connect manually',
  '确认设备模式 ...': 'Confirming device mode ...',
  '设备在 APP 模式 — 发送进入下载模式 (cmd 50) ...': 'Device is in APP mode - sending enter-download-mode (cmd 50) ...',
  '寻找 IAP 端口 (TOWMCUIAP) ...': 'Searching for the IAP port (TOWMCUIAP) ...',
  '进入下载模式失败 — 无法打开端口': 'Failed to enter download mode - cannot open the port',
  '未找到 IAP 端口 — 请检查设备': 'IAP port not found - check the device',
  '未找到 IAP 端口 — 设备未进入下载模式': 'IAP port not found - the device did not enter download mode',
  '查询版本失败 — 设备无响应': 'Version query failed - the device is not responding',
  '发送进入下载模式 (cmd 50) ...': 'Sending enter-download-mode (cmd 50) ...',
  '无法查询版本': 'Cannot query the version',
  '设备无响应': 'the device is not responding',
  '读取异常': 'Read error',
  '写入失败': 'Write failed',
  '空数据': 'empty data',
};

const RULES: Array<[RegExp, (...m: string[]) => string]> = [
  [/^固件大小: (\d+) 字节 \(([\d.]+) KB\)$/, (_, n, kb) => `Firmware size: ${n} bytes (${kb} KB)`],
  [/^固件大小: (\d+) 字节$/, (_, n) => `Firmware size: ${n} bytes`],
  [/^打开 (.+) @ (\d+) \.\.\.$/, (_, port, baud) => `Opening ${port} @ ${baud} ...`],
  [/^已连接 @ (\d+)$/, (_, baud) => `Connected @ ${baud}`],
  [/^已重连 @ (\d+)$/, (_, baud) => `Reconnected @ ${baud}`],
  [/^保持当前波特率 (\d+) \(不切换\)$/, (_, baud) => `Keeping baudrate ${baud} (no switch)`],
  [/^切换波特率 (\d+) -> (\d+) \.\.\.$/, (_, a, b) => `Switching baudrate ${a} -> ${b} ...`],
  [/^擦除失败 \(错误码: 0x([0-9A-Fa-f]+)\)$/, (_, code) => `Erase failed (error code: 0x${code})`],
  [/^开始编程 \((\d+) 块, 单块 (\d+) ms 超时\) \.\.\.$/, (_, n, ms) => `Programming started (${n} blocks, ${ms} ms timeout per block) ...`],
  [/^编程 (\d+)\/(\d+)$/, (_, a, b) => `Programming ${a}/${b}`],
  [/^编程块 (\d+)\/(\d+) 触发重试: 第 (\d+)\/(\d+) 次成功, 最坏 (\d+)ms, 块耗时 (\d+)ms$/,
    (_, a, b, c, d, worst, ms) => `Program block ${a}/${b} retried: succeeded on attempt ${c}/${d}, worst ${worst} ms, block ${ms} ms`],
  [/^编程块 (\d+)\/(\d+) 全部超时: (\d+) 次尝试, 最坏 (\d+)ms, 残留 (\d+) 字节:$/,
    (_, a, b, tries, worst, left) => `Program block ${a}/${b} timed out: ${tries} attempts, worst ${worst} ms, ${left} bytes left:`],
  [/^编程块 (\d+)\/(\d+) 失败 \(0x([0-9A-Fa-f]+)\)$/, (_, a, b, code) => `Program block ${a}/${b} failed (0x${code})`],
  [/^校验 (\d+)\/(\d+)$/, (_, a, b) => `Verifying ${a}/${b}`],
  [/^校验块 (\d+)\/(\d+) 触发重试: 第 (\d+)\/(\d+) 次成功, 最坏 (\d+)ms, 块耗时 (\d+)ms$/,
    (_, a, b, c, d, worst, ms) => `Verify block ${a}/${b} retried: succeeded on attempt ${c}/${d}, worst ${worst} ms, block ${ms} ms`],
  [/^校验块 (\d+)\/(\d+) 全部超时: (\d+) 次尝试, 最坏 (\d+)ms, 残留 (\d+) 字节:$/,
    (_, a, b, tries, worst, left) => `Verify block ${a}/${b} timed out: ${tries} attempts, worst ${worst} ms, ${left} bytes left:`],
  [/^校验块 (\d+)\/(\d+) 失败 — 固件不匹配$/, (_, a, b) => `Verify block ${a}/${b} failed - firmware mismatch`],
  [/^擦除超时: 尝试 (\d+)\/(\d+) 次 \(单次上限 (\d+)ms, 最坏 (\d+)ms\), 残留 (\d+) 字节:$/,
    (_, a, b, lim, worst, left) => `Erase timed out: ${a}/${b} attempts (limit ${lim} ms each, worst ${worst} ms), ${left} bytes left:`],
  [/^擦除成功 \((\d+)ms, 尝试 (\d+) 次, 最坏单次 (\d+)ms\)$/,
    (_, ms, tries, worst) => `Erase OK (${ms} ms, ${tries} attempts, worst ${worst} ms)`],
  [/^编程完成: 总耗时 ([\d.]+)s \(([\d.]+) KB\/s\), 单块最坏 (\d+)ms \/ 平均 (\d+)ms, 重试块 (\d+)\/(\d+), 重试次数 (\d+) \(最后: 块 (\d+) 第 (\d+) 次 (\d+)ms\)$/,
    (_, s, kbs, worst, avg, rb, tot, retries, blk, nth, ms) =>
      `Programming done: ${s}s total (${kbs} KB/s), worst block ${worst} ms / avg ${avg} ms, retried blocks ${rb}/${tot}, retries ${retries} (last: block ${blk} attempt ${nth} ${ms} ms)`],
  [/^校验通过: 总耗时 ([\d.]+)s, 单块最坏 (\d+)ms \/ 平均 (\d+)ms, 重试块 (\d+)\/(\d+), 重试次数 (\d+) \(最后: 块 (\d+) 第 (\d+) 次 (\d+)ms\)$/,
    (_, s, worst, avg, rb, tot, retries, blk, nth, ms) =>
      `Verification passed: ${s}s total, worst block ${worst} ms / avg ${avg} ms, retried blocks ${rb}/${tot}, retries ${retries} (last: block ${blk} attempt ${nth} ${ms} ms)`],
  [/^编程完成: 总耗时 ([\d.]+)s \(([\d.]+) KB\/s\)$/, (_, s, kbs) => `Programming done: ${s}s total (${kbs} KB/s)`],
  [/^校验通过: 总耗时 ([\d.]+)s$/, (_, s) => `Verification passed: ${s}s total`],
  [/^(.+) \.\.\.\(共(\d+)字节\)$/, (_, head, n) => `${translateLoose(head)} ... (${n} bytes total)`],
  [/^  ↳ 残留 (\d+) 字节:(.*)$/, (_, n, rest) => `  ↳ ${n} bytes left:${rest}`],
];

/** 长尾日志的词级兜底：保证英文模式不残留中文（语义近似，够用即可）。 */
const WORDS: Array<[RegExp, string]> = [
  [/固件升级完成/g, 'Firmware upgrade complete'],
  [/升级正在进行中/g, 'Upgrade in progress'],
  [/固件大小/g, 'Firmware size'],
  [/字节/g, 'bytes'],
  [/固件数据/g, 'firmware data'],
  [/固件/g, 'firmware'],
  [/编程块/g, 'program block'],
  [/校验块/g, 'verify block'],
  [/编程/g, 'programming'],
  [/校验/g, 'verify'],
  [/擦除/g, 'erase'],
  [/成功/g, 'OK'],
  [/失败/g, 'failed'],
  [/超时/g, 'timed out'],
  [/尝试/g, 'attempts'],
  [/最坏/g, 'worst'],
  [/平均/g, 'avg'],
  [/单块/g, 'per block'],
  [/单次/g, 'per attempt'],
  [/总耗时/g, 'total elapsed'],
  [/耗时/g, 'elapsed'],
  [/残留/g, 'leftover'],
  [/波特率/g, 'baudrate'],
  [/切换/g, 'switch'],
  [/设备/g, 'device'],
  [/串口/g, 'serial port'],
  [/端口/g, 'port'],
  [/等待/g, 'waiting for'],
  [/重新连接/g, 'reconnect'],
  [/自动重连/g, 'auto-reconnect'],
  [/请手动连接/g, 'please connect manually'],
  [/已连接/g, 'connected'],
  [/已重连/g, 'reconnected'],
  [/正在/g, 'in progress'],
  [/读取/g, 'read'],
  [/文件/g, 'file'],
  [/为空/g, 'is empty'],
  [/发送/g, 'send'],
  [/命令/g, 'command'],
  [/进入下载模式/g, 'enter download mode'],
  [/下载模式/g, 'download mode'],
  [/寻找/g, 'searching for'],
  [/未找到/g, 'not found'],
  [/请检查设备/g, 'please check the device'],
  [/探测/g, 'probing'],
  [/确认/g, 'confirming'],
  [/模式/g, 'mode'],
  [/未指定/g, 'not specified'],
  [/未选择/g, 'not selected'],
  [/未收到/g, 'not received'],
  [/无法/g, 'cannot'],
  [/无响应/g, 'not responding'],
  [/未知/g, 'unknown'],
  [/错误码/g, 'error code'],
  [/重试块/g, 'retried blocks'],
  [/重试次数/g, 'retry count'],
];

function translateLoose(text: string): string {
  let out = text;
  for (const [re, rep] of WORDS) out = out.replace(re, rep);
  return out.replace(/\s{2,}/g, ' ').trim();
}

/** 把一条驱动文案翻成当前语言（中文原样返回）。 */
export function localizeDriverText(text: string): string {
  if (typeof text !== 'string' || !text) return text;
  if (getMainLang() !== 'en') return text;
  if (EXACT[text]) return EXACT[text];
  for (const [re, fn] of RULES) {
    const m = text.match(re);
    if (m) return fn(...m);
  }
  if (!/[\u3400-\u9fff]/.test(text)) return text;
  return translateLoose(text);
}

const MESSAGE_EVENTS = new Set(['iap_log', 'iap2_log', 'serial_error', 'kmnet_error']);
const ERROR_EVENTS = new Set(['send_result', 'iap_done', 'iap2_done', 'firmware:download_done']);
const STATUS_EVENTS = new Set(['iap_progress', 'iap2_progress']);

/** 按事件类型翻译驱动载荷里的展示字段；返回同一个对象（就地更新）。 */
export function localizeDriverEvent<T>(event: string, data: T): T {
  if (!data || typeof data !== 'object') return data;
  const obj = data as Record<string, unknown>;
  if (MESSAGE_EVENTS.has(event) && typeof obj.message === 'string') {
    obj.message = localizeDriverText(obj.message);
  }
  if (ERROR_EVENTS.has(event) && typeof obj.error === 'string') {
    obj.error = localizeDriverText(obj.error);
  }
  if (STATUS_EVENTS.has(event) && typeof obj.status === 'string') {
    obj.status = localizeDriverText(obj.status);
  }
  return data;
}
