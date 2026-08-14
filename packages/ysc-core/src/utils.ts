/** 极简类型化事件发射器，供 transport / device 层复用。 */

export type Handler<T> = (payload: T) => void;

export class Emitter<E extends { type: string }> {
  private handlers = new Map<string, Set<Handler<any>>>();

  on<K extends E['type']>(type: K, handler: (e: Extract<E, { type: K }>) => void): () => void {
    let set = this.handlers.get(type);
    if (!set) {
      set = new Set();
      this.handlers.set(type, set);
    }
    set.add(handler);
    return () => {
      set!.delete(handler);
    };
  }

  /** 订阅所有事件类型。 */
  onAny(handler: Handler<E>): () => void {
    return this.on('__any__' as any, handler as any);
  }

  emit(e: E): void {
    const anySet = this.handlers.get('__any__' as any);
    if (anySet) for (const h of anySet) h(e);
    const set = this.handlers.get(e.type);
    if (set) for (const h of set) h(e);
  }

  clear(): void {
    this.handlers.clear();
  }
}

export function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

/** UTF-8 解码（复用单例）。 */
const _dec = new TextDecoder();
export function decodeUtf8(bytes: Uint8Array): string {
  return _dec.decode(bytes);
}

/** 安全 JSON 解析；失败返回 undefined。 */
export function tryParseJson<T = any>(s: string): T | undefined {
  try {
    return JSON.parse(s) as T;
  } catch {
    return undefined;
  }
}
