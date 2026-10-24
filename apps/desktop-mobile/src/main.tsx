import { render } from "preact";
import { insideApp } from "./panel/api";
import { type MainApi, tauriMainApi } from "./main/api";
import { demoMain } from "./main/demo";
import { mainStrings } from "./main/i18n";
import { MainApp } from "./main/MainApp";
import { isMacPlatform } from "./shared/shortcut";
import "./shared/tokens.css";
import "./shared/tiles.css";
import "./shared/ring.css";
import "./main/main.css";

const root = document.getElementById("app");
if (root) {
  // Outside the app (a browser preview) the window shows made-up accounts.
  const load: Promise<MainApi> = insideApp() ? tauriMainApi() : Promise.resolve(demoMain());
  void load.then((api) =>
    render(<MainApp api={api} text={mainStrings(navigator.language)} isMac={isMacPlatform()} />, root),
  );
}
