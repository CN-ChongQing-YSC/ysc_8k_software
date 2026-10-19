/** AppSelect 的选项类型。放在独立 .ts 文件里，便于 index.ts 以类型方式导出
 *  （`*.vue` 垫片只声明默认导出，无法从 SFC 里再导出命名类型）。 */
export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}
