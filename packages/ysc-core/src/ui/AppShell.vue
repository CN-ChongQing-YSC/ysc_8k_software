<template>
  <div class="shell">
    <TopBar />
    <div v-if="banner" class="banner warn">{{ banner }}</div>
    <div class="shell-body">
      <NavRail :groups="groups" />
      <main class="shell-content">
        <slot />
      </main>
    </div>
    <slot name="footer" />
  </div>
</template>

<script setup lang="ts">
import TopBar from './TopBar.vue';
import NavRail from './NavRail.vue';
import { NAV } from './nav-config';

withDefaults(defineProps<{ banner?: string }>(), { banner: '' });

const groups = NAV;
</script>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}
.banner {
  padding: 10px 14px;
  font-size: 13px;
  border-bottom: 1px solid var(--border);
}
.banner.warn {
  background: rgba(251, 191, 36, 0.1);
  color: var(--accent-yellow);
}
.shell-body {
  flex: 1;
  display: flex;
  min-height: 0;
}
.shell-content {
  flex: 1;
  overflow-y: auto;
  padding: 18px;
  background: var(--bg-primary);
}
</style>
