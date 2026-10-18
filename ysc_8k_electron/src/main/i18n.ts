/**
 * 主进程语言状态。渲染进程切换语言时通过 `app:setLang` 同步过来，
 * 用于对话框标题、文件大小单位、错误提示等主进程侧文案。
 *
 * 主进程没有引入 Vue i18n（那一套只服务于 renderer），这里用最小的 tr() 就够了。
 */
export type MainLang = 'zh' | 'en';

let currentLang: MainLang = 'zh';

export function setMainLang(lang: unknown): void {
  if (lang === 'zh' || lang === 'en') currentLang = lang;
}

export function getMainLang(): MainLang {
  return currentLang;
}

/** 按当前语言返回中文或英文文案。 */
export function tr(zh: string, en: string): string {
  return currentLang === 'en' ? en : zh;
}

/** 字节数 → 可读大小；单位跟随当前语言。 */
export function formatSize(bytes: number): string {
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return currentLang === 'en' ? `${bytes} B` : `${bytes} 字节`;
}
