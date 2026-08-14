/**
 * useUiStore —— 界面状态（当前视图/语言/通知）。
 * activeView 由 nav-config.ts 驱动（Phase 4 起用）；MVP 网页版用简易视图键。
 */
import { defineStore } from 'pinia';
import { ref } from 'vue';

export interface Toast {
  id: number;
  message: string;
  kind: 'info' | 'success' | 'error' | 'warn';
}

let toastId = 1;

export const useUiStore = defineStore('ysc-ui', () => {
  const activeView = ref<string>('device');
  const lang = ref<'zh' | 'en'>('zh');
  const toasts = ref<Toast[]>([]);

  function setView(v: string): void {
    activeView.value = v;
    try {
      localStorage.setItem('ysc_ui_current_view', v);
    } catch {
      /* ignore */
    }
  }

  function loadView(): void {
    try {
      const v = localStorage.getItem('ysc_ui_current_view');
      if (v) activeView.value = v;
    } catch {
      /* ignore */
    }
  }

  function toast(message: string, kind: Toast['kind'] = 'info', ttl = 3500): void {
    const id = toastId++;
    toasts.value.push({ id, message, kind });
    setTimeout(() => {
      toasts.value = toasts.value.filter((t) => t.id !== id);
    }, ttl);
  }

  function dismissToast(id: number): void {
    toasts.value = toasts.value.filter((t) => t.id !== id);
  }

  return { activeView, lang, toasts, setView, loadView, toast, dismissToast };
});
