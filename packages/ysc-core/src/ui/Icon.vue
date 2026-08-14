<template>
  <span
    class="material-symbols-outlined ysc-icon"
    :style="{ fontSize: size + 'px', width: size + 'px', height: size + 'px' }"
    aria-hidden="true"
    >{{ glyph }}</span
  >
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { ICONS } from './icons';
import type { IconKey } from './nav-config';

const props = withDefaults(defineProps<{ name: IconKey; size?: number }>(), {
  size: 20,
});

const glyph = computed(() => ICONS[props.name] ?? 'circle');
</script>

<style scoped>
.ysc-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
</style>
