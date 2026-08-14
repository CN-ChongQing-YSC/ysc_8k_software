<template>
  <button
    type="button"
    class="tgl"
    :class="{ on: modelValue, disabled: disabled }"
    role="switch"
    :aria-checked="modelValue"
    :disabled="disabled"
    @click="onClick"
  >
    <span class="track"><span class="thumb" /></span>
    <span v-if="label || $slots.default" class="lbl">
      <slot>{{ label }}</slot>
    </span>
  </button>
</template>

<script setup lang="ts">
const props = withDefaults(
  defineProps<{ modelValue: boolean; label?: string; disabled?: boolean }>(),
  { disabled: false },
);
const emit = defineEmits<{ 'update:modelValue': [boolean] }>();
function onClick(): void {
  if (!props.disabled) emit('update:modelValue', !props.modelValue);
}
</script>

<style scoped>
.tgl {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  background: transparent;
  border: none;
  padding: 0;
  font: inherit;
  color: var(--text-secondary);
  cursor: pointer;
}
.tgl.disabled {
  cursor: not-allowed;
  opacity: 0.5;
}
.track {
  width: 38px;
  height: 22px;
  border-radius: 999px;
  background: var(--bg-selected-soft);
  border: 1px solid var(--border);
  position: relative;
  transition: var(--transition-fast);
  flex-shrink: 0;
}
.thumb {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--text-muted);
  transition: var(--transition-fast);
}
.tgl:hover:not(.disabled) .track {
  border-color: var(--border-strong);
}
.tgl.on .track {
  background: var(--accent-green);
  border-color: var(--accent-green);
}
.tgl.on .thumb {
  left: 18px;
  background: #0d1f12;
}
.tgl:focus-visible .track {
  box-shadow: var(--ring-focus);
}
.lbl {
  font-size: var(--text-sm);
  font-weight: var(--weight-semibold);
}
</style>
