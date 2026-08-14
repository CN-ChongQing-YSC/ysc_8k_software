/**
 * 帧构造器。逐字节复刻：
 *   - JSON 控制帧 —— serial_port.cpp:300 SendJsonCommand
 *   - IAP 二进制帧 —— iap_upgrader.cpp:72 BuildIAPFrame（v1 与 v2 towmcu 同形）
 *
 * JSON 帧布局： <START>(7) | total_len BE u16(2) | utf8 json | <END>(5)
 *   total_len = HEADER_LEN(9) + jsonLen + END_MARKER_LEN(5) = 14 + jsonLen
 *
 * IAP 帧布局：  <START>(7) | total_len BE u16(2) | payload | <END>(5)
 *   payload = cmd(1) + dataLen u8(1) + [4 零字节 rev（仅 ERASE/VERIFY）] + data + cksum LE u16(2)
 *   total_len = 14 + payloadLen
 */
import {
  START_BYTES,
  END_BYTES,
  START_MARKER_LEN,
  END_MARKER_LEN,
  HEADER_LEN,
} from './constants';
import { IAP_CMD_ERASE, IAP_CMD_VERIFY } from './commands';
import { iapCksum } from './checksum';

const te = new TextEncoder();

/**
 * 构造 JSON 控制帧。
 * @param json 已序列化的 JSON 字符串（如 '{"cmd":132}'）。
 */
export function buildJsonFrame(json: string): Uint8Array {
  const jsonBytes = te.encode(json);
  const jsonLen = jsonBytes.length;
  const packetLen = HEADER_LEN + jsonLen + END_MARKER_LEN; // = 14 + jsonLen
  const out = new Uint8Array(START_MARKER_LEN + 2 + jsonLen + END_MARKER_LEN);
  let pos = 0;
  out.set(START_BYTES, pos);
  pos += START_MARKER_LEN;
  out[pos++] = (packetLen >>> 8) & 0xff;
  out[pos++] = packetLen & 0xff;
  out.set(jsonBytes, pos);
  pos += jsonLen;
  out.set(END_BYTES, pos);
  return out;
}

/**
 * 构造 IAP 二进制帧。
 * @param cmd IAP_CMD_*（0x80..0x84）
 * @param data 分片数据（PROGRAM/VERIFY 为 ≤60 字节；ERASE/END 为空）
 */
export function buildIapFrame(cmd: number, data?: Uint8Array | null): Uint8Array {
  const dl = data ? data.length : 0;
  const hasRev = cmd === IAP_CMD_ERASE || cmd === IAP_CMD_VERIFY;
  const payloadLen = 1 /*cmd*/ + 1 /*len*/ + (hasRev ? 4 : 0) + dl + 2 /*cksum*/;
  const payload = new Uint8Array(payloadLen);
  let p = 0;
  payload[p++] = cmd & 0xff;
  payload[p++] = dl & 0xff;
  if (hasRev) {
    payload[p++] = 0;
    payload[p++] = 0;
    payload[p++] = 0;
    payload[p++] = 0;
  }
  if (dl > 0 && data) {
    payload.set(data, p);
    p += dl;
  }
  const cksum = iapCksum(payload.subarray(0, p));
  payload[p++] = cksum & 0xff;
  payload[p++] = (cksum >>> 8) & 0xff;

  const totalLen = START_MARKER_LEN + 2 + payloadLen + END_MARKER_LEN; // = 14 + payloadLen
  const out = new Uint8Array(totalLen);
  let o = 0;
  out.set(START_BYTES, o);
  o += START_MARKER_LEN;
  out[o++] = (totalLen >>> 8) & 0xff;
  out[o++] = totalLen & 0xff;
  out.set(payload, o);
  o += payloadLen;
  out.set(END_BYTES, o);
  return out;
}
