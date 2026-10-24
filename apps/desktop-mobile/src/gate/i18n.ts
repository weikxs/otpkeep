/** The management page's text in Simplified Chinese and English; the browser's language decides. */

const zh = {
  title: "OTPKeep 管理",
  search: "搜索名称、账号、备注或标签",
  count: (n: number) => `${n} 个账号`,
  connecting: "正在连接 OTPKeep…",
  ended: "会话已结束",
  endedBody: "请从菜单栏的 OTPKeep 图标选择“在浏览器中管理…”重新打开。",
  linkUsed: "此链接已被使用",
  linkUsedBody: "每个链接只能打开一次。请从 OTPKeep 重新打开管理页。",
  needsPassword: "请先在 OTPKeep 中输入主口令，然后刷新本页。",
  noMatch: "没有匹配的账号",
  empty: "还没有账号",
  name: "显示名称",
  note: "备注",
  notePlaceholder: "例如：备用码放在哪里、这个账号做什么用",
  edit: "编辑",
  save: "保存",
  cancel: "取消",
  saved: "已保存",
  copy: "复制验证码",
  copied: (s: number) => (s > 0 ? `已复制 · ${s} 秒后从剪贴板清除` : "已复制"),
  shortcut: "快捷键",
  noShortcut: "未设置",
  defaultShortcut: (label: string) => `默认 ${label}（按列表位置）`,
  record: "设置",
  change: "更改",
  clear: "清除",
  recording: "请按下组合键…（Esc 取消）",
  shortcutSaved: "快捷键已设置，按下即可复制验证码",
  shortcutCleared: "已清除快捷键",
  errors: {
    TITLE_EMPTY: "名称不能为空",
    TITLE_TOO_LONG: "名称不能超过 100 个字符",
    NOTE_TOO_LONG: "备注不能超过 2000 个字符",
    NOT_FOUND: "这个账号已不存在，请刷新",
    SHORTCUT_INVALID: "需要包含 ⌃ Control、⌥ Option / Alt 或 ⌘ Command / Win 中的至少一个",
    SHORTCUT_REJECTED: "系统没有接受这个组合，可能已被其他程序占用，请换一个",
    NEEDS_PASSWORD: "请先在 OTPKeep 中输入主口令",
    NO_CODE_YET: "计数器型（HOTP）账号需要先在 OTPKeep 面板中生成验证码",
    COPY_FAILED: "复制失败，请重试",
    READ_ONLY: "新版本 OTPKeep 正在使用这些数据，不能修改",
    SAVE_FAILED: "保存失败，请重试",
    NO_SESSION: "会话已结束，请从 OTPKeep 重新打开",
    UNKNOWN: "出错了，请重试",
  } as Record<string, string>,
  takenBy: (holder: string) => {
    if (holder === "SEARCH_PANEL") return "这个组合留给搜索面板使用，请换一个";
    if (holder.startsWith("ROW:")) return `这个组合是列表第 ${holder.slice(4)} 个账号的默认快捷键，请换一个`;
    if (holder.startsWith("ENTRY:")) return `这个组合已被“${holder.slice(6)}”使用`;
    return "这个组合已被使用";
  },
};

export type GateStrings = typeof zh;

const en: GateStrings = {
  title: "OTPKeep management",
  search: "Search names, accounts, notes or tags",
  count: (n: number) => `${n} account${n === 1 ? "" : "s"}`,
  connecting: "Connecting to OTPKeep…",
  ended: "The session has ended",
  endedBody: "Open it again from the OTPKeep menu bar icon: “Manage in the browser…”.",
  linkUsed: "This link has already been used",
  linkUsedBody: "Each link opens once. Open the management page again from OTPKeep.",
  needsPassword: "Enter the master password in OTPKeep first, then reload this page.",
  noMatch: "No account matches",
  empty: "No accounts yet",
  name: "Display name",
  note: "Note",
  notePlaceholder: "For example: where the backup codes are, what this account is for",
  edit: "Edit",
  save: "Save",
  cancel: "Cancel",
  saved: "Saved",
  copy: "Copy code",
  copied: (s) => (s > 0 ? `Copied · clipboard clears in ${s}s` : "Copied"),
  shortcut: "Shortcut",
  noShortcut: "None",
  defaultShortcut: (label: string) => `Default ${label} (by list position)`,
  record: "Set",
  change: "Change",
  clear: "Clear",
  recording: "Press the key combination… (Esc cancels)",
  shortcutSaved: "Shortcut set: press it to copy the code",
  shortcutCleared: "Shortcut cleared",
  errors: {
    TITLE_EMPTY: "The name cannot be empty",
    TITLE_TOO_LONG: "The name cannot be longer than 100 characters",
    NOTE_TOO_LONG: "The note cannot be longer than 2000 characters",
    NOT_FOUND: "This account no longer exists; reload the page",
    SHORTCUT_INVALID: "Include at least one of Control, Alt / Option or Command / Win",
    SHORTCUT_REJECTED: "The system did not accept this combination; another app may use it. Try another",
    NEEDS_PASSWORD: "Enter the master password in OTPKeep first",
    NO_CODE_YET: "Counter-based (HOTP) accounts need a code generated in the OTPKeep panel first",
    COPY_FAILED: "Could not copy; try again",
    READ_ONLY: "A newer OTPKeep uses this data; it cannot be changed",
    SAVE_FAILED: "Could not save; try again",
    NO_SESSION: "The session has ended; open the page again from OTPKeep",
    UNKNOWN: "Something went wrong; try again",
  },
  takenBy: (holder: string) => {
    if (holder === "SEARCH_PANEL") return "This combination is kept for the search panel; try another";
    if (holder.startsWith("ROW:")) return `This is the default shortcut of account ${holder.slice(4)} in the list; try another`;
    if (holder.startsWith("ENTRY:")) return `“${holder.slice(6)}” already uses this combination`;
    return "This combination is already used";
  },
};

export function gateStrings(language: string): GateStrings {
  return language.toLowerCase().startsWith("zh") ? zh : en;
}

/** The message for a refusal. */
export function describeFailure(text: GateStrings, code: string, detail?: string): string {
  if (code === "SHORTCUT_TAKEN") return text.takenBy(detail ?? "");
  return text.errors[code] ?? text.errors.UNKNOWN ?? code;
}
