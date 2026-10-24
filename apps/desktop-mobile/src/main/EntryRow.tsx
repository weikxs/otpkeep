import { useState } from "preact/hooks";
import { groupCode, monogram, tileColor } from "../panel/format";
import { Ring } from "../shared/Ring";
import { formatShortcut } from "../shared/shortcut";
import { CommandFailure, type Entry } from "./api";
import { type MainStrings, describe } from "./i18n";

/** Codes ending within this many seconds also show the next one (PRD FR-1.4). */
const NEXT_CODE_WINDOW_S = 5;

export interface EntryRowProps {
  entry: Entry;
  coreNow: number;
  copied: boolean;
  text: MainStrings;
  isMac: boolean;
  onCopy: (entry: Entry) => void;
  onSave: (change: { title?: string; note?: string }) => Promise<void>;
}

/** One account in the main window: click to copy; names and the note are edited in place. */
export function EntryRow({ entry, coreNow, copied, text, isMac, onCopy, onSave }: EntryRowProps) {
  const [editing, setEditing] = useState(false);
  const subtitle = [entry.issuer, entry.account].filter((s) => s !== "" && s !== entry.title).join(" · ");
  const remainingS =
    entry.validUntilMs === undefined ? undefined : Math.max(0, Math.ceil((entry.validUntilMs - coreNow) / 1000));
  const ending = remainingS !== undefined && remainingS <= NEXT_CODE_WINDOW_S;

  if (editing) {
    return (
      <li class="entry-row editing" data-testid="entry">
        <EditForm entry={entry} text={text} onSave={onSave} onDone={() => setEditing(false)} />
      </li>
    );
  }

  return (
    <li class="entry-row" data-testid="entry">
      <button type="button" class="entry-main" onClick={() => onCopy(entry)} aria-label={`${entry.title} ${text.copied}`}>
        <span class={`tile tile-${tileColor(entry.issuer || entry.title)}`} aria-hidden="true">
          {monogram(entry.issuer, entry.title)}
        </span>
        <span class="entry-names">
          <span class="entry-title">{entry.title}</span>
          {subtitle !== "" && <span class="entry-subtitle">{subtitle}</span>}
          {entry.note !== "" && <span class="entry-note">{entry.note}</span>}
        </span>
        <span class="entry-code">
          {copied ? (
            <span class="copied">{text.copied} ✓</span>
          ) : entry.code === undefined && entry.kind === "hotp" ? (
            <span class="generate">{text.generate}</span>
          ) : entry.code === undefined ? (
            <span class="no-code">{text.noCode}</span>
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
        </span>
        {remainingS !== undefined && entry.periodS !== undefined ? (
          <Ring remaining={remainingS} period={entry.periodS} ending={ending} label={`${remainingS}`} />
        ) : (
          <span class="ring-space" />
        )}
      </button>
      <span class="entry-actions">
        {entry.shortcut !== undefined && <kbd class="keys">{formatShortcut(entry.shortcut, isMac)}</kbd>}
        <button type="button" class="link-button" onClick={() => setEditing(true)}>
          {text.edit}
        </button>
      </span>
    </li>
  );
}

interface EditFormProps {
  entry: Entry;
  text: MainStrings;
  onSave: (change: { title?: string; note?: string }) => Promise<void>;
  onDone: () => void;
}

function EditForm({ entry, text, onSave, onDone }: EditFormProps) {
  const [title, setTitle] = useState(entry.title);
  const [note, setNote] = useState(entry.note);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (event: Event) => {
    event.preventDefault();
    const change = {
      ...(title !== entry.title ? { title } : {}),
      ...(note !== entry.note ? { note } : {}),
    };
    if (Object.keys(change).length === 0) return onDone();
    setBusy(true);
    try {
      await onSave(change);
      onDone();
    } catch (thrown) {
      const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
      setError(describe(text, failure.code, failure.detail));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form class="edit-form" onSubmit={(e) => void submit(e)}>
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
        <button type="button" class="secondary" onClick={onDone}>
          {text.cancel}
        </button>
      </div>
    </form>
  );
}
