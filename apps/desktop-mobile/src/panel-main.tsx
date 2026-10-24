import { render } from "preact";
import { type PanelApi, insideApp, tauriApi } from "./panel/api";
import { demoApi } from "./panel/demo";
import { strings } from "./panel/i18n";
import { Panel } from "./panel/Panel";
import "./shared/tokens.css";
import "./shared/tiles.css";
import "./shared/ring.css";
import "./panel/panel.css";

const root = document.getElementById("panel");
if (root) {
  // Outside the app (a browser preview) the panel shows made-up accounts.
  const load: Promise<PanelApi> = insideApp() ? tauriApi() : Promise.resolve(demoApi());
  const isMac = navigator.userAgent.includes("Mac");
  void load.then((api) => render(<Panel api={api} text={strings(navigator.language)} isMac={isMac} />, root));
}
