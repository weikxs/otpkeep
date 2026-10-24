/**
 * What the quick panel asks of the core. The panel never receives secrets: only display fields and the
 * current and next codes (design/12 §6).
 */

export type EntryKind = "totp" | "hotp";

export interface PanelEntry {
  id: string;
  title: string;
  issuer: string;
  account: string;
  kind: EntryKind;
  pinned: boolean;
  /** Current code; for HOTP only after "generate". */
  code?: string;
  /** The code after this one, when the current one ends within five seconds. */
  nextCode?: string;
  /** TOTP period in seconds. */
  periodS?: number;
  /** When the current TOTP code ends, in the core's clock (Unix milliseconds). */
  validUntilMs?: number;
  /** 1–10: the global shortcut digit (10 means 0). */
  shortcut?: number;
}

/** There is no app lock (ADR-019): a master password is asked for once per run. */
export type PanelStatus = "ready" | "needsPassword" | "unavailable";

export type Problem =
  | "DATA_MISSING"
  | "KEY_MISSING"
  | "DAMAGED"
  | "NEWER_FORMAT"
  | "KEYSTORE_UNAVAILABLE"
  | "BUSY"
  | "IO"
  | "NO_DATA_DIR";

export interface PanelState {
  status: PanelStatus;
  entries: PanelEntry[];
  /** The core's clock when the state was made, to align countdowns. */
  nowMs: number;
  /** The vault is protected by a master password (otherwise by the system keychain). */
  usesMasterPassword: boolean;
  /** Why the vault cannot be opened, when unavailable. */
  problem?: Problem;
  /** This device is in a sync group. */
  inGroup: boolean;
  /** A newer OTPKeep uses the data, so nothing may be changed. */
  readOnly: boolean;
}

export interface CopyResult {
  /** Seconds until the clipboard is cleared, or 0 when it is not cleared. */
  clearsInS: number;
}

export interface PanelApi {
  state(query: string): Promise<PanelState>;
  copy(id: string): Promise<CopyResult>;
  nextHotp(id: string): Promise<PanelEntry>;
  unlock(password: string): Promise<PanelState>;
  openMain(page?: "add" | "import" | "settings"): Promise<void>;
  hide(): Promise<void>;
}

/** The core, through Tauri commands. */
export async function tauriApi(): Promise<PanelApi> {
  const { invoke } = await import("@tauri-apps/api/core");
  return {
    state: (query) => invoke<PanelState>("panel_state", { query }),
    copy: (id) => invoke<CopyResult>("copy_code", { id }),
    nextHotp: (id) => invoke<PanelEntry>("next_hotp", { id }),
    unlock: (password) => invoke<PanelState>("unlock", { password }),
    openMain: (page) => invoke<void>("open_main", { page: page ?? null }),
    hide: () => invoke<void>("hide_panel"),
  };
}

/** Whether the page runs inside the app (rather than a browser preview). */
export function insideApp(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
