<template>
  <div class="panel">
    <div class="panel-header">
      <svg class="panel-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M8 12V4" /><path d="M4.5 6.5L8 3l3.5 3.5" /><line x1="3" y1="13" x2="13" y2="13" /></svg>
      <span>HID 上传</span>
    </div>
    <div class="panel-body">
      <button
        class="btn btn-toggle"
        :class="{ active: enabled }"
        @click="$emit('toggle')"
      >
        {{ enabled ? '禁用' : '启用' }}
      </button>
    </div>
  </div>
</template>

<script setup>
defineProps({
  enabled: Boolean,
});

defineEmits(['toggle']);
</script>
