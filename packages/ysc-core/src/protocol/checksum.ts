/**
 * IAP 帧校验和：payload 各字节求和，低 16 位小端。
 * 对应 iap_upgrader.cpp:91 `uint16_t cksum += payload[i]`（自然回绕）。
 *
 * @param payload 已构造的 payload（cmd+len+rev+data），不含校验和本身。
 * @returns 0..65535 的校验和值（写入帧时按 LE 拆两字节）。
 */
export function iapCksum(payload: Uint8Array): number {
  let s = 0;
  for (let i = 0; i < payload.length; i++) {
    s = (s + payload[i]) & 0xffff;
  }
  return s;
}
