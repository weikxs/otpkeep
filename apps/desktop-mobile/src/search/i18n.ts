/** The search panel's own words; the rest come from the quick panel's strings. */

const zh = {
  copy: "复制",
  type: "输入",
  noPermission: (s: number) => `没有"辅助功能"授权，已改为复制${s > 0 ? `（${s} 秒后清除）` : ""}`,
  unsupported: (s: number) => `此桌面环境不支持自动输入，已改为复制${s > 0 ? `（${s} 秒后清除）` : ""}`,
  mainOpen: (s: number) => `主窗口开着时无法自动输入，已改为复制${s > 0 ? `（${s} 秒后清除）` : ""}`,
  failed: (s: number) => `无法输入验证码，已改为复制${s > 0 ? `（${s} 秒后清除）` : ""}`,
  grant: "去授权",
  edit: "编辑",
  close: "关闭",
};

export type SearchStrings = typeof zh;

const en: SearchStrings = {
  copy: "Copy",
  type: "Type",
  noPermission: (s: number) =>
    `No Accessibility permission, so it was copied instead${s > 0 ? ` (clears in ${s}s)` : ""}`,
  unsupported: (s: number) =>
    `This desktop does not allow typing, so it was copied instead${s > 0 ? ` (clears in ${s}s)` : ""}`,
  mainOpen: (s: number) =>
    `Typing does not work while the main window is open, so it was copied instead${s > 0 ? ` (clears in ${s}s)` : ""}`,
  failed: (s: number) => `The code could not be typed, so it was copied instead${s > 0 ? ` (clears in ${s}s)` : ""}`,
  grant: "Allow",
  edit: "Edit",
  close: "Close",
};

export function searchStrings(language: string): SearchStrings {
  return language.toLowerCase().startsWith("zh") ? zh : en;
}
