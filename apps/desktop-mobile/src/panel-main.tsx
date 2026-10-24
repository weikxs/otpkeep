import { render } from "preact";
import type { Settings } from "./main/api";
import { type PanelApi, insideApp, tauriApi } from "./panel/api";
import { demoApi } from "./panel/demo";
import { strings } from "./panel/i18n";
import { Panel } from "./panel/Panel";
import { applyTheme, languageOf } from "./shared/appearance";
import { type LanguageSetting, isLanguageSetting } from "./shared/LanguageSelect";
import { isMacPlatform } from "./shared/shortcut";
import "./shared/tokens.css";
import "./shared/tiles.css";
import "./shared/ring.css";
import "./shared/language.css";
import "./panel/panel.css";

type Shown = Pick<Settings, "theme" | "language" | "panelOpacity">;

const root = document.getElementById("panel");

/** Draws the panel with the person's theme, language and opacity; `choose` saves a new language. */
function show(api: PanelApi, settings: Shown | undefined, choose: (language: LanguageSetting) => void) {
  if (!root) return;
  if (settings) {
    applyTheme(settings.theme);
    document.documentElement.style.setProperty("--panel-opacity", `${settings.panelOpacity}%`);
  }
  const setting = settings && isLanguageSetting(settings.language) ? settings.language : "system";
  const language = languageOf(setting, navigator.language);
  render(
    <Panel
      api={api}
      text={strings(language)}
      isMac={isMacPlatform()}
      language={{ value: setting, onChange: choose }}
    />,
    root,
  );
}

if (root) {
  if (insideApp()) {
    // Settings changed in any window reach the panel at once, this one's language menu included.
    void Promise.all([tauriApi(), import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]).then(
      async ([api, { invoke }, { listen }]) => {
        const choose = (language: LanguageSetting) => void invoke("set_settings", { patch: { language } });
        show(api, await invoke<Settings>("get_settings"), choose);
        await listen<Settings>("settings-changed", (event) => show(api, event.payload, choose));
      },
    );
  } else {
    // Outside the app (a browser preview) the panel shows made-up accounts; the language is the page's own.
    const api = demoApi();
    const choose = (language: LanguageSetting) =>
      show(api, { theme: "system", language, panelOpacity: 100 }, choose);
    show(api, undefined, choose);
  }
}
