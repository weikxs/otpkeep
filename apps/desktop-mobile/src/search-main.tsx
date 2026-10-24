import { render } from "preact";
import type { Settings } from "./main/api";
import { insideApp } from "./panel/api";
import { strings } from "./panel/i18n";
import { type SearchApi, tauriSearchApi } from "./search/api";
import { demoSearchApi } from "./search/demo";
import { searchStrings } from "./search/i18n";
import { SearchApp } from "./search/SearchApp";
import { applyTheme, languageOf } from "./shared/appearance";
import { type LanguageSetting, isLanguageSetting } from "./shared/LanguageSelect";
import { isMacPlatform } from "./shared/shortcut";
import "./shared/tokens.css";
import "./shared/tiles.css";
import "./shared/ring.css";
import "./shared/language.css";
import "./panel/panel.css";
import "./search/search.css";

type Shown = Pick<Settings, "theme" | "language">;

const root = document.getElementById("search");

/** Draws the search panel with the person's theme and language; `choose` saves a new language. */
function show(api: SearchApi, settings: Shown | undefined, choose: (language: LanguageSetting) => void) {
  if (!root) return;
  if (settings) applyTheme(settings.theme);
  const setting = settings && isLanguageSetting(settings.language) ? settings.language : "system";
  const language = languageOf(setting, navigator.language);
  render(
    <SearchApp
      api={api}
      text={strings(language)}
      words={searchStrings(language)}
      isMac={isMacPlatform()}
      language={{ value: setting, onChange: choose }}
    />,
    root,
  );
}

if (root) {
  if (insideApp()) {
    void Promise.all([tauriSearchApi(), import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]).then(
      async ([api, { invoke }, { listen }]) => {
        const choose = (language: LanguageSetting) => void invoke("set_settings", { patch: { language } });
        show(api, await invoke<Settings>("get_settings"), choose);
        await listen<Settings>("settings-changed", (event) => show(api, event.payload, choose));
      },
    );
  } else {
    // Outside the app (a browser preview) it shows made-up accounts; the language is the page's own.
    const api = demoSearchApi();
    const choose = (language: LanguageSetting) => show(api, { theme: "system", language }, choose);
    show(api, undefined, choose);
  }
}
