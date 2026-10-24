import { useState } from "preact/hooks";
import { AdvancedSection } from "./AdvancedSection";
import { CommandFailure, type Entry, type History, type MainApi, type MoveTo } from "./api";
import { type MainStrings, describe } from "./i18n";
import { TagInput } from "./TagInput";

/** The changes the editor's Save button makes. */
export interface EntryChange {
  title?: string;
  note?: string;
  issuer?: string;
  account?: string;
  tags?: string[];
  pinned?: boolean;
}

export interface EntryEditorProps {
  entry: Entry;
  text: MainStrings;
  isMac: boolean;
  keyApi: Pick<MainApi, "keyInfo" | "previewKey" | "replaceKey" | "setShortcut" | "entryQr">;
  onChanged: (message: string) => void;
  /** The entries just above and below, for moving one place. */
  previous: string | undefined;
  next: string | undefined;
  onSave: (change: EntryChange) => Promise<void>;
  onMove: (to: MoveTo) => Promise<void>;
  onTrash: () => Promise<void>;
  loadHistory: () => Promise<History>;
  onDone: () => void;
}

/** Everything about one entry that is not its secret: names, note, tags, pin, place, history, trash. */
export function EntryEditor(props: EntryEditorProps) {
  const { entry, text, previous, next, onSave, onMove, onTrash, loadHistory, onDone } = props;
  const [title, setTitle] = useState(entry.title);
  const [issuer, setIssuer] = useState(entry.issuer);
  const [account, setAccount] = useState(entry.account);
  const [note, setNote] = useState(entry.note);
  const [tags, setTags] = useState(entry.tags);
  const [pinned, setPinned] = useState(entry.pinned);
  const [history, setHistory] = useState<History | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const attempt = async (action: () => Promise<void>, after?: () => void) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      after?.();
    } catch (thrown) {
      const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
      setError(describe(text, failure.code, failure.detail));
    } finally {
      setBusy(false);
    }
  };

  const submit = (event: Event) => {
    event.preventDefault();
    const change = {
      ...(title !== entry.title ? { title } : {}),
      ...(note !== entry.note ? { note } : {}),
      ...(issuer !== entry.issuer ? { issuer } : {}),
      ...(account !== entry.account ? { account } : {}),
      ...(tags.join("\n") !== entry.tags.join("\n") ? { tags } : {}),
      ...(pinned !== entry.pinned ? { pinned } : {}),
    };
    if (Object.keys(change).length === 0) return onDone();
    void attempt(() => onSave(change), onDone);
  };

  return (
    <form class="edit-form" onSubmit={submit}>
      <label>
        <span class="field-label">{text.name}</span>
        <input value={title} maxLength={100} autoFocus onInput={(e) => setTitle((e.target as HTMLInputElement).value)} />
      </label>
      <label>
        <span class="field-label">{text.note}</span>
        <textarea
          value={note}
          rows={3}
          maxLength={2000}
          placeholder={text.notePlaceholder}
          onInput={(e) => setNote((e.target as HTMLTextAreaElement).value)}
        />
      </label>
      <div class="field">
        <span class="field-label">{text.tags}</span>
        <TagInput tags={tags} label={text.tags} placeholder={text.tagPlaceholder} removeLabel={text.removeTag} onChange={setTags} />
      </div>
      <label class="check">
        <input type="checkbox" checked={pinned} onChange={(e) => setPinned((e.target as HTMLInputElement).checked)} />
        {text.pin}
      </label>
      <div class="field">
        <span class="field-label">{text.order}</span>
        <div class="order-buttons">
          <button type="button" class="secondary small" disabled={busy || previous === undefined} onClick={() => void attempt(() => onMove({ to: "first" }))}>
            {text.moveTop}
          </button>
          <button
            type="button"
            class="secondary small"
            disabled={busy || previous === undefined}
            onClick={() => previous && void attempt(() => onMove({ to: "before", other: previous }))}
          >
            {text.moveUp}
          </button>
          <button
            type="button"
            class="secondary small"
            disabled={busy || next === undefined}
            onClick={() => next && void attempt(() => onMove({ to: "after", other: next }))}
          >
            {text.moveDown}
          </button>
          <button type="button" class="secondary small" disabled={busy || next === undefined} onClick={() => void attempt(() => onMove({ to: "last" }))}>
            {text.moveBottom}
          </button>
        </div>
      </div>
      <details class="history">
        <summary>{text.advancedEdit}</summary>
        <div class="advanced-grid two">
          <label>
            <span class="field-label">{text.issuer}</span>
            <input value={issuer} maxLength={256} onInput={(e) => setIssuer((e.target as HTMLInputElement).value)} />
          </label>
          <label>
            <span class="field-label">{text.account}</span>
            <input value={account} maxLength={256} onInput={(e) => setAccount((e.target as HTMLInputElement).value)} />
          </label>
        </div>
        <AdvancedSection entry={entry} text={text} isMac={props.isMac} api={props.keyApi} onChanged={props.onChanged} />
      </details>
      <details
        class="history"
        onToggle={(e) => {
          if ((e.target as HTMLDetailsElement).open && history === null) void loadHistory().then(setHistory);
        }}
      >
        <summary>{text.history}</summary>
        {history && history.title.length + history.note.length === 0 && <p class="muted">{text.noHistory}</p>}
        {history && (
          <ul class="versions">
            {history.title.map((v) => (
              <li key={`t${v.atMs}${v.value}`}>
                <span class="muted">{text.name} · {text.when(v.atMs)}</span>
                <span class="version-value">{v.value}</span>
                <button type="button" class="link-button" onClick={() => setTitle(v.value)}>
                  {text.useVersion}
                </button>
              </li>
            ))}
            {history.note.map((v) => (
              <li key={`n${v.atMs}${v.value}`}>
                <span class="muted">{text.note} · {text.when(v.atMs)}</span>
                <span class="version-value">{v.value === "" ? text.emptyValue : v.value}</span>
                <button type="button" class="link-button" onClick={() => setNote(v.value)}>
                  {text.useVersion}
                </button>
              </li>
            ))}
          </ul>
        )}
      </details>
      {error && (
        <p class="field-error" role="alert">
          {error}
        </p>
      )}
      <div class="form-actions">
        <button type="submit" class="primary" disabled={busy}>
          {text.save}
        </button>
        <button type="button" class="secondary" onClick={onDone}>
          {text.cancel}
        </button>
        <span class="spacer" />
        {confirming ? (
          <span class="confirm">
            <span class="muted">{text.confirmTrash}</span>
            <button type="button" class="danger" disabled={busy} onClick={() => void attempt(onTrash, onDone)}>
              {text.trash}
            </button>
            <button type="button" class="secondary" onClick={() => setConfirming(false)}>
              {text.cancel}
            </button>
          </span>
        ) : (
          <button type="button" class="link-button danger-link" onClick={() => setConfirming(true)}>
            {text.trash}
          </button>
        )}
      </div>
    </form>
  );
}
