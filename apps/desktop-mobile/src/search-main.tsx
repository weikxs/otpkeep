import { render } from "preact";
import type { Settings } from "./main/api";
import { insideApp } from "./panel/api";
import { demoApi } from "./panel/demo";
import { strings } from "./panel/i18n";
import { type SearchApi, tauriSearchApi } from "./search/api";
import { searchStrings } from "./search/i18n";
import { SearchApp } from "./search/SearchApp";
import { applyTheme, languageOf } from "./shared/appearance";
import { isMacPlatform } from "./shared/shortcut";
import "./shared/tokens.css";
import "./shared/tiles.css";
import "./shared/ring.css";
import "./panel/panel.css";
import "./search/search.css";

const root = document.getElementById("search");

/** Draws the search panel with the person's theme and language. */
function show(api: SearchApi, settings?: Pick<Settings, "theme" | "language">) {
  if (!root) return;
  if (settings) applyTheme(settings.theme);
  const language = settings ? languageOf(settings.language, navigator.language) : navigator.language;
  render(
    <SearchApp api={api} text={strings(language)} words={searchStrings(language)} isMac={isMacPlatform()} />,
    root,
  );
}

if (root) {
  if (insideApp()) {
    void Promise.all([tauriSearchApi(), import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]).then(
      async ([api, { invoke }, { listen }]) => {
        show(api, await invoke<Settings>("get_settings"));
        await listen<Settings>("settings-changed", (event) => show(api, event.payload));
      },
    );
  } else {
    // Outside the app (a browser preview) it shows made-up accounts.
    show({ ...demoApi(), onShown: () => () => undefined });
  }
}
