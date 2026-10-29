import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '..', '..');
const coreSrc = path.join(repoRoot, 'packages', 'ysc-core', 'src');
const uiSrc = path.join(repoRoot, 'packages', 'ysc-ui', 'src');
const selfModules = path.join(here, 'node_modules');

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  resolve: {
    alias: [
      { find: /^@ysc\/core$/, replacement: path.join(coreSrc, 'index.ts') },
      { find: /^@ysc\/core\/(.*)$/, replacement: path.join(coreSrc, '$1') },
      { find: /^@ysc\/ui$/, replacement: path.join(uiSrc, 'index.ts') },
      { find: /^@ysc\/ui\/(.*)$/, replacement: path.join(uiSrc, '$1') },
      { find: /^vue$/, replacement: path.join(selfModules, 'vue') },
      { find: /^pinia$/, replacement: path.join(selfModules, 'pinia') },
      { find: /^gsap$/, replacement: path.join(selfModules, 'gsap') },
    ],
    dedupe: ['vue', 'pinia', 'gsap'],
  },
  server: {
    host: '127.0.0.1',
    port: 5179,
    strictPort: true,
    fs: { allow: [repoRoot] },
  },
  build: {
    target: 'chrome110',
    outDir: 'dist',
    emptyOutDir: true,
    chunkSizeWarningLimit: 1200,
    // 主窗口 + 独立手柄测试器窗口（Tauri WebviewWindow 加载 tester.html）
    rollupOptions: {
      input: {
        main: path.join(here, 'index.html'),
        tester: path.join(here, 'tester.html'),
      },
    },
  },
});
