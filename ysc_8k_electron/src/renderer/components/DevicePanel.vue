<template>
  <div class="panel">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <rect x="1" y="4" width="6" height="8" rx="1" /><rect x="9" y="4" width="6" height="8" rx="1" />
      </svg>
      <span>{{ t('device.title') }}</span>
      <button class="d-refresh" @click="emit('refresh')">{{ t('serial.refresh') }}</button>
    </div>
    <div class="panel-body">
      <div class="d-hint">{{ t('device.hint') }}</div>

      <div v-for="slot in slots" :key="slot.dev" class="d-card" :class="{ on: slot.connected }">
        <div class="d-card-head">
          <span class="d-role-badge" :class="'role-' + roleKey(slot)">
            {{ roleLabel(slot) }}
          </span>
          <span class="d-status" :class="{ on: slot.connected }">
            {{ slot.connected ? t('device.connected') : t('device.notConnected') }}
          </span>
          <span v-if="slot.connected" class="d-port">{{ slot.port }}<template v-if="slot.baud"> @ {{ slot.baud }}</template></span>
        </div>

        <div class="d-row">
          <label>{{ t('device.roleLabel') }}</label>
          <select
            class="input-select"
            :value="slot.role === 'none' ? 'auto' : slot.role"
            @change="onRoleChange(slot, $event)"
          >
            <option value="auto">{{ t('device.roleAuto') }}</option>
            <option value="mouse">{{ t('device.roleMouse') }}</option>
            <option value="kbd">{{ t('device.roleKbd') }}</option>
          </select>
          <span v-if="slot.connected && slot.role !== 'none' && slot.auto" class="d-auto-tag">{{ t('device.auto') }}</span>
          <span v-else-if="slot.connected && slot.role === 'none' && !slot.probeDone" class="d-auto-tag warn">{{ t('device.detecting') }}</span>
          <span v-else-if="slot.connected && slot.role === 'none' && slot.probeDone" class="d-auto-tag warn">{{ t('device.detectFailed') }}</span>
        </div>

        <template v-if="!slot.connected">
          <div class="d-row">
            <label>{{ t('device.port') }}</label>
            <select class="input-select" v-model="pick[slot.dev].port">
              <option value="">{{ t('device.portPlaceholder') }}</option>
              <option v-for="p in ports" :key="p" :value="p">{{ p }}</option>
            </select>
          </div>
          <div class="d-row">
            <label>{{ t('device.baud') }}</label>
            <input class="input-text" v-model="pick[slot.dev].baud" :placeholder="t('device.baudHint')" />
          </div>
          <div class="d-actions">
            <button
              class="btn btn-accent"
              :disabled="!pick[slot.dev].port"
              @click="onConnect(slot)"
            >{{ t('device.connect') }}</button>
            <button class="btn btn-outline red" @click="onDisconnect(slot)">
              {{ t('device.disconnect') }}
            </button>
          </div>
        </template>
        <template v-else>
          <div class="d-row">
            <label>{{ t('device.upstream') }}</label>
            <span class="d-upstream">{{ upstreamText(slot) }}</span>
          </div>
          <div class="d-actions">
            <button class="btn btn-outline red" @click="onDisconnect(slot)">
              {{ t('device.disconnect') }}
            </button>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup>
import { reactive } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

const props = defineProps({
  /** 设备槽位列表（来自驱动器 device_list 事件） */
  slots: { type: Array, default: () => [] },
  /** 可选串口列表 */
  ports: { type: Array, default: () => [] },
});

const emit = defineEmits(['connect', 'disconnect', 'set-role', 'refresh']);

// 每个槽位的待连接端口/波特率
const pick = reactive({
  0: { port: '', baud: '0' },
  1: { port: '', baud: '0' },
});

function roleKey(slot) {
  if (!slot.connected) return 'none';
  if (slot.role === 'mouse') return 'mouse';
  if (slot.role === 'kbd') return 'kbd';
  return 'none';
}

function roleLabel(slot) {
  const k = roleKey(slot);
  if (k === 'mouse') return t('device.roleMouse');
  if (k === 'kbd') return t('device.roleKbd');
  return t('device.roleNone');
}

function upstreamText(slot) {
  if (slot.vid == null || slot.vid < 0) return t('device.upstreamUnknown');
  const vid = slot.vid.toString(16).toUpperCase().padStart(4, '0');
  const pid = (slot.pid >= 0 ? slot.pid : 0).toString(16).toUpperCase().padStart(4, '0');
  return (slot.product ? slot.product + '  ' : '') + vid + ':' + pid;
}

function onConnect(slot) {
  const p = pick[slot.dev];
  emit('connect', slot.dev, p.port, parseInt(p.baud) || 0);
}

function onDisconnect(slot) {
  emit('disconnect', slot.dev);
}

function onRoleChange(slot, e) {
  // auto = 交回驱动自动识别（置为 none，驱动会重新探测）
  emit('set-role', slot.dev, e.target.value === 'auto' ? 'none' : e.target.value);
}
</script>

<style scoped>
.d-refresh {
  margin-left: auto;
  padding: 2px 10px;
  font-size: 12px;
  border: 1px solid var(--border, #444);
  border-radius: 4px;
  background: transparent;
  color: var(--text, #ccc);
  cursor: pointer;
}
.d-refresh:hover { border-color: var(--accent, #4a9); }

.d-hint {
  font-size: 11px;
  opacity: 0.65;
  margin-bottom: 8px;
  line-height: 1.5;
}

.d-card {
  border: 1px solid var(--border, #444);
  border-radius: 6px;
  padding: 8px 10px;
  margin-bottom: 8px;
  background: rgba(255, 255, 255, 0.02);
}
.d-card.on { border-color: var(--accent, #4a9); }

.d-card-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
}

.d-role-badge {
  font-size: 11px;
  padding: 1px 8px;
  border-radius: 10px;
  border: 1px solid var(--border, #444);
}
.d-role-badge.role-mouse { border-color: #4a9; color: #6cb; }
.d-role-badge.role-kbd { border-color: #c96; color: #dba; }

.d-status { font-size: 11px; opacity: 0.6; }
.d-status.on { color: #6cb; opacity: 1; }

.d-port { font-size: 11px; opacity: 0.7; margin-left: auto; }

.d-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 5px;
}
.d-row label {
  font-size: 11px;
  min-width: 52px;
  opacity: 0.7;
}
.d-row .input-select, .d-row .input-text { flex: 1; min-width: 0; }

.d-auto-tag {
  font-size: 10px;
  padding: 0 5px;
  border-radius: 3px;
  border: 1px solid var(--border, #444);
  opacity: 0.7;
}
.d-auto-tag.warn { border-color: #c96; color: #dba; opacity: 1; }

.d-upstream { font-size: 11px; opacity: 0.8; }

.d-actions { display: flex; gap: 6px; margin-top: 6px; }
.d-actions .btn { flex: 1; }
</style>
