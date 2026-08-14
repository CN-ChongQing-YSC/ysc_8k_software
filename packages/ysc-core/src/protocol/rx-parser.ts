/**
 * 帧接收状态机。逐字节复刻 serial_port.cpp:525-575 的 RX FSM。
 *
 * 三态：RX_IDLE → RX_HEADER → RX_PAYLOAD → (回 RX_IDLE)
 *   - IDLE：逐字节匹配 <START>；部分匹配失败时，若当前字节是 '<' 则从位置 1 重启匹配
 *           （兼容 "<ST<START>" 这种重叠情形）。
 *   - HEADER：收满 2 字节 BE 总长；算出 payloadLen = total - 9 - 5，越界则回 IDLE。
 *   - PAYLOAD：收满 payloadLen + 5 字节；末 5 字节须为 <END>，命中则投递前 payloadLen 字节。
 *
 * 投递的 payload 可能是 JSON 控制帧，也可能是 IAP 响应帧——由上层按内容判别。
 */
import {
  START_BYTES,
  END_BYTES,
  START_MARKER_LEN,
  END_MARKER_LEN,
  HEADER_LEN,
  MAX_PAYLOAD,
} from './constants';

const RX_IDLE = 0;
const RX_HEADER = 1;
const RX_PAYLOAD = 2;

const LESS_THAN = 0x3c; // '<'

export class FrameParser {
  private state = RX_IDLE;
  private startMatchPos = 0;
  private hdrLen = 0;
  private len0 = 0;
  private len1 = 0;
  private payloadLen = 0;
  private payloadTarget = 0;
  private payloadPos = 0;
  private readonly buf: Uint8Array;

  /** 每收到一个完整帧时回调，参数为 payload（不含 <END>）。 */
  public onFrame: (payload: Uint8Array) => void;

  constructor(onFrame: (payload: Uint8Array) => void, maxPayload: number = MAX_PAYLOAD) {
    this.onFrame = onFrame;
    this.buf = new Uint8Array(maxPayload + END_MARKER_LEN);
  }

  /** 喂入一段串口字节流（可跨帧、可分多次喂）。 */
  feed(bytes: Uint8Array): void {
    for (let i = 0; i < bytes.length; i++) {
      const b = bytes[i];
      switch (this.state) {
        case RX_IDLE: {
          if (b === START_BYTES[this.startMatchPos]) {
            this.startMatchPos++;
            if (this.startMatchPos === START_MARKER_LEN) {
              this.startMatchPos = 0;
              this.state = RX_HEADER;
              this.hdrLen = 0;
            }
          } else if (this.startMatchPos > 0) {
            // 部分匹配中断：若当前字节是 '<'，可能是一个新 <START> 的起点
            this.startMatchPos = b === LESS_THAN ? 1 : 0;
          }
          break;
        }
        case RX_HEADER: {
          if (this.hdrLen === 0) this.len0 = b;
          else this.len1 = b;
          this.hdrLen++;
          if (this.hdrLen === 2) {
            const expectedLen = (this.len0 << 8) | this.len1;
            this.payloadLen = expectedLen - HEADER_LEN - END_MARKER_LEN;
            if (this.payloadLen < 0 || this.payloadLen > this.buf.length - END_MARKER_LEN) {
              this.state = RX_IDLE;
            } else {
              this.payloadPos = 0;
              this.payloadTarget = this.payloadLen + END_MARKER_LEN;
              this.state = RX_PAYLOAD;
            }
          }
          break;
        }
        case RX_PAYLOAD: {
          if (this.payloadPos < this.buf.length) {
            this.buf[this.payloadPos++] = b;
          } else {
            this.state = RX_IDLE;
            this.payloadPos = 0;
            break;
          }
          if (this.payloadPos === this.payloadTarget) {
            let ok = true;
            for (let k = 0; k < END_MARKER_LEN; k++) {
              if (this.buf[this.payloadLen + k] !== END_BYTES[k]) {
                ok = false;
                break;
              }
            }
            if (ok && this.payloadLen > 0) {
              // 投递 payload 副本（避免后续覆盖）
              this.onFrame(this.buf.slice(0, this.payloadLen));
            }
            this.state = RX_IDLE;
            this.payloadPos = 0;
          }
          break;
        }
      }
    }
  }

  /** 复位状态机（断开/重连时调用）。 */
  reset(): void {
    this.state = RX_IDLE;
    this.startMatchPos = 0;
    this.hdrLen = 0;
    this.payloadPos = 0;
  }
}
