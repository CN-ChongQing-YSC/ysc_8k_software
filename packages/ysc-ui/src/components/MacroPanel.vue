<template>
  <div class="panel macro-page">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <rect x="2" y="5" width="12" height="6" rx="1.5" />
        <line x1="4" y1="7" x2="6" y2="7" />
        <line x1="7" y1="7" x2="9" y2="7" />
        <line x1="10" y1="7" x2="12" y2="7" />
        <line x1="4" y1="9.5" x2="12" y2="9.5" />
      </svg>
      <span>{{ t('macro.title') }}</span>
      <div class="macro-header-hint">{{ t('macro.hint') }}</div>
      <div style="flex:1" />
      <div class="macro-header-actions">
        <button class="btn btn-ghost" @click="loadFromDevice">{{ t('macro.refresh') }}</button>
        <button class="btn btn-accent" :disabled="!dirty" @click="saveAll">{{ t('macro.save') }}</button>
        <button class="btn btn-outline red" @click="onReset">{{ t('macro.reset') }}</button>
      </div>
    </div>
    <div v-if="toast" class="macro-toast" :class="toastType">{{ toast }}</div>
    <div class="macro-grid">
      <div class="macro-slot" v-for="(m, i) in slots" :key="i">
        <div class="macro-slot-header">
          <span class="macro-slot-num">{{ i + 1 }}</span>
          <span class="macro-slot-title">{{ t('macro.slot', { n: i + 1 }) }}</span>
          <div style="flex:1" />
          <button class="btn-toggle btn-xs" :class="{ active: m.enabled }" @click="toggleSlot(i)">
            {{ m.enabled ? 'ON' : 'OFF' }}
          </button>
        </div>
        <div class="macro-slot-body" v-show="m.enabled">
          <div class="macro-field">
            <span class="macro-field-label">{{ t('macro.trigger') }}</span>
            <span class="macro-field-hint">{{ t('macro.triggerHint') }}</span>
            <div class="macro-btn-group">
              <button v-for="b in buttonDefs" :key="'t'+b.bit"
                class="macro-btn-toggle"
                :class="{ active: (m.trigger & b.bit) !== 0 }"
                @click="toggleBit(i, 'trigger', b.bit)"
              >{{ b.name }}</button>
            </div>
          </div>
          <div class="macro-field">
            <span class="macro-field-label">{{ t('macro.suppress') }}</span>
            <span class="macro-field-hint">{{ t('macro.suppressHint') }}</span>
            <div class="macro-btn-group">
              <button v-for="b in buttonDefs" :key="'p'+b.bit"
                class="macro-btn-toggle"
                :class="{ active: (m.suppress & b.bit) !== 0 }"
                @click="toggleBit(i, 'suppress', b.bit)"
              >{{ b.name }}</button>
            </div>
          </div>
          <div class="macro-field">
            <span class="macro-field-label">{{ t('macro.wheel') }}</span>
            <select class="input-select macro-wheel-select" :value="m.wheel" @change="setWheel(i, $event)">
              <option :value="0">{{ t('macro.wheelNone') }}</option>
              <option :value="-3">-3</option>
              <option :value="-2">-2</option>
              <option :value="-1">{{ t('macro.wheelDown') }}</option>
              <option :value="1">{{ t('macro.wheelUp') }}</option>
              <option :value="2">2</option>
              <option :value="3">3</option>
            </select>
          </div>
          <div class="macro-field macro-interval-field" v-show="m.wheel !== 0">
            <div class="macro-interval-col">
              <span class="macro-field-label">{{ t('macro.interval') }}</span>
              <span class="macro-field-hint">{{ t('macro.intervalHint') }}</span>
              <div class="macro-num-input">
                <input type="number" class="input-select" min="0" max="65535" step="1"
                  :value="m.interval" @input="setNum(i, 'interval', $event)" />
                <span class="macro-num-unit">{{ t('macro.intervalUnit') }}</span>
              </div>
              <div class="macro-num-input">
                <span class="macro-num-prefix">±</span>
                <input type="number" class="input-select" min="0" max="65535" step="1"
                  :title="t('macro.intervalJitterHint')"
                  :value="m.ij" @input="setNum(i, 'ij', $event)" />
                <span class="macro-num-unit">{{ t('macro.intervalUnit') }}</span>
              </div>
            </div>
            <div class="macro-interval-col">
              <span class="macro-field-label">{{ t('macro.duration') }}</span>
              <span class="macro-field-hint">{{ t('macro.durationHint') }}</span>
              <div class="macro-num-input">
                <input type="number" class="input-select" min="0" max="65535" step="1"
                  :value="m.duration" @input="setNum(i, 'duration', $event)" />
                <span class="macro-num-unit">{{ t('macro.durationUnit') }}</span>
              </div>
              <div class="macro-num-input">
                <span class="macro-num-prefix">±</span>
                <input type="number" class="input-select" min="0" max="65535" step="1"
                  :title="t('macro.durationJitterHint')"
                  :value="m.dj" @input="setNum(i, 'dj', $event)" />
                <span class="macro-num-unit">{{ t('macro.durationUnit') }}</span>
              </div>
            </div>
          </div>
        </div>
        <div class="macro-slot-empty" v-show="!m.enabled">
          <span>{{ t('macro.notEnabled') }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { reactive, ref, computed, watch } from 'vue';
import { useI18n } from '../i18n/index.js';

const { t } = useI18n();

const props = defineProps({
  macros: { type: Array, default: () => [] }
});
const emit = defineEmits(['save-all', 'reset', 'load']);

const buttonDefs = computed(() => [
  { bit: 0x01, name: t('monitor.btn.left') },
  { bit: 0x02, name: t('monitor.btn.right') },
  { bit: 0x04, name: t('monitor.btn.middle') },
  { bit: 0x08, name: t('monitor.btn.back') },
  { bit: 0x10, name: t('monitor.btn.forward') },
]);

const dirty = ref(false);
const toast = ref('');
const toastType = ref('ok');
let toastTimer = null;

function showToast(msg, type) {
  toast.value = msg;
  toastType.value = type || 'ok';
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(function() { toast.value = ''; }, 2000);
}

const slots = reactive(Array.from({ length: 8 }, () => ({
  enabled: 0, trigger: 0, suppress: 0, wheel: 0, interval: 0, duration: 0, ij: 0, dj: 0
})));

watch(() => props.macros, (newMacros) => {
  if (!newMacros || !newMacros.length) return;
  for (let i = 0; i < 8; i++) {
    if (newMacros[i]) {
      slots[i].enabled  = newMacros[i].enabled;
      slots[i].trigger  = newMacros[i].trigger;
      slots[i].suppress = newMacros[i].suppress;
      slots[i].wheel    = newMacros[i].wheel;
      slots[i].interval = newMacros[i].interval || 0;
      slots[i].duration = newMacros[i].duration || 0;
      slots[i].ij = newMacros[i].ij || 0;
      slots[i].dj = newMacros[i].dj || 0;
    }
  }
  dirty.value = false;
}, { deep: true, immediate: true });

function markDirty() {
  dirty.value = true;
}

function toggleSlot(i) {
  slots[i].enabled = slots[i].enabled ? 0 : 1;
  markDirty();
}

function toggleBit(i, field, bit) {
  slots[i][field] = slots[i][field] ^ bit;
  markDirty();
}

function setWheel(i, ev) {
  slots[i].wheel = parseInt(ev.target.value, 10);
  markDirty();
}

function setNum(i, field, ev) {
  var v = parseInt(ev.target.value, 10);
  if (isNaN(v)) v = 0;
  if (v < 0) v = 0;
  if (v > 65535) v = 65535;
  slots[i][field] = v;
  markDirty();
}

function saveAll() {
  var all = [];
  for (var i = 0; i < 8; i++) {
    var interval = slots[i].interval;
    var ij = slots[i].ij;
    if (ij > interval) ij = interval;                 // 浮动幅度不超过基准，避免负数实际值
    var duration = slots[i].duration;
    // 持续时长若超过间隔，按固件语义等同满周期；这里在 UI 侧也做一次规整
    if (interval > 0 && duration > interval) duration = interval;
    var dj = slots[i].dj;
    if (dj > duration) dj = duration;
    all.push({
      enabled: slots[i].enabled,
      trigger: slots[i].trigger,
      suppress: slots[i].suppress,
      wheel: slots[i].wheel,
      interval: interval,
      duration: duration,
      ij: ij,
      dj: dj,
    });
  }
  emit('save-all', all);
  dirty.value = false;
  showToast(t('macro.saved'), 'ok');
}

function onReset() {
  for (var i = 0; i < 8; i++) {
    slots[i].enabled = 0;
    slots[i].trigger = 0;
    slots[i].suppress = 0;
    slots[i].wheel = 0;
    slots[i].interval = 0;
    slots[i].duration = 0;
    slots[i].ij = 0;
    slots[i].dj = 0;
  }
  emit('reset');
  dirty.value = false;
  showToast(t('macro.resetDone'), 'ok');
}

function loadFromDevice() {
  emit('load');
  showToast(t('macro.refreshed'), 'ok');
}
</script>
