import { useEffect, useState } from "preact/hooks";
import { groupCode } from "../panel/format";
import { ShortcutField } from "../shared/ShortcutField";
import { CommandFailure, type Entry, type KeyForm, type KeyInfo, type MainApi, type QrImage } from "./api";
import { SingleQr } from "./QrDialogs";
import { type MainStrings, describe } from "./i18n";

export interface AdvancedSectionProps {
  entry: Entry;
  text: MainStrings;
  isMac: boolean;
  api: Pick<MainApi, "keyInfo" | "previewKey" | "replaceKey" | "setShortcut" | "entryQr">;
  onChanged: (message: string) => void;
}

/** The entry's own shortcut and its code settings, each saved on its own (PRD FR-4.4, FR-5.2). */
export function AdvancedSection({ entry, text, isMac, api, onChanged }: AdvancedSectionProps) {
  const [info, setInfo] = useState<KeyInfo | null>(null);
  const [editing, setEditing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [qr, setQr] = useState<QrImage | null>(null);

  const showQr = async () => {
    try {
      setQr(await api.entryQr(entry.id));
    } catch (thrown) {
      // Saying no to the native question is not an error.
      if (!(thrown instanceof CommandFailure && thrown.code === "CANCELLED")) setError(explain(thrown));
    }
  };

  const explain = (thrown: unknown) => {
    const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
    return describe(text, failure.code, failure.detail);
  };

  // An entry removed meanwhile (on another device, or a list drawn from older data) has no settings to show.
  useEffect(() => {
    api.keyInfo(entry.id).then(setInfo, (thrown: unknown) => setError(explain(thrown)));
  }, [api, entry.id]);

  const saveShortcut = async (shortcut: string | null) => {
    try {
      await api.setShortcut(entry.id, shortcut);
      setError(null);
      onChanged(text.shortcutSaved);
    } catch (thrown) {
      setError(explain(thrown));
    }
  };

  return (
    <div class="advanced-section">
      <div class="field">
        <span class="field-label">{text.shortcut}</span>
        <ShortcutField
          {...(entry.ownShortcut === undefined ? {} : { shortcut: entry.ownShortcut })}
          {...(entry.ownShortcut === undefined && entry.shortcut !== undefined ? { defaultShortcut: entry.shortcut } : {})}
          inactive={entry.ownShortcut !== undefined && entry.shortcut !== entry.ownShortcut}
          isMac={isMac}
          text={text.shortcutTexts}
          onSave={saveShortcut}
        />
      </div>
      <div class="field">
        <span class="field-label">{text.codeSettings}</span>
        {info && !editing && (
          <span class="key-summary">
            <span>{text.keySummary(text.kinds[info.kind], info.algorithm, info.digits, info.periodS, info.counter)}</span>
            <button type="button" class="link-button" onClick={() => setEditing(true)}>
              {text.changeKey}
            </button>
          </span>
        )}
        {info?.shortSecret && <span class="preview-note">{text.shortSecret}</span>}
        <button type="button" class="link-button align-start" onClick={() => void showQr()}>
          {text.showQr}
        </button>
        {info && editing && (
          <KeyEditor
            info={info}
            text={text}
            preview={(form) => api.previewKey(entry.id, form)}
            save={async (form) => {
              await api.replaceKey(entry.id, form);
              setInfo(await api.keyInfo(entry.id));
              setEditing(false);
              onChanged(text.keySaved);
            }}
            onCancel={() => setEditing(false)}
          />
        )}
      </div>
      {error && (
        <p class="field-error" role="alert">
          {error}
        </p>
      )}
      {qr && <SingleQr qr={qr} title={entry.title} text={text} onClose={() => setQr(null)} />}
    </div>
  );
}

interface KeyEditorProps {
  info: KeyInfo;
  text: MainStrings;
  preview: (form: KeyForm) => Promise<string | null>;
  save: (form: KeyForm) => Promise<void>;
  onCancel: () => void;
}

function KeyEditor({ info, text, preview, save, onCancel }: KeyEditorProps) {
  const [form, setForm] = useState<KeyForm>({
    secret: "",
    kind: info.kind,
    algorithm: (["SHA1", "SHA256", "SHA512"].includes(info.algorithm) ? info.algorithm : "SHA1") as KeyForm["algorithm"],
    digits: info.kind === "steam" ? 6 : info.digits,
    periodS: info.periodS ?? 30,
    counter: info.counter ?? 0,
  });
  const [sample, setSample] = useState<string | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const set = <K extends keyof KeyForm>(name: K, value: KeyForm[K]) => {
    setForm({ ...form, [name]: value });
    setSample(undefined);
  };
  const value = (e: Event) => (e.target as HTMLInputElement).value;

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (thrown) {
      const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
      setError(describe(text, failure.code, failure.detail));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="key-editor">
      <div class="advanced-grid">
        <label>
          <span class="field-label">{text.kind}</span>
          <select value={form.kind} onChange={(e) => set("kind", value(e) as KeyForm["kind"])}>
            {(["totp", "hotp", "steam"] as const).map((k) => (
              <option key={k} value={k}>
                {text.kinds[k]}
              </option>
            ))}
          </select>
        </label>
        {form.kind !== "steam" && (
          <>
            <label>
              <span class="field-label">{text.algorithm}</span>
              <select value={form.algorithm} onChange={(e) => set("algorithm", value(e) as KeyForm["algorithm"])}>
                {(["SHA1", "SHA256", "SHA512"] as const).map((a) => (
                  <option key={a} value={a}>
                    {a}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span class="field-label">{text.digits}</span>
              <select value={String(form.digits)} onChange={(e) => set("digits", Number(value(e)))}>
                {[6, 7, 8].map((d) => (
                  <option key={d} value={String(d)}>
                    {d}
                  </option>
                ))}
              </select>
            </label>
          </>
        )}
        {form.kind === "hotp" ? (
          <label>
            <span class="field-label">{text.counter}</span>
            <input type="number" min={0} value={form.counter} onInput={(e) => set("counter", Number(value(e)))} />
          </label>
        ) : (
          <label>
            <span class="field-label">{text.period}</span>
            <input type="number" min={1} max={300} value={form.periodS} onInput={(e) => set("periodS", Number(value(e)))} />
          </label>
        )}
      </div>
      <label>
        <span class="field-label">{text.newSecret}</span>
        <input
          class="mono"
          value={form.secret}
          autoComplete="off"
          spellcheck={false}
          placeholder={text.newSecretPlaceholder}
          onInput={(e) => set("secret", value(e))}
        />
      </label>
      {sample !== undefined && <p class="sample-line">{sample === null ? text.keyPreviewCounter : text.keyPreview(groupCode(sample))}</p>}
      {form.secret.trim() !== "" && <p class="preview-note">{text.replaceWarning}</p>}
      {error && (
        <p class="field-error" role="alert">
          {error}
        </p>
      )}
      <div class="form-actions">
        <button type="button" class="secondary small" disabled={busy} onClick={() => void run(async () => setSample(await preview(form)))}>
          {text.preview}
        </button>
        <button type="button" class="primary small" disabled={busy} onClick={() => void run(() => save(form))}>
          {text.save}
        </button>
        <button type="button" class="secondary small" onClick={onCancel}>
          {text.cancel}
        </button>
      </div>
    </div>
  );
}
