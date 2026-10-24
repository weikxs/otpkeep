import { render } from "preact";
import { useCallback, useMemo, useRef, useState } from "preact/hooks";
import { type GateApi, connect } from "./gate/api";
import { demoGate } from "./gate/demo";
import { GateApp } from "./gate/GateApp";
import { gateStrings } from "./gate/i18n";
import { languageOf } from "./shared/appearance";
import { type LanguageSetting, isLanguageSetting } from "./shared/LanguageSelect";
import { isMacPlatform } from "./shared/shortcut";
import "./shared/tokens.css";
import "./shared/tiles.css";
import "./shared/ring.css";
import "./shared/language.css";
import "./gate/gate.css";

/**
 * The page in OTPKeep's language setting: the browser's language until OTPKeep answers, then the setting it
 * shares with its windows. A change here is saved in OTPKeep, so its windows follow too.
 */
function GateRoot({ open }: { open: () => Promise<GateApi> }) {
  const [setting, setSetting] = useState<LanguageSetting>("system");
  const api = useRef<GateApi | null>(null);

  const connectAndRead = useCallback(async () => {
    const connected = await open();
    api.current = connected;
    // Reading the setting is a nicety: the page works in the browser's language without it.
    const read: string = await connected.language().catch(() => "system");
    setSetting(isLanguageSetting(read) ? read : "system");
    return connected;
  }, [open]);

  const choose = useCallback((next: LanguageSetting) => {
    setSetting(next);
    void api.current?.setLanguage(next).then(setSetting, () => {});
  }, []);

  const text = useMemo(() => gateStrings(languageOf(setting, navigator.language)), [setting]);
  return <GateApp connect={connectAndRead} text={text} isMac={isMacPlatform()} language={{ value: setting, onChange: choose }} />;
}

const root = document.getElementById("app");
if (root) {
  // `?demo` previews the page with made-up accounts, without OTPKeep (development only).
  const preview = import.meta.env.DEV && new URLSearchParams(location.search).has("demo");
  const open = preview ? () => Promise.resolve(demoGate()) : () => connect(window);
  render(<GateRoot open={open} />, root);
}
