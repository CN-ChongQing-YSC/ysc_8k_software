import { reactive, computed } from 'vue';
import zh from './zh.js';
import en from './en.js';
import { docs as docsZh } from './docs/zh.js';
import { docs as docsEn } from './docs/en.js';

const messages = { zh, en };
const docsPacks = { zh: docsZh, en: docsEn };

const STORAGE_KEY = 'ysc8k_lang';

function detectInitialLang() {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === 'zh' || saved === 'en') return saved;
  } catch (e) {}
  return 'zh';
}

const state = reactive({
  lang: detectInitialLang(),
});

function resolve(obj, path) {
  const parts = path.split('.');
  let cur = obj;
  for (let i = 0; i < parts.length; i++) {
    if (cur == null) return undefined;
    cur = cur[parts[i]];
  }
  return cur;
}

function format(template, params) {
  if (typeof template !== 'string') return template;
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, function(_, key) {
    return params.hasOwnProperty(key) ? String(params[key]) : '{' + key + '}';
  });
}

function tInternal(path, params) {
  const dict = messages[state.lang] || messages.zh;
  let val = resolve(dict, path);
  if (val === undefined) {
    val = resolve(messages.zh, path);
  }
  if (val === undefined) return path;
  return format(val, params);
}

export function setLang(lang) {
  if (lang !== 'zh' && lang !== 'en') return;
  state.lang = lang;
  try { localStorage.setItem(STORAGE_KEY, lang); } catch (e) {}
  document.documentElement.setAttribute('lang', lang);
}

export function getLang() {
  return state.lang;
}

export function useI18n() {
  const t = function(path, params) {
    return tInternal(path, params);
  };
  return {
    t,
    lang: computed(() => state.lang),
    setLang,
    getLang,
    docs: computed(() => docsPacks[state.lang] || docsPacks.zh),
  };
}

export function initI18n() {
  document.documentElement.setAttribute('lang', state.lang);
}
