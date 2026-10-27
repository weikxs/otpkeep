/** What the search panel asks of the core: the shared panel commands, plus opening an entry in the main window. */

import type { PanelApi } from "../panel/api";

/** Whether codes can be typed into other apps here. */
export type Typing = "ready" | "needsPermission" | "unsupported";

export interface TypeResult {
  /** Being typed; the core has hidden the window already. */
  typed: boolean;
  /**
   * Why it was copied instead: no permission, not possible on this desktop, the main window is open (macOS
   * would hide it to give focus back), or no key could be sent.
   */
  fallback?: "NO_PERMISSION" | "UNSUPPORTED" | "MAIN_OPEN" | "FAILED";
  /** When copied: seconds until the clipboard is cleared. */
  clearsInS: number;
}

export interface SearchApi extends Pick<PanelApi, "state" | "copy" | "nextHotp" | "unlock" | "retryOpen" | "onChange" | "hide"> {
  /** Opens the main window, at `edit:<id>` to edit that entry. */
  openMain(page?: string): Promise<void>;
  /** Types the code into the app that had focus, or copies it where that is not possible. */
  typeCode(id: string): Promise<TypeResult>;
  /** Shows the system's request for the permission typing needs (macOS). */
  askTypingPermission(): Promise<Typing>;
  /** Calls `listener` each time the search panel is shown; returns how to stop. */
  onShown(listener: () => void): () => void;
  /** Calls `listener` when a code could not be typed after all and was copied instead; returns how to stop. */
  onTypingFailed(listener: (result: TypeResult) => void): () => void;
}

/** The core, through Tauri commands. */
export async function tauriSearchApi(): Promise<SearchApi> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  const subscribe = <T>(name: string, listener: (payload: T) => void) => {
    const stop = listen<T>(name, (event) => listener(event.payload));
    return () => void stop.then((unlisten) => unlisten());
  };
  return {
    state: (query) => invoke("panel_state", { query }),
    copy: (id) => invoke("copy_code", { id }),
    nextHotp: (id) => invoke("next_hotp", { id }),
    unlock: (password) => invoke("unlock", { password }),
    retryOpen: () => invoke<void>("retry_open"),
    typeCode: (id) => invoke("type_code", { id }),
    askTypingPermission: () => invoke("ask_typing_permission"),
    openMain: (page) => invoke<void>("open_main", { page: page ?? null }),
    // Hides the window that asks: here the search panel.
    hide: () => invoke<void>("hide_panel"),
    onChange: (listener) => subscribe("vault-changed", () => listener()),
    onShown: (listener) => subscribe("search-shown", () => listener()),
    onTypingFailed: (listener) => subscribe<TypeResult>("typing-failed", listener),
  };
}
