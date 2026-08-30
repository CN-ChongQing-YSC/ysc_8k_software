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
  // 侧栏展开态：展开=单栏全量（每页带作用说明），收起=双栏图标栏。默认展开。
  const railExpanded = ref<boolean>(true);
  // 各分组最后访问的页面：点击分组图标直达该页，免去"先点组再点页"的二次点击。
  const lastViewByGroup = ref<Record<string, string>>({});

  function setView(v: string, group?: string): void {
    activeView.value = v;
    if (group) lastViewByGroup.value = { ...lastViewByGroup.value, [group]: v };
    try {
      localStorage.setItem('ysc_ui_current_view', v);
      localStorage.setItem('ysc_ui_last_view_by_group', JSON.stringify(lastViewByGroup.value));
    } catch {
      /* ignore */
    }
  }

  function loadView(): void {
    try {
      const v = localStorage.getItem('ysc_ui_current_view');
      if (v) activeView.value = v;
      const g = localStorage.getItem('ysc_ui_last_view_by_group');
      if (g) lastViewByGroup.value = JSON.parse(g) || {};
    } catch {
      /* ignore */
    }
  }

  function setRailExpanded(v: boolean): void {
    railExpanded.value = v;
    try {
      localStorage.setItem('ysc_ui_rail_expanded', v ? '1' : '0');
    } catch {
      /* ignore */
    }
  }

  function loadRailExpanded(): void {
    try {
      const v = localStorage.getItem('ysc_ui_rail_expanded');
      if (v !== null) railExpanded.value = v === '1';
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

  return {
    activeView,
    lang,
    toasts,
    railExpanded,
    lastViewByGroup,
    setView,
    loadView,
    setRailExpanded,
    loadRailExpanded,
    toast,
    dismissToast,
  };
});
