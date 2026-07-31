<template>
  <div class="panel panel-log">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 6 6 8 4 10" /><line x1="8" y1="10" x2="12" y2="10" /><rect x="1.5" y="2" width="13" height="12" rx="2" /></svg>
      <span>调试日志</span>
      <div style="flex: 1" />
      <button class="btn btn-xs" @click="$emit('clear')">清除</button>
    </div>
    <div class="log-body" ref="logBody">{{ text }}</div>
  </div>
</template>

<script setup>
import { ref, watch, nextTick, computed } from 'vue';

const props = defineProps({
  messages: { type: Array, default: () => [] },
});

defineEmits(['clear']);

const logBody = ref(null);

const text = computed(() => props.messages.join('\n'));

watch(() => props.messages.length, async () => {
  await nextTick();
  if (logBody.value) {
    logBody.value.scrollTop = logBody.value.scrollHeight;
  }
});
</script>
