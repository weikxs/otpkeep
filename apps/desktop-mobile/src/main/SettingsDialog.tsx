import { useEffect, useState } from "preact/hooks";
import { ShortcutField } from "../shared/ShortcutField";
import { formatShortcut } from "../shared/shortcut";
import type { Typing } from "../search/api";
import { CommandFailure, type MainApi, type Settings, type SettingsPatch } from "./api";
import { type MainStrings, describe } from "./i18n";

const CLEAR_CHOICES = [0, 10, 20, 30, 45, 60, 90, 120];
/** How many accounts the menu bar menu may show on its first level. */
const MENU_CHOICES = [10, 15, 20, 25, 30, 40, 50];

export interface SettingsDialogProps {
  api: Pick<MainApi, "settings" | "setSettings" | "setSearchShortcut" | "typingStatus" | "askTypingPermission">;
  text: MainStrings;
  isMac: boolean;
  /** Called with the saved settings, so the window can follow the theme and language. */
  onSaved: (settings: Settings) => void;
  onClose: () => void;
}

/** The settings; every change is saved at once (design/10-reference.md §2). */
export function SettingsDialog({ api, text, isMac, onSaved, onClose }: SettingsDialogProps) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [typing, setTyping] = useState<Typing | null>(null);

  useEffect(() => void api.settings().then(setSettings), [api]);
  useEffect(() => void api.typingStatus().then(setTyping), [api]);

  const save = async (saving: Promise<Settings>) => {
    setError(null);
    try {
      const saved = await saving;
      setSettings(saved);
      onSaved(saved);
    } catch (thrown) {
      const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
      setError(describe(text, failure.code, failure.detail));
    }
  };
  const change = (patch: SettingsPatch) => save(api.setSettings(patch));
  const value = (e: Event) => (e.target as HTMLSelectElement).value;

  return (
    <div class="overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <section class="dialog" role="dialog" aria-modal="true" aria-label={text.settingsTitle} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <header class="dialog-header">
          <h2>{text.settingsTitle}</h2>
          <button type="button" class="icon-button" aria-label={text.close} onClick={onClose}>
            ×
          </button>
        </header>
        {settings && (
          <div class="settings">
            <label class="setting">
              <span class="setting-name">{text.language}</span>
              <select value={settings.language} onChange={(e) => void change({ language: value(e) as Settings["language"] })}>
                {(["system", "zh", "en"] as const).map((l) => (
                  <option key={l} value={l}>
                    {text.languages[l]}
                  </option>
                ))}
              </select>
            </label>
            <label class="setting">
              <span class="setting-name">{text.theme}</span>
              <select value={settings.theme} onChange={(e) => void change({ theme: value(e) as Settings["theme"] })}>
                {(["system", "light", "dark"] as const).map((t) => (
                  <option key={t} value={t}>
                    {text.themes[t]}
                  </option>
                ))}
              </select>
            </label>
            <label class="setting">
              <span class="setting-name">
                {text.clipboard}
                <span class="muted">{text.clipboardHint}</span>
              </span>
              <select value={String(settings.clipboardClearS)} onChange={(e) => void change({ clipboardClearS: Number(value(e)) })}>
                {CLEAR_CHOICES.map((s) => (
                  <option key={s} value={String(s)}>
                    {s === 0 ? text.clipboardOff : text.clipboardAfter(s)}
                  </option>
                ))}
              </select>
            </label>
            <label class="setting">
              <span class="setting-name">
                {text.menuAccounts}
                <span class="muted">{text.menuAccountsHint}</span>
              </span>
              <select
                value={String(settings.menuAccounts)}
                aria-label={text.menuAccounts}
                onChange={(e) => void change({ menuAccounts: Number(value(e)) })}
              >
                {MENU_CHOICES.map((n) => (
                  <option key={n} value={String(n)}>
                    {n}
                  </option>
                ))}
              </select>
            </label>
            <label class="setting">
              <span class="setting-name">
                {text.rowShortcuts}
                <span class="muted">{text.rowShortcutsHint(isMac)}</span>
              </span>
              <input
                type="checkbox"
                class="switch"
                checked={settings.rowShortcuts}
                onChange={(e) => void change({ rowShortcuts: (e.target as HTMLInputElement).checked })}
              />
            </label>
            <div class="setting">
              <span class="setting-name">
                {text.searchShortcut}
                <span class="muted">{text.searchShortcutHint}</span>
              </span>
              <span class="setting-value">
                <ShortcutField
                  {...(settings.searchShortcut ? { shortcut: settings.searchShortcut } : {})}
                  isMac={isMac}
                  text={{ ...text.shortcutTexts, noShortcut: text.searchShortcutOff }}
                  onSave={(shortcut) => save(api.setSearchShortcut(shortcut))}
                />
                {settings.searchShortcutDefault && settings.searchShortcut !== settings.searchShortcutDefault && (
                  <button
                    type="button"
                    class="link-button"
                    onClick={() => void save(api.setSearchShortcut(settings.searchShortcutDefault))}
                  >
                    {text.searchShortcutReset(formatShortcut(settings.searchShortcutDefault, isMac))}
                  </button>
                )}
              </span>
            </div>
            {typing && (
              <div class="setting">
                <span class="setting-name">
                  {text.typing}
                  <span class="muted">{text.typingHint}</span>
                </span>
                <span class="setting-value">
                  <span class={typing === "ready" ? "muted" : "warn"}>{text.typingStatus[typing]}</span>
                  {typing === "needsPermission" && (
                    <button type="button" class="link-button" onClick={() => void api.askTypingPermission().then(setTyping)}>
                      {text.typingGrant}
                    </button>
                  )}
                </span>
              </div>
            )}
            <div class="setting about">
              <span class="setting-name">{text.about}</span>
              <span class="muted">{text.version(settings.version)}</span>
            </div>
          </div>
        )}
        {error && (
          <p class="field-error" role="alert">
            {error}
          </p>
        )}
      </section>
    </div>
  );
}
