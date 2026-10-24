import { render } from "preact";
import { insideApp } from "./panel/api";
import { type MainApi, tauriMainApi } from "./main/api";
import { demoMain } from "./main/demo";
import { Root } from "./main/Root";
import { isMacPlatform } from "./shared/shortcut";
import "./shared/tokens.css";
import "./shared/tiles.css";
import "./shared/ring.css";
import "./shared/language.css";
import "./main/main.css";

const root = document.getElementById("app");
if (root) {
  // Outside the app (a browser preview) the window shows made-up accounts.
  const load: Promise<MainApi> = insideApp() ? tauriMainApi() : Promise.resolve(demoMain());
  void load.then((api) =>
    render(<Root api={api} systemLanguage={navigator.language} isMac={isMacPlatform()} />, root),
  );
}
