/** What the search panel asks of the core: the quick panel's commands, plus opening an entry in the main window. */

import type { PanelApi } from "../panel/api";

export interface SearchApi extends Pick<PanelApi, "state" | "copy" | "nextHotp" | "unlock" | "onChange" | "hide"> {
  /** Opens the main window, at `edit:<id>` to edit that entry. */
  openMain(page?: string): Promise<void>;
  /** Calls `listener` each time the search panel is shown; returns how to stop. */
  onShown(listener: () => void): () => void;
}

/** The core, through Tauri commands. */
export async function tauriSearchApi(): Promise<SearchApi> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  const subscribe = (name: string, listener: () => void) => {
    const stop = listen(name, () => listener());
    return () => void stop.then((unlisten) => unlisten());
  };
  return {
    state: (query) => invoke("panel_state", { query }),
    copy: (id) => invoke("copy_code", { id }),
    nextHotp: (id) => invoke("next_hotp", { id }),
    unlock: (password) => invoke("unlock", { password }),
    openMain: (page) => invoke<void>("open_main", { page: page ?? null }),
    // Hides the window that asks: here the search panel.
    hide: () => invoke<void>("hide_panel"),
    onChange: (listener) => subscribe("vault-changed", listener),
    onShown: (listener) => subscribe("search-shown", listener),
  };
}
