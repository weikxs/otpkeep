import { useState } from "preact/hooks";
import { CommandFailure, type ExportFormat, type GoogleExport, type LeftOut, type MainApi } from "./api";
import { type MainStrings, describe } from "./i18n";
import { GoogleCodes } from "./QrDialogs";

type Choice = ExportFormat | "google";

export interface ExportDialogProps {
  api: Pick<MainApi, "exportFile" | "exportGoogle">;
  text: MainStrings;
  /** The chosen entries, or `null` for all of them. */
  ids: string[] | null;
  count: number;
  onExported: (message: string) => void;
  onClose: () => void;
}

/** Choosing a format and exporting (PRD FR-6.1–6.3); plaintext formats are confirmed natively by the core. */
export function ExportDialog({ api, text, ids, count, onExported, onClose }: ExportDialogProps) {
  const [choice, setChoice] = useState<Choice>("backup");
  const [password, setPassword] = useState("");
  const [again, setAgain] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [leftOut, setLeftOut] = useState<LeftOut[] | null>(null);
  const [google, setGoogle] = useState<GoogleExport | null>(null);

  const go = async () => {
    if (choice === "backup" && password !== again) {
      setError(text.passwordsDiffer);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (choice === "google") {
        setGoogle(await api.exportGoogle(ids));
        return;
      }
      const done = await api.exportFile(choice, ids, choice === "backup" ? password : undefined);
      onExported(text.exported(done.count, done.fileName));
      if (done.leftOut.length > 0) setLeftOut(done.leftOut);
      else onClose();
    } catch (thrown) {
      const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
      // Cancelling the native question or the save panel is not an error.
      if (failure.code !== "CANCELLED") setError(describe(text, failure.code, failure.detail));
    } finally {
      setBusy(false);
      setPassword("");
      setAgain("");
    }
  };

  if (google) return <GoogleCodes codes={google} text={text} onClose={onClose} />;

  return (
    <div class="overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <section class="dialog" role="dialog" aria-modal="true" aria-label={text.exportTitle} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <header class="dialog-header">
          <h2>{text.exportTitle}</h2>
          <button type="button" class="icon-button" aria-label={text.close} onClick={onClose}>
            ×
          </button>
        </header>
        <p class="muted">{text.exportScope(count, ids === null)}</p>
        {leftOut ? (
          <div class="preview">
            <p class="preview-title">{text.leftOutTitle(leftOut.length)}</p>
            <ul class="skipped-list">
              {leftOut.map((l) => (
                <li key={l.title}>
                  {l.title}
                  {text.sep}
                  {text.leftReasons[l.reason] ?? l.reason}
                </li>
              ))}
            </ul>
            <div class="form-actions">
              <button type="button" class="primary" onClick={onClose}>
                {text.close}
              </button>
            </div>
          </div>
        ) : (
          <form
            class="export-form"
            onSubmit={(e) => {
              e.preventDefault();
              void go();
            }}
          >
            <div class="formats" role="radiogroup" aria-label={text.exportTitle}>
              {(["backup", "links", "google", "goldenPassport"] as const).map((f) => (
                <label key={f} class={`format${choice === f ? " active" : ""}`}>
                  <input type="radio" name="format" checked={choice === f} onChange={() => setChoice(f)} />
                  <span class="format-text">
                    <span class="format-name">{text.formats[f]}</span>
                    <span class="muted">{text.formatHints[f]}</span>
                  </span>
                </label>
              ))}
            </div>
            {choice === "backup" ? (
              <div class="advanced-grid two">
                <label>
                  <span class="field-label">{text.backupPasswordNew}</span>
                  <input type="password" value={password} autoComplete="new-password" onInput={(e) => setPassword((e.target as HTMLInputElement).value)} />
                </label>
                <label>
                  <span class="field-label">{text.backupPasswordAgain}</span>
                  <input type="password" value={again} autoComplete="new-password" onInput={(e) => setAgain((e.target as HTMLInputElement).value)} />
                </label>
                <p class="muted">{text.passwordHint}</p>
              </div>
            ) : (
              <p class="preview-note">{text.plaintextWarning}</p>
            )}
            {error && (
              <p class="field-error" role="alert">
                {error}
              </p>
            )}
            <div class="form-actions">
              <button type="submit" class="primary" disabled={busy || (choice === "backup" && password === "")}>
                {choice === "google" ? text.showCodes : text.exportGo}
              </button>
              <button type="button" class="secondary" onClick={onClose}>
                {text.cancel}
              </button>
            </div>
          </form>
        )}
      </section>
    </div>
  );
}
