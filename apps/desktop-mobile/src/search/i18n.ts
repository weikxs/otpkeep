/** The search panel's own words; the rest come from the quick panel's strings. */

const zh = {
  copy: "复制",
  edit: "编辑",
  close: "关闭",
};

export type SearchStrings = typeof zh;

const en: SearchStrings = {
  copy: "Copy",
  edit: "Edit",
  close: "Close",
};

export function searchStrings(language: string): SearchStrings {
  return language.toLowerCase().startsWith("zh") ? zh : en;
}
