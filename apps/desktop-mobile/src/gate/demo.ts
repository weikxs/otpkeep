import { type EntryEdit, GateFailure, type GateApi, type GateEntry } from "./api";

/** Made-up accounts for the browser preview and tests. */
const DEMO: GateEntry[] = [
  { id: "d1", title: "GitHub", issuer: "GitHub", account: "demo-user", note: "", pinned: true, kind: "totp", defaultShortcut: "control+alt+super+Digit1" },
  { id: "d2", title: "邮箱", issuer: "示例邮箱", account: "demo@example.com", note: "备用码在抽屉里", pinned: false, kind: "totp", shortcut: "control+alt+super+KeyM", defaultShortcut: "control+alt+super+Digit2" },
  { id: "d3", title: "Bank token", issuer: "Example Bank", account: "0000", note: "", pinned: false, kind: "hotp", defaultShortcut: "control+alt+super+Digit3" },
];

const TITLE_MAX = 100;

/** A GateApi over demo data, with the core's main rules. */
export function demoGate(): GateApi & { ended: boolean } {
  let entries = DEMO.map((e) => ({ ...e }));
  const state = { ended: false };
  const find = (id: string) => {
    const entry = entries.find((e) => e.id === id);
    if (!entry) throw new GateFailure("NOT_FOUND");
    return entry;
  };
  const replace = (updated: GateEntry) => {
    entries = entries.map((e) => (e.id === updated.id ? updated : e));
    return updated;
  };
  return {
    get ended() {
      return state.ended;
    },
    entries: async (query) => {
      const q = query.trim().toLowerCase();
      return entries.filter((e) => q === "" || `${e.title} ${e.issuer} ${e.account} ${e.note}`.toLowerCase().includes(q));
    },
    edit: async (id, edit: EntryEdit) => {
      const entry = find(id);
      if (edit.title !== undefined && edit.title.trim() === "") throw new GateFailure("TITLE_EMPTY");
      if (edit.title !== undefined && edit.title.length > TITLE_MAX) throw new GateFailure("TITLE_TOO_LONG");
      return replace({ ...entry, ...edit });
    },
    setShortcut: async (id, shortcut) => {
      const entry = find(id);
      if (shortcut === null) {
        const { shortcut: _removed, ...rest } = entry;
        return replace(rest);
      }
      const canonical = shortcut.replace("Control", "control").replace("Alt", "alt").replace("Shift", "shift").replace("Super", "super");
      const holder = entries.find((e) => e.id !== id && e.shortcut === canonical);
      if (holder) throw new GateFailure("SHORTCUT_TAKEN", `ENTRY:${holder.title}`);
      const row = entries.find((e) => e.defaultShortcut === canonical);
      if (row) throw new GateFailure("SHORTCUT_TAKEN", `ROW:${entries.indexOf(row) + 1}`);
      return replace({ ...entry, shortcut: canonical });
    },
    heartbeat: async () => {},
    end: () => {
      state.ended = true;
    },
  };
}
