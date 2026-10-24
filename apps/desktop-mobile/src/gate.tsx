import { render } from "preact";
import { connect } from "./gate/api";
import { demoGate } from "./gate/demo";
import { GateApp } from "./gate/GateApp";
import { gateStrings } from "./gate/i18n";
import { isMacPlatform } from "./shared/shortcut";
import "./shared/tokens.css";
import "./shared/tiles.css";
import "./gate/gate.css";

const root = document.getElementById("app");
if (root) {
  // `?demo` previews the page with made-up accounts, without OTPKeep (development only).
  const preview = import.meta.env.DEV && new URLSearchParams(location.search).has("demo");
  const open = preview ? () => Promise.resolve(demoGate()) : () => connect(window);
  render(<GateApp connect={open} text={gateStrings(navigator.language)} isMac={isMacPlatform()} />, root);
}
