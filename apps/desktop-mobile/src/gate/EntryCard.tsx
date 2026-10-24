import { useState } from "preact/hooks";
import { groupCode, monogram, tileColor } from "../panel/format";
import { Ring } from "../shared/Ring";
import { ShortcutField } from "../shared/ShortcutField";
import { type EntryEdit, GateFailure, type GateEntry } from "./api";
import { type GateStrings, describeFailure } from "./i18n";

/** Codes ending within this many seconds also show the next one (PRD FR-1.4). */
const NEXT_CODE_WINDOW_S = 5;

export interface EntryCardProps {
  entry: GateEntry;
  /** OTPKeep's clock now, for the countdown. */
  coreNow: number;
  text: GateStrings;
  isMac: boolean;
  onEdit: (edit: EntryEdit) => Promise<GateEntry>;
  onShortcut: (shortcut: string | null) => Promise<GateEntry>;
  /** OTPKeep copies the code; resolves with seconds until the clipboard is cleared. */
  onCopy: () => Promise<{ clearsInS: number }>;
  onNotice: (message: string) => void;
}

/**
 * One account on a row of fixed height: names (the note joins the second line), its code with the
 * countdown, its shortcut, copy and edit. Only the editor makes a row taller.
 */
export function EntryCard(props: EntryCardProps) {
  const { entry, coreNow, text, isMac, onShortcut, onCopy, onNotice } = props;
  const [editing, setEditing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const details = [entry.issuer, entry.account, entry.note].filter((s) => s !== "" && s !== entry.title).join(" · ");
  const remainingS =
    entry.validUntilMs === undefined ? undefined : Math.max(0, Math.ceil((entry.validUntilMs - coreNow) / 1000));
  const ending = remainingS !== undefined && remainingS <= NEXT_CODE_WINDOW_S;

  const explain = (failure: unknown) =>
    failure instanceof GateFailure ? describeFailure(text, failure.code, failure.detail) : text.errors.UNKNOWN ?? "";

  const copy = async () => {
    try {
      const { clearsInS } = await onCopy();
      setError(null);
      onNotice(text.copied(clearsInS));
    } catch (failure) {
      setError(explain(failure));
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

  if (editing) {
    return (
      <li class="card editing" data-testid="card">
        <Tile entry={entry} />
        <EditForm
          entry={entry}
          text={text}
          onEdit={props.onEdit}
          explain={explain}
          onDone={(saved) => {
            setEditing(false);
            setError(null);
            if (saved) onNotice(text.saved);
          }}
        />
      </li>
    );
  }

  return (
    <li class="card" data-testid="card">
      <Tile entry={entry} />
      <div class="card-names">
        <span class="card-title" title={entry.title}>
          {entry.title}
        </span>
        {error ? (
          <span class="card-error" role="alert">
            {error}
          </span>
        ) : (
          details !== "" && (
            <span class="card-subtitle" title={details}>
              {details}
            </span>
          )
        )}
      </div>
      <div class="card-code">
        {entry.code === undefined ? (
          <span class="muted">{entry.kind === "hotp" ? text.hotpInApp : text.noCode}</span>
        ) : (
          <>
            <span class={`code${ending ? " ending" : ""}`} data-testid="code">
              {groupCode(entry.code)}
            </span>
            {ending && entry.nextCode && (
              <span class="next-code">
                {text.next} {groupCode(entry.nextCode)}
              </span>
            )}
          </>
        )}
      </div>
      {remainingS !== undefined && entry.periodS !== undefined ? (
        <Ring remaining={remainingS} period={entry.periodS} ending={ending} label={text.seconds(remainingS)} />
      ) : (
        <span class="ring-space" />
      )}
      <div class="card-shortcut">
        <ShortcutField
          {...(entry.shortcut === undefined ? {} : { shortcut: entry.shortcut })}
          {...(entry.defaultShortcut === undefined ? {} : { defaultShortcut: entry.defaultShortcut })}
          isMac={isMac}
          text={{ ...text, needsModifier: text.errors.SHORTCUT_INVALID ?? "" }}
          onSave={saveShortcut}
        />
      </div>
      <div class="card-actions">
        <button type="button" class="link-button" onClick={() => void copy()}>
          {text.copy}
        </button>
        <button type="button" class="link-button" onClick={() => setEditing(true)}>
          {text.edit}
        </button>
      </div>
    </li>
  );
}

function Tile({ entry }: { entry: GateEntry }) {
  return (
    <span class={`tile tile-${tileColor(entry.issuer || entry.title)}`} aria-hidden="true">
      {monogram(entry.issuer, entry.title)}
    </span>
  );
}

interface EditFormProps {
  entry: GateEntry;
  text: GateStrings;
  onEdit: (edit: EntryEdit) => Promise<GateEntry>;
  explain: (failure: unknown) => string;
  /** `saved` is false when nothing changed or the person cancelled. */
  onDone: (saved: boolean) => void;
}

/** The names and the note, edited together. */
function EditForm({ entry, text, onEdit, explain, onDone }: EditFormProps) {
  const [title, setTitle] = useState(entry.title);
  const [note, setNote] = useState(entry.note);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const save = async (event: Event) => {
    event.preventDefault();
    const edit: EntryEdit = {
      ...(title !== entry.title ? { title } : {}),
      ...(note !== entry.note ? { note } : {}),
    };
    if (Object.keys(edit).length === 0) {
      onDone(false);
      return;
    }
    setBusy(true);
    try {
      await onEdit(edit);
      onDone(true);
    } catch (failure) {
      setError(explain(failure));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form class="edit-form" onSubmit={(e) => void save(e)}>
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
      {error && (
        <p class="field-error" role="alert">
          {error}
        </p>
      )}
      <div class="form-actions">
        <button type="submit" class="primary" disabled={busy}>
          {text.save}
        </button>
        <button type="button" class="secondary" onClick={() => onDone(false)}>
          {text.cancel}
        </button>
      </div>
    </form>
  );
}
