<template>
  <header class="topbar">
    <div class="brand">
      <img :src="brandLogo" class="brand-mark" alt="YSC" />
      <span class="brand-title">YSC 8K</span>
      <span class="brand-sub">· {{ platformLabel }}</span>
    </div>

    <div class="conn" :class="dev.connected ? 'on' : 'off'" :title="dev.connected ? `${dev.port} @ ${dev.baud}` : t('topbar.notConnected')">
      <span class="dot" />
      <span class="conn-text">{{ dev.connected ? `${dev.port} @ ${dev.baud}` : t('topbar.notConnected') }}</span>
      <span v-if="dev.connected && dev.version" class="conn-ver">{{ dev.version.clean }}</span>
    </div>

    <div class="actions">
      <button class="lang" :title="t('app.langTooltip')" @click="toggleLang">
        {{ lang === 'zh' ? '中' : 'EN' }}
      </button>
      <slot name="actions" />
    </div>
  </header>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useDeviceStore } from '../store/device-store';
import { useUiStore } from '../store/ui-store';
import { useI18n } from '../i18n';
import { getPlatform } from '../platform';
import brandLogo from './assets/brand-white.png';

const dev = useDeviceStore();
const ui = useUiStore();
const { t, lang, setLang } = useI18n();

const platformLabel = computed(() => (getPlatform() === 'web' ? t('topbar.web') : t('topbar.desktop')));

function toggleLang(): void {
  setLang(lang.value === 'zh' ? 'en' : 'zh');
  // 同步界面状态（其它组件/持久化使用 ui.lang）
  ui.lang = lang.value;
}

// 初始语言以 i18n 的持久化状态为准，避免与 ui.lang 默认值不一致
ui.lang = lang.value;
</script>

<style scoped>
.topbar {
  height: 44px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 14px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.brand {
  display: flex;
  align-items: center;
  gap: 7px;
}
.brand-mark {
  height: 32px;
  width: auto;
  display: block;
  opacity: 0.95;
}
.brand-title {
  font-weight: 700;
  letter-spacing: 0.5px;
}
.brand-sub {
  color: var(--text-muted);
  font-size: 12px;
}
.conn {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 4px 11px;
  border-radius: 999px;
  font-size: 12px;
  font-family: var(--font-mono);
}
.conn .dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.conn.on {
  background: var(--pill-on-bg);
  color: var(--pill-on-fg);
}
.conn.on .dot {
  background: var(--accent-green);
  box-shadow: 0 0 6px var(--accent-green);
}
.conn.off {
  background: var(--pill-off-bg);
  color: var(--pill-off-fg);
}
.conn.off .dot {
  background: var(--text-muted);
}
.conn-ver {
  opacity: 0.7;
}
.actions {
  display: flex;
  align-items: center;
  gap: 8px;
}
.lang {
  background: var(--btn-bg);
  color: var(--btn-text);
  border: 1px solid var(--border);
  border-radius: var(--btn-radius);
  width: 38px;
  height: 30px;
  font-size: 12px;
  font-weight: 700;
  transition: var(--transition-fast);
}
.lang:hover {
  background: var(--btn-bg-hover);
  border-color: var(--border-strong);
  color: var(--text-primary);
}
</style>
