import { useEffect, useRef, useState } from "preact/hooks";
import { CommandFailure, type ImportView, type MainApi, type ManualForm } from "./api";
import { type MainStrings, describe } from "./i18n";
import { ImportPreview } from "./ImportPreview";
import { ManualFormView } from "./ManualFormView";

export type AddTab = "paste" | "file" | "manual";

type Step =
  | { kind: "input" }
  | { kind: "password"; bytes: Uint8Array }
  | { kind: "preview"; view: ImportView };

export interface AddDialogProps {
  api: MainApi;
  text: MainStrings;
  tab: AddTab;
  /** A file dropped on the window, read straight away. */
  dropped?: Uint8Array;
  onAdded: (count: number) => void;
  onClose: () => void;
}

/** Adding accounts: paste a link, choose a picture or file, or type it in; then preview and choose. */
export function AddDialog({ api, text, tab: initialTab, dropped, onAdded, onClose }: AddDialogProps) {
  const [tab, setTab] = useState<AddTab>(initialTab);
  const [step, setStep] = useState<Step>({ kind: "input" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pasted, setPasted] = useState("");
  const [password, setPassword] = useState("");
  const token = useRef<string | null>(null);

  const explain = (thrown: unknown) => {
    const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
    return describe(text, failure.code, failure.detail);
  };

  const run = async (stage: () => Promise<ImportView>, onFailure?: (failure: CommandFailure) => boolean) => {
    setBusy(true);
    setError(null);
    try {
      const view = await stage();
      token.current = view.token;
      setStep({ kind: "preview", view });
    } catch (thrown) {
      const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
      if (!onFailure?.(failure)) setError(explain(failure));
    } finally {
      setBusy(false);
    }
  };

  const readBytes = (bytes: Uint8Array) =>
    run(
      () => api.stageFile(bytes),
      (failure) => {
        if (failure.code !== "BACKUP_PASSWORD_NEEDED") return false;
        setStep({ kind: "password", bytes });
        return true;
      },
    );

  const readFile = async (file: File) => readBytes(new Uint8Array(await file.arrayBuffer()));

  // Read the dropped file once, when the dialog opens with it.
  const started = useRef(false);
  useEffect(() => {
    if (dropped && !started.current) {
      started.current = true;
      void readBytes(dropped);
    }
  });

  const close = () => {
    if (token.current) void api.cancel(token.current);
    onClose();
  };

  const back = () => {
    if (token.current) void api.cancel(token.current);
    token.current = null;
    setStep({ kind: "input" });
  };

  const commit = async (view: ImportView, fresh: boolean[], duplicates: boolean[]) => {
    setBusy(true);
    setError(null);
    try {
      const added = await api.commit(view.token, fresh, duplicates);
      token.current = null;
      onAdded(added);
    } catch (thrown) {
      const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
      setError(explain(failure));
      if (failure.code === "IMPORT_CHANGED") {
        setStep({ kind: "preview", view: await api.stagedView(view.token) });
      }
    } finally {
      setBusy(false);
    }
  };

  /** A screenshot pasted anywhere in the dialog is read as a picture. */
  const onPaste = (event: ClipboardEvent) => {
    const file = Array.from(event.clipboardData?.files ?? []).find((f) => f.type.startsWith("image/"));
    if (!file) return;
    event.preventDefault();
    void readFile(file);
  };

  return (
    <div class="overlay" onClick={(e) => e.target === e.currentTarget && close()}>
      <section
        class="dialog"
        role="dialog"
        aria-modal="true"
        aria-label={text.addTitle}
        onPaste={onPaste}
        onKeyDown={(e) => e.key === "Escape" && close()}
      >
        <header class="dialog-header">
          <h2>{text.addTitle}</h2>
          <button type="button" class="icon-button" aria-label={text.close} onClick={close}>
            ×
          </button>
        </header>
        {step.kind === "preview" ? (
          <ImportPreview
            view={step.view}
            text={text}
            busy={busy}
            onCommit={(fresh, duplicates) => void commit(step.view, fresh, duplicates)}
            onBack={back}
          />
        ) : step.kind === "password" ? (
          <form
            class="password-step"
            onSubmit={(e) => {
              e.preventDefault();
              void run(() => api.stageFile(step.bytes, password));
            }}
          >
            <p class="preview-title">{text.backupTitle}</p>
            <p class="muted">{text.backupBody}</p>
            <input
              type="password"
              value={password}
              aria-label={text.backupPassword}
              placeholder={text.backupPassword}
              autoFocus
              onInput={(e) => setPassword((e.target as HTMLInputElement).value)}
            />
            <div class="form-actions">
              <button type="submit" class="primary" disabled={busy || password === ""}>
                {text.open}
              </button>
              <button type="button" class="secondary" onClick={back}>
                {text.back}
              </button>
            </div>
          </form>
        ) : (
          <>
            <nav class="tabs" role="tablist">
              {(["paste", "file", "manual"] as const).map((t) => (
                <button
                  key={t}
                  type="button"
                  role="tab"
                  aria-selected={tab === t}
                  class={tab === t ? "tab active" : "tab"}
                  onClick={() => {
                    setTab(t);
                    setError(null);
                  }}
                >
                  {t === "paste" ? text.tabPaste : t === "file" ? text.tabFile : text.tabManual}
                </button>
              ))}
            </nav>
            {tab === "paste" && (
              <form
                class="paste-form"
                onSubmit={(e) => {
                  e.preventDefault();
                  void run(() => api.stageText(pasted));
                }}
              >
                <textarea
                  value={pasted}
                  rows={6}
                  class="mono"
                  aria-label={text.tabPaste}
                  placeholder={text.pastePlaceholder}
                  autoFocus
                  onInput={(e) => setPasted((e.target as HTMLTextAreaElement).value)}
                />
                <div class="form-actions">
                  <button type="submit" class="primary" disabled={busy || pasted.trim() === ""}>
                    {busy ? text.reading : text.read}
                  </button>
                </div>
              </form>
            )}
            {tab === "file" && (
              <div class="file-step">
                <label class="file-drop">
                  <input
                    type="file"
                    accept="image/*,.otpkeep,.okv,.txt,.json,.secrets"
                    aria-label={text.chooseFile}
                    onChange={(e) => {
                      const file = (e.target as HTMLInputElement).files?.[0];
                      if (file) void readFile(file);
                    }}
                  />
                  <span class="primary-like">{busy ? text.reading : text.chooseFile}</span>
                  <span class="muted">{text.fileHint}</span>
                  <span class="muted">{text.dropHint}</span>
                </label>
              </div>
            )}
            {tab === "manual" && (
              <ManualFormView
                text={text}
                busy={busy}
                error={null}
                onPreview={(form: ManualForm) => void run(() => api.stageManual(form))}
              />
            )}
          </>
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
