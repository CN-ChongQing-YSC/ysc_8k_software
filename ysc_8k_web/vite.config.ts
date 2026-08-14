import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { fileURLToPath, URL } from 'node:url';

// 源码直消费：把 @ysc/core 别名指向同级 packages/ysc-core/src，
// 让 Vite 把共享核心当成本 app 的源码编译（单一 Vue 实例 + 完整 HMR）。
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@ysc/core': fileURLToPath(new URL('../packages/ysc-core/src', import.meta.url)),
    },
  },
  build: {
    outDir: 'dist',
  },
  server: {
    port: 5174,
  },
});
