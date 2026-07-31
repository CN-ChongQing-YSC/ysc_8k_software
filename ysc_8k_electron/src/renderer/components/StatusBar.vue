<template>
  <div class="status-bar">
    <div class="status-group">
      <div class="status-dot" :class="{ on: serialConnected }" />
      <span class="status-label">{{ t('status.serial') }}</span>
      <span class="status-value">
        {{ serialConnected ? serialPort + '@' + formatBaud(serialBaud) : '--' }}
      </span>
    </div>
    <div class="status-sep" />
    <div class="status-group">
      <div class="status-dot" :class="{ on: netRunning }" />
      <span class="status-label">{{ t('status.network') }}</span>
      <span class="status-value">
        {{ netRunning ? netIp + ':' + netPort : '--' }}
      </span>
    </div>
    <div class="status-spacer" />
    <span class="status-ver">v{{ version }}</span>
  </div>
</template>

<script setup>
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

const props = defineProps({
  serialConnected: Boolean,
  serialPort: { type: String, default: '' },
  serialBaud: { type: Number, default: 0 },
  netRunning: Boolean,
  netPort: { type: Number, default: 0 },
  netIp: { type: String, default: '' },
  version: { type: String, default: '1.0.0' },
});

function formatBaud(baud) {
  if (baud >= 1000000) return (baud / 1000000).toFixed(baud % 1000000 ? 1 : 0) + 'M';
  if (baud >= 1000) return Math.round(baud / 1000) + 'K';
  return '' + baud;
}
</script>
