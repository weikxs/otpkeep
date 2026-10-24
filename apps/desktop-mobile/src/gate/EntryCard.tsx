import { useState } from "preact/hooks";
import { monogram, tileColor } from "../panel/format";
import { type EntryEdit, GateFailure, type GateEntry } from "./api";
import { type GateStrings, describeFailure } from "./i18n";
import { ShortcutField } from "../shared/ShortcutField";

export interface EntryCardProps {
  entry: GateEntry;
  text: GateStrings;
  isMac: boolean;
  onEdit: (edit: EntryEdit) => Promise<GateEntry>;
  onShortcut: (shortcut: string | null) => Promise<GateEntry>;
  onNotice: (message: string) => void;
}

/** One account: its names, its note and its shortcut, each editable in place. */
export function EntryCard({ entry, text, isMac, onEdit, onShortcut, onNotice }: EntryCardProps) {
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(entry.title);
  const [note, setNote] = useState(entry.note);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const subtitle = [entry.issuer, entry.account].filter((s) => s !== "" && s !== entry.title).join(" · ");

  const explain = (failure: unknown) =>
    failure instanceof GateFailure ? describeFailure(text, failure.code, failure.detail) : text.errors.UNKNOWN ?? "";

  const startEditing = () => {
    setTitle(entry.title);
    setNote(entry.note);
    setError(null);
    setEditing(true);
  };

  const save = async (event: Event) => {
    event.preventDefault();
    const edit: EntryEdit = {
      ...(title !== entry.title ? { title } : {}),
      ...(note !== entry.note ? { note } : {}),
    };
    if (Object.keys(edit).length === 0) {
      setEditing(false);
      return;
    }
    setBusy(true);
    try {
      await onEdit(edit);
      setEditing(false);
      setError(null);
      onNotice(text.saved);
    } catch (failure) {
      setError(explain(failure));
    } finally {
      setBusy(false);
    }
  };

  const saveShortcut = async (shortcut: string | null) => {
    try {
      await onShortcut(shortcut);
      setError(null);
      onNotice(shortcut === null ? text.shortcutCleared : text.shortcutSaved);
    } catch (failure) {
      setError(explain(failure));
    }
  };

  return (
    <li class="card" data-testid="card">
      <span class={`tile tile-${tileColor(entry.issuer || entry.title)}`} aria-hidden="true">
        {monogram(entry.issuer, entry.title)}
      </span>
      <div class="card-body">
        {editing ? (
          <form class="edit-form" onSubmit={(e) => void save(e)}>
            <label>
              <span class="field-label">{text.name}</span>
              <input
                value={title}
                maxLength={100}
                autoFocus
                onInput={(e) => setTitle((e.target as HTMLInputElement).value)}
              />
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
            <div class="form-actions">
              <button type="submit" class="primary" disabled={busy}>
                {text.save}
              </button>
              <button type="button" class="secondary" onClick={() => setEditing(false)}>
                {text.cancel}
              </button>
            </div>
          </form>
        ) : (
          <>
            <div class="card-head">
              <span class="card-title">{entry.title}</span>
              <button type="button" class="link-button" onClick={startEditing}>
                {text.edit}
              </button>
            </div>
            {subtitle !== "" && <div class="card-subtitle">{subtitle}</div>}
            {entry.note !== "" && <p class="card-note">{entry.note}</p>}
          </>
        )}
        <div class="card-shortcut">
          <span class="field-label">{text.shortcut}</span>
          <ShortcutField
            {...(entry.shortcut === undefined ? {} : { shortcut: entry.shortcut })}
            {...(entry.defaultShortcut === undefined ? {} : { defaultShortcut: entry.defaultShortcut })}
            isMac={isMac}
            text={{ ...text, needsModifier: text.errors.SHORTCUT_INVALID ?? "" }}
            onSave={saveShortcut}
          />
        </div>
        {error && (
          <p class="field-error" role="alert">
            {error}
          </p>
        )}
      </div>
    </li>
  );
}
