import { render } from "preact";
import type { Settings } from "./main/api";
import { type PanelApi, insideApp, tauriApi } from "./panel/api";
import { demoApi } from "./panel/demo";
import { strings } from "./panel/i18n";
import { Panel } from "./panel/Panel";
import { applyTheme, languageOf } from "./shared/appearance";
import { isMacPlatform } from "./shared/shortcut";
import "./shared/tokens.css";
import "./shared/tiles.css";
import "./shared/ring.css";
import "./panel/panel.css";

const root = document.getElementById("panel");

/** Draws the panel with the person's theme and language. */
function show(api: PanelApi, settings?: Pick<Settings, "theme" | "language">) {
  if (!root) return;
  if (settings) applyTheme(settings.theme);
  const language = settings ? languageOf(settings.language, navigator.language) : navigator.language;
  render(<Panel api={api} text={strings(language)} isMac={isMacPlatform()} />, root);
}

if (root) {
  if (insideApp()) {
    // Settings changed in the main window reach the panel at once.
    void Promise.all([tauriApi(), import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]).then(
      async ([api, { invoke }, { listen }]) => {
        show(api, await invoke<Settings>("get_settings"));
        await listen<Settings>("settings-changed", (event) => show(api, event.payload));
      },
    );
  } else {
    // Outside the app (a browser preview) the panel shows made-up accounts.
    show(demoApi());
  }
}
