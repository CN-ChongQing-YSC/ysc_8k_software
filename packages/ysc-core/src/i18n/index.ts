/**
 * 国际化。迁自 ysc_8k_electron/src/renderer/i18n/index.js（已是纯 Vue reactive + localStorage）。
 * 平台无关，Electron 与网页版共用。
 */
import { reactive, computed } from 'vue';
import zh from './zh';
import en from './en';
import { docs as docsZh } from './docs/zh';
import { docs as docsEn } from './docs/en';

type Lang = 'zh' | 'en';

const messages: Record<Lang, any> = { zh, en };
const docsPacks: Record<Lang, any> = { zh: docsZh, en: docsEn };

const STORAGE_KEY = 'ysc8k_lang';

function detectInitialLang(): Lang {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === 'zh' || saved === 'en') return saved;
  } catch (e) {
    /* ignore */
  }
  return 'zh';
}

const state = reactive<{ lang: Lang }>({ lang: detectInitialLang() });

function resolve(obj: any, path: string): any {
  const parts = path.split('.');
  let cur = obj;
  for (let i = 0; i < parts.length; i++) {
    if (cur == null) return undefined;
    cur = cur[parts[i]];
  }
  return cur;
}

function format(template: any, params?: Record<string, any>): any {
  if (typeof template !== 'string') return template;
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, function (_, key) {
    return params.hasOwnProperty(key) ? String(params[key]) : '{' + key + '}';
  });
}

function tInternal(path: string, params?: Record<string, any>): string {
  const dict = messages[state.lang] || messages.zh;
  let val = resolve(dict, path);
  if (val === undefined) {
    val = resolve(messages.zh, path);
  }
  if (val === undefined) return path;
  return format(val, params);
}

export function setLang(lang: Lang): void {
  if (lang !== 'zh' && lang !== 'en') return;
  state.lang = lang;
  try {
    localStorage.setItem(STORAGE_KEY, lang);
  } catch (e) {
    /* ignore */
  }
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('lang', lang);
  }
}

export function getLang(): Lang {
  return state.lang;
}

export function useI18n() {
  const t = function (path: string, params?: Record<string, any>): string {
    return tInternal(path, params);
  };
  return {
    t,
    lang: computed<Lang>(() => state.lang),
    setLang,
    getLang,
    docs: computed(() => docsPacks[state.lang] || docsPacks.zh),
  };
}

export function initI18n(): void {
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('lang', state.lang);
  }
}
