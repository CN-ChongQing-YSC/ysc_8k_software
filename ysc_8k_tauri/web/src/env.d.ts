/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue';
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>;
  export default component;
}

declare module '@ysc/ui/i18n/index.js' {
  export function initI18n(): void;
  export function useI18n(): {
    t: (key: string, params?: Record<string, unknown>) => string;
    lang: { value: 'zh' | 'en' };
    setLang: (lang: 'zh' | 'en') => void;
    toggleLang: () => void;
  };
}

