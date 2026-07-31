<template>
  <nav class="sidebar">
    <div class="sidebar-top">
      <button
        class="sidebar-btn"
        :class="{ active: current === 'home' }"
        :title="t('sidebar.home')"
        @click="$emit('navigate', 'home')"
      >
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="10" cy="10" r="2.5" />
          <path d="M10 2.5l1.5 3.2 3.5.5-2.5 2.5.6 3.5L10 10.7 6.9 12.2l.6-3.5L5 6.2l3.5-.5z" />
          <circle cx="10" cy="10" r="8" />
        </svg>
      </button>
      <button
        class="sidebar-btn"
        :class="{ active: current === 'docs' }"
        :title="t('sidebar.docs')"
        @click="$emit('navigate', 'docs')"
      >
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M6 3h8a2 2 0 012 2v10a2 2 0 01-2 2H6" />
          <path d="M6 3a2 2 0 00-2 2v10a2 2 0 002 2" />
          <path d="M6 3v14" />
          <line x1="10" y1="8" x2="14" y2="8" />
          <line x1="10" y1="11" x2="14" y2="11" />
        </svg>
      </button>
      <button
        class="sidebar-btn"
        :class="{ active: current === 'macro' }"
        :title="t('sidebar.macro')"
        @click="$emit('navigate', 'macro')"
      >
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <rect x="2" y="5" width="16" height="10" rx="2" />
          <line x1="5" y1="8" x2="7" y2="8" />
          <line x1="9" y1="8" x2="11" y2="8" />
          <line x1="13" y1="8" x2="15" y2="8" />
          <line x1="5" y1="12" x2="15" y2="12" />
        </svg>
      </button>
      <button
        class="sidebar-btn"
        :class="{ active: current === 'jitter' }"
        :title="t('sidebar.jitterMacro')"
        @click="$emit('navigate', 'jitter')"
      >
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M2 10 L5 10 L6.5 5 L9.5 15 L11 10 L14 10" />
          <line x1="16" y1="10" x2="18" y2="10" />
        </svg>
      </button>
      <button
        class="sidebar-btn"
        :class="{ active: current === 'mouse-curve' }"
        :title="t('sidebar.mouseCurve')"
        @click="$emit('navigate', 'mouse-curve')"
      >
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M2 15 C5 15 6 5 10 5 C14 5 15 15 18 15" />
        </svg>
      </button>
      <button
        class="sidebar-btn"
        :class="{ active: current === 'gamepad' }"
        :title="t('sidebar.gamepad')"
        @click="$emit('navigate', 'gamepad')"
      >
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <rect x="2" y="7" width="16" height="9" rx="4" />
          <circle cx="6" cy="11.5" r="1.2" />
          <circle cx="14" cy="11.5" r="1.2" />
          <line x1="9" y1="11.5" x2="11" y2="11.5" />
        </svg>
      </button>
      <button
        class="sidebar-btn"
        :class="{ active: current === 'firmware' }"
        :title="t('sidebar.firmware')"
        @click="$emit('navigate', 'firmware')"
      >
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <rect x="4" y="2" width="12" height="16" rx="2" />
          <line x1="7" y1="6" x2="13" y2="6" />
          <line x1="7" y1="9" x2="13" y2="9" />
          <circle cx="10" cy="14" r="1.5" />
        </svg>
      </button>
      <button
        class="sidebar-btn"
        :class="{ active: current === 'towmcu-firmware' }"
        :title="t('sidebar.towmcuFirmware')"
        @click="$emit('navigate', 'towmcu-firmware')"
      >
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <rect x="2" y="3" width="7" height="14" rx="1.5" />
          <rect x="11" y="3" width="7" height="14" rx="1.5" />
          <line x1="4" y1="7" x2="7" y2="7" />
          <line x1="4" y1="10" x2="7" y2="10" />
          <line x1="13" y1="7" x2="16" y2="7" />
          <line x1="13" y1="10" x2="16" y2="10" />
        </svg>
      </button>
      <button
        class="sidebar-btn"
        :class="{ active: current === 'debug' }"
        :title="t('sidebar.debug')"
        @click="$emit('navigate', 'debug')"
      >
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="9" cy="9" r="5.5" />
          <line x1="13" y1="13" x2="17" y2="17" />
          <line x1="6.5" y1="9" x2="11.5" y2="9" />
        </svg>
      </button>
    </div>
    <div class="sidebar-bottom">
      <div class="sidebar-logo">
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M11 1L4 12h5l-1 7 7-11h-5l1-7z" />
        </svg>
      </div>
    </div>
  </nav>
</template>

<script setup>
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

defineProps({
  current: { type: String, default: 'home' },
});

defineEmits(['navigate']);
</script>
