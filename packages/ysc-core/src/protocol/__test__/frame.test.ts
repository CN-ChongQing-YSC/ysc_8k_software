/**
 * 协议层离线测试（无测试框架，纯断言）。
 * 运行：见 packages/ysc-core 脚本，或 `tsc` 编译为 CJS 后 `node` 执行。
 *
 * 验证：
 *   1. JSON 帧往返：buildJsonFrame → FrameParser → payload 还原
 *   2. 多帧拼接 / 分片喂入 / 帧间垃圾字节重同步
 *   3. IAP 帧结构：ERASE 带 4 字节 rev；PROGRAM 60B 分片长度与 cksum 位置
 *   4. 与 C++ 公式手动交叉核对（totalLen = 14 + payloadLen；cksum = LE 求和）
 */
import { buildJsonFrame, buildIapFrame } from '../frame';
import { FrameParser } from '../rx-parser';
import { iapCksum } from '../checksum';
import { START_BYTES, END_BYTES, CHUNK_SZ } from '../constants';
import { IAP_CMD_ERASE, IAP_CMD_PROM } from '../commands';

let failures = 0;
function check(name: string, cond: boolean, extra = ''): void {
  if (cond) {
    console.log(`  ✓ ${name}`);
  } else {
    console.error(`  ✗ ${name} ${extra}`);
    failures++;
  }
}

function hex(u: Uint8Array): string {
  return Array.from(u)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join(' ');
}

/** 收集 parser 解出的所有 payload。 */
function collect(): { parser: FrameParser; frames: Uint8Array[] } {
  const frames: Uint8Array[] = [];
  const parser = new FrameParser((p) => frames.push(p));
  return { parser, frames };
}

// ── 1. JSON 帧往返 ──
console.log('\n[1] JSON 帧往返');
{
  const json = '{"cmd":132}';
  const frame = buildJsonFrame(json);
  check('帧以 <START> 开头', Array.prototype.slice.call(frame, 0, 7).join() === Array.from(START_BYTES).join());
  check('帧以 <END> 结尾', Array.prototype.slice.call(frame, frame.length - 5).join() === Array.from(END_BYTES).join());
  const totalLen = (frame[7] << 8) | frame[8];
  check('totalLen = 14 + jsonLen', totalLen === 14 + json.length, `(got ${totalLen})`);

  const { parser, frames } = collect();
  parser.feed(frame);
  check('解出 1 帧', frames.length === 1, `(got ${frames.length})`);
  check('payload 还原 JSON', new TextDecoder().decode(frames[0]) === json);
}

// ── 2. 多帧拼接 + 帧间垃圾字节 ──
console.log('\n[2] 多帧 / 分片 / 垃圾字节重同步');
{
  const f1 = buildJsonFrame('{"cmd":45,"kc":4,"down":1}');
  const f2 = buildJsonFrame('{"cmd":132}');
  const garbage = new TextEncoder().encode('garbage<ST'); // 含半截 <ST，须正确重同步
  const stream = new Uint8Array(f1.length + garbage.length + f2.length);
  stream.set(f1, 0);
  stream.set(garbage, f1.length);
  stream.set(f2, f1.length + garbage.length);

  const { parser, frames } = collect();
  // 分片喂入：每次 5 字节
  for (let i = 0; i < stream.length; i += 5) {
    parser.feed(stream.subarray(i, Math.min(i + 5, stream.length)));
  }
  check('垃圾字节中解出 2 帧', frames.length === 2, `(got ${frames.length})`);
  check('第1帧 JSON 还原', new TextDecoder().decode(frames[0]) === '{"cmd":45,"kc":4,"down":1}');
  check('第2帧 JSON 还原', new TextDecoder().decode(frames[1]) === '{"cmd":132}');
}

// ── 3. 空 payload 帧（payloadLen=0）应被丢弃 ──
console.log('\n[3] 空 payload 帧丢弃');
{
  // 手工构造一个 payloadLen=0 的帧：<START> + totalLen=14(=9+0+5) + <END>
  const empty = new Uint8Array(7 + 2 + 5);
  empty.set(START_BYTES, 0);
  empty[7] = 0;
  empty[8] = 14;
  empty.set(END_BYTES, 9);
  const { parser, frames } = collect();
  parser.feed(empty);
  check('空 payload 不投递', frames.length === 0, `(got ${frames.length})`);
}

// ── 4. IAP 帧：ERASE 带 4 字节 rev ──
console.log('\n[4] IAP ERASE 帧结构');
{
  const frame = buildIapFrame(IAP_CMD_ERASE);
  // payload 偏移：[0]=cmd 0x81, [1]=dataLen 0, [2..5]=rev 0, [6..7]=cksum
  check('cmd = 0x81', frame[9] === 0x81, `(got ${frame[9].toString(16)})`);
  check('dataLen = 0', frame[10] === 0);
  check('rev 字节全 0', frame[11] === 0 && frame[12] === 0 && frame[13] === 0 && frame[14] === 0);
  const expectedTotal = 14 + 8; // payload = 1+1+4+0+2 = 8
  const totalLen = (frame[7] << 8) | frame[8];
  check('totalLen = 22', totalLen === expectedTotal, `(got ${totalLen})`);
  // cksum = cmd+len+rev 求和 LE
  const expectedCksum = iapCksum(frame.subarray(9, 15)); // [9..14] = cmd+len+rev(4)
  const gotCksum = frame[15] | (frame[16] << 8);
  check('cksum LE 正确', gotCksum === expectedCksum, `(got ${gotCksum} exp ${expectedCksum})`);
}

// ── 5. IAP 帧：PROGRAM 60B 分片 ──
console.log('\n[5] IAP PROGRAM 60B 分片');
{
  const chunk = new Uint8Array(CHUNK_SZ);
  for (let i = 0; i < CHUNK_SZ; i++) chunk[i] = (i * 7 + 3) & 0xff;
  const frame = buildIapFrame(IAP_CMD_PROM, chunk);
  check('cmd = 0x80', frame[9] === 0x80);
  check('dataLen = 60', frame[10] === 60);
  check('数据段匹配', hex(frame.subarray(11, 11 + CHUNK_SZ)) === hex(chunk));
  // payload = 1+1+0+60+2 = 64
  const expectedTotal = 14 + 64;
  const totalLen = (frame[7] << 8) | frame[8];
  check('totalLen = 78', totalLen === expectedTotal, `(got ${totalLen})`);
  // cksum 覆盖 cmd+len+data
  const expectedCksum = iapCksum(frame.subarray(9, 9 + 62)); // [9..70] = cmd+len+data(60)
  const gotCksum = frame[71] | (frame[72] << 8);
  check('cksum LE 正确', gotCksum === expectedCksum);
}

// ── 6. IAP 帧也能被 FrameParser 解出（同构外层信封） ──
console.log('\n[6] FrameParser 解 IAP 帧');
{
  const frame = buildIapFrame(IAP_CMD_PROM, new Uint8Array(60).fill(0xab));
  const { parser, frames } = collect();
  parser.feed(frame);
  check('解出 1 个 IAP payload', frames.length === 1, `(got ${frames.length})`);
  check('payload 长度 = 64', frames[0].length === 64, `(got ${frames[0].length})`);
  check('payload[0] = 0x80', frames[0][0] === 0x80);
}

console.log('');
if (failures === 0) {
  console.log('✅ 协议层测试全部通过');
} else {
  console.error(`❌ ${failures} 项失败`);
  throw new Error(`${failures} protocol test failure(s)`);
}
