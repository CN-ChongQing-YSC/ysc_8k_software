<template>
  <div class="titlebar">
    <img :src="brandLogo" class="titlebar-logo" alt="YSC" />
    <span class="titlebar-text">{{ t('app.title') }}</span>
    <div class="titlebar-spacer" />
    <button class="titlebar-btn lang-btn" @click="toggleLang" :title="t('app.langTooltip')">
      {{ t('app.lang') }}
    </button>
    <button class="titlebar-btn" @click="minimize" :title="t('titlebar.minimize')">
      <svg viewBox="0 0 12 12" width="12" height="12"><line x1="2" y1="6" x2="10" y2="6" stroke="currentColor" stroke-width="1.2" /></svg>
    </button>
    <button class="titlebar-btn" @click="maximize" :title="t('titlebar.maximize')">
      <svg viewBox="0 0 12 12" width="12" height="12"><rect x="2" y="2" width="8" height="8" rx="1" stroke="currentColor" stroke-width="1.2" fill="none" /></svg>
    </button>
    <button class="titlebar-btn close" @click="quitApp" :title="t('titlebar.close')">
      <svg viewBox="0 0 12 12" width="12" height="12"><line x1="3" y1="3" x2="9" y2="9" stroke="currentColor" stroke-width="1.2" /><line x1="9" y1="3" x2="3" y2="9" stroke="currentColor" stroke-width="1.2" /></svg>
    </button>
  </div>
</template>

<script setup>
import { useI18n } from '../i18n/index.js';
import brandLogo from '@ysc/core/ui/assets/brand-white.png';

const { t, lang, setLang } = useI18n();

function toggleLang() {
  setLang(lang.value === 'zh' ? 'en' : 'zh');
}

function minimize() {
  window.driverApi.send('window_minimize');
}

function maximize() {
  window.driverApi.send('window_maximize');
}

function quitApp() {
  window.driverApi.send('app_quit');
}
</script>

<style scoped>
/* 覆盖 global.css 里旧的 16px 闪电 SVG 尺寸：YSC logo 图像 */
.titlebar-logo {
  width: 26px;
  height: 26px;
  margin-left: 12px;
  margin-right: 2px;
  display: block;
  opacity: 0.95;
}
.lang-btn {
  font-size: 11px;
  font-weight: 700;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  padding: 0 8px;
  min-width: 32px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--bg-secondary);
  margin-right: 4px;
  transition: all 0.15s;
}
.lang-btn:hover {
  color: var(--accent-selected);
  border-color: var(--accent-selected);
}
</style>
