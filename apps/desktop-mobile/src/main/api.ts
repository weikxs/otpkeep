/**
 * What the main window asks of the core (design/07-platform.md §2.4a). Accounts come with display fields and
 * codes; secrets only travel one way, typed into the manual form.
 */

import type { CopyResult, PanelEntry, PanelState } from "../panel/api";

export type { CopyResult };
export type Entry = PanelEntry;
export type MainState = PanelState;

/** A refusal: `code` names it, `detail` adds a rule or a position. */
export class CommandFailure extends Error {
  constructor(
    readonly code: string,
    readonly detail?: string,
  ) {
    super(code);
  }
}

export type RowGroup = "fresh" | "duplicate";

export interface ImportRow {
  group: RowGroup;
  /** Position within its group, as the commit counts. */
  index: number;
  title: string;
  issuer: string;
  account: string;
  kind: "totp" | "hotp" | "steam";
  algorithm: string;
  digits: number;
  periodS?: number;
  counter?: number;
  /** The code right now, to compare with the website. */
  sampleCode?: string;
  titleTaken: boolean;
  lost: string[];
  existingTitle?: string;
  sameAsRow?: number;
}

export type Place =
  | { kind: "line"; line: number }
  | { kind: "transfer"; code: number; index: number }
  | { kind: "named"; name: string };

export interface SkippedPart {
  place: Place;
  reason: string;
  detail?: string;
}

export interface ImportView {
  token: string;
  rows: ImportRow[];
  skipped: SkippedPart[];
}

export interface ManualForm {
  title: string;
  issuer: string;
  account: string;
  secret: string;
  kind: "totp" | "hotp" | "steam";
  algorithm: "SHA1" | "SHA256" | "SHA512";
  digits: number;
  periodS: number;
  counter: number;
}

/** Where to move an entry. */
export type MoveTo = { to: "first" } | { to: "last" } | { to: "before"; other: string } | { to: "after"; other: string };

export interface TrashItem {
  id: string;
  title: string;
  issuer: string;
  account: string;
  trashedAtMs: number;
  /** When it will be deleted for good. */
  purgeAtMs: number;
}

export interface Version {
  value: string;
  atMs: number;
}

/** Earlier names and notes, newest first; kept on this device only. */
export interface History {
  title: Version[];
  note: Version[];
}

/** An entry's code parameters, never its secret. */
export interface KeyInfo {
  kind: "totp" | "hotp" | "steam";
  algorithm: string;
  digits: number;
  periodS?: number;
  counter?: number;
  shortSecret: boolean;
}

/** New parameters; a typed secret replaces the current one, an empty one keeps it. */
export interface KeyForm {
  secret: string;
  kind: "totp" | "hotp" | "steam";
  algorithm: "SHA1" | "SHA256" | "SHA512";
  digits: number;
  periodS: number;
  counter: number;
}

export type BatchAction = { action: "trash" } | { action: "addTag"; tag: string } | { action: "removeTag"; tag: string };

export interface MainApi {
  state(query: string): Promise<MainState>;
  copy(id: string): Promise<CopyResult>;
  nextHotp(id: string): Promise<PanelEntry>;
  edit(id: string, change: { title?: string; note?: string }): Promise<void>;
  unlock(password: string): Promise<MainState>;
  setPinned(id: string, pinned: boolean): Promise<void>;
  setTags(id: string, tags: string[]): Promise<void>;
  move(id: string, to: MoveTo): Promise<void>;
  trash(id: string): Promise<void>;
  restore(id: string): Promise<void>;
  purge(id: string): Promise<void>;
  trashList(): Promise<TrashItem[]>;
  history(id: string): Promise<History>;
  /** Applies one action to several entries, all or nothing; returns how many changed. */
  batch(ids: string[], action: BatchAction): Promise<number>;
  rename(id: string, change: { issuer?: string; account?: string }): Promise<void>;
  keyInfo(id: string): Promise<KeyInfo>;
  previewKey(id: string, form: KeyForm): Promise<string | null>;
  replaceKey(id: string, form: KeyForm): Promise<void>;
  setShortcut(id: string, shortcut: string | null): Promise<void>;
  stageText(text: string): Promise<ImportView>;
  stageFile(bytes: Uint8Array, backupPassword?: string): Promise<ImportView>;
  stageManual(form: ManualForm): Promise<ImportView>;
  stagedView(token: string): Promise<ImportView>;
  commit(token: string, fresh: boolean[], duplicates: boolean[]): Promise<number>;
  cancel(token: string): Promise<void>;
  openGate(): Promise<void>;
  /** Calls `listener` when the data changed elsewhere; returns how to stop. */
  onChange(listener: () => void): () => void;
  /** Calls `listener` when the panel or the tray asks for a page (`add`, `import`, `settings`). */
  onPage(listener: (page: string) => void): () => void;
}

/** Turns what a command threw into a {@link CommandFailure}. */
export function failure(thrown: unknown): CommandFailure {
  if (thrown instanceof CommandFailure) return thrown;
  const value = thrown as { code?: unknown; detail?: unknown } | null;
  if (value && typeof value.code === "string") {
    return new CommandFailure(value.code, typeof value.detail === "string" ? value.detail : undefined);
  }
  return new CommandFailure("UNKNOWN");
}

/** The core, through Tauri commands. */
export async function tauriMainApi(): Promise<MainApi> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  const call = async <T>(command: string, args?: Record<string, unknown>, options?: { headers: Record<string, string> }) => {
    try {
      return await invoke<T>(command, args, options);
    } catch (thrown) {
      throw failure(thrown);
    }
  };
  const subscribe = <T>(name: string, listener: (payload: T) => void) => {
    const stop = listen<T>(name, (event) => listener(event.payload));
    return () => void stop.then((unlisten) => unlisten());
  };
  return {
    state: (query) => call("main_state", { query }),
    copy: (id) => call("copy_code", { id }),
    nextHotp: (id) => call("next_hotp", { id }),
    edit: (id, change) => call("edit_entry", { id, title: change.title ?? null, note: change.note ?? null }),
    unlock: (password) => call("unlock", { password }),
    setPinned: (id, pinned) => call("set_pinned", { id, pinned }),
    setTags: (id, tags) => call("set_tags", { id, tags }),
    move: (id, to) => call("move_entry", { id, to }),
    trash: (id) => call("trash_entry", { id }),
    restore: (id) => call("restore_entry", { id }),
    purge: (id) => call("purge_entry", { id }),
    trashList: () => call("trash_list"),
    history: (id) => call("entry_history", { id }),
    batch: (ids, action) => call("batch_entries", { ids, action }),
    rename: (id, change) => call("rename_entry", { id, issuer: change.issuer ?? null, account: change.account ?? null }),
    keyInfo: (id) => call("key_info", { id }),
    previewKey: (id, form) => call("preview_key", { id, form }),
    replaceKey: (id, form) => call("replace_key", { id, form }),
    setShortcut: (id, shortcut) => call("set_shortcut", { id, shortcut }),
    stageText: (text) => call("stage_text", { text }),
    stageFile: (bytes, backupPassword) =>
      // Raw bytes; the password may hold any character, so it is percent-encoded for the header.
      call("stage_file", bytes as unknown as Record<string, unknown>, {
        headers: backupPassword === undefined ? {} : { "x-backup-password": encodeURIComponent(backupPassword) },
      }),
    stageManual: (form) => call("stage_manual", { form }),
    stagedView: (token) => call("staged_view", { token }),
    commit: (token, fresh, duplicates) => call("commit_import", { token, fresh, duplicates }),
    cancel: (token) => call("cancel_import", { token }),
    openGate: () => call("open_gate"),
    onChange: (listener) => subscribe("vault-changed", () => listener()),
    onPage: (listener) => subscribe<string>("open-page", listener),
  };
}
