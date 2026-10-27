import { render } from "preact";
import type { Settings } from "./main/api";
import { type PanelApi, insideApp, tauriApi } from "./panel/api";
import { demoApi } from "./panel/demo";
import { PasswordPrompt } from "./panel/EntryList";
import { strings } from "./panel/i18n";
import { applyTheme, languageOf } from "./shared/appearance";
import "./shared/tokens.css";
import "./panel/panel.css";
import "./unlock/unlock.css";

type Shown = Pick<Settings, "theme" | "language">;

const root = document.getElementById("unlock");

/**
 * The password window (opened from the menu bar menu): asks for the master password. Once the data is open the
 * app closes the window for good, so nothing more happens here.
 */
function show(api: Pick<PanelApi, "unlock">, settings: Shown | undefined) {
  if (!root) return;
  if (settings) applyTheme(settings.theme);
  const language = languageOf(settings?.language ?? "system", navigator.language);
  render(<PasswordPrompt api={api} text={strings(language)} onOpened={() => undefined} />, root);
}

if (root) {
  if (insideApp()) {
    void Promise.all([tauriApi(), import("@tauri-apps/api/core")]).then(async ([api, { invoke }]) => {
      show(api, await invoke<Settings>("get_settings"));
    });
  } else {
    // Outside the app (a browser preview) the password "demo" opens the made-up data.
    show(demoApi({ status: "needsPassword" }), undefined);
  }
}
