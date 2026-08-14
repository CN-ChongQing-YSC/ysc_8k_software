/**
 * Vue SFC 类型垫片：让纯 tsc 能解析 `import X from './X.vue'`。
 * .vue 文件本身的模板类型检查由消费方 app 的 vite/vue-tsc 负责；
 * 这里仅声明模块存在，导出为一个泛型 DefineComponent。
 */
declare module '*.vue' {
  import type { DefineComponent } from 'vue';
  const component: DefineComponent<Record<string, never>, Record<string, never>, unknown>;
  export default component;
}
