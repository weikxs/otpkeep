import { useState } from "preact/hooks";
import { type BatchAction, CommandFailure } from "./api";
import { type MainStrings, describe } from "./i18n";

export interface BatchBarProps {
  count: number;
  /** Tags the chosen entries have, for "remove a tag". */
  tags: string[];
  text: MainStrings;
  onSelectAll: () => void;
  onSelectNone: () => void;
  onApply: (action: BatchAction) => Promise<void>;
  onDone: () => void;
}

type Mode = "idle" | "add" | "remove" | "trash";

/** The actions for the chosen entries, at the bottom of the window (PRD FR-4.5). */
export function BatchBar({ count, tags, text, onSelectAll, onSelectNone, onApply, onDone }: BatchBarProps) {
  const [mode, setMode] = useState<Mode>("idle");
  const [tag, setTag] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const apply = async (action: BatchAction) => {
    setBusy(true);
    setError(null);
    try {
      await onApply(action);
      setMode("idle");
      setTag("");
    } catch (thrown) {
      const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
      setError(describe(text, failure.code, failure.detail));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="batch-bar" role="toolbar" aria-label={text.selected(count)}>
      <span class="batch-count">{text.selected(count)}</span>
      <button type="button" class="link-button" onClick={onSelectAll}>
        {text.selectAll}
      </button>
      <button type="button" class="link-button" onClick={onSelectNone}>
        {text.selectNone}
      </button>
      <span class="spacer" />
      {mode === "add" && (
        <form
          class="batch-inline"
          onSubmit={(e) => {
            e.preventDefault();
            void apply({ action: "addTag", tag });
          }}
        >
          <input value={tag} maxLength={30} autoFocus aria-label={text.tagToAdd} placeholder={text.tagToAdd} onInput={(e) => setTag((e.target as HTMLInputElement).value)} />
          <button type="submit" class="primary small" disabled={busy}>
            {text.apply}
          </button>
        </form>
      )}
      {mode === "remove" && (
        <span class="batch-inline">
          <select value={tag} aria-label={text.tagToRemove} onChange={(e) => setTag((e.target as HTMLSelectElement).value)}>
            <option value="">{text.tagToRemove}</option>
            {tags.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
          <button type="button" class="primary small" disabled={busy || tag === ""} onClick={() => void apply({ action: "removeTag", tag })}>
            {text.apply}
          </button>
        </span>
      )}
      {mode === "trash" && (
        <span class="batch-inline">
          <span class="muted">{text.confirmBatchTrash(count)}</span>
          <button type="button" class="danger small" disabled={busy} onClick={() => void apply({ action: "trash" })}>
            {text.trash}
          </button>
        </span>
      )}
      {mode === "idle" ? (
        <>
          <button type="button" class="secondary small" disabled={count === 0} onClick={() => setMode("add")}>
            {text.addTagTo}
          </button>
          <button type="button" class="secondary small" disabled={count === 0 || tags.length === 0} onClick={() => setMode("remove")}>
            {text.removeTagFrom}
          </button>
          <button type="button" class="secondary small danger-link" disabled={count === 0} onClick={() => setMode("trash")}>
            {text.trash}
          </button>
        </>
      ) : (
        <button type="button" class="secondary small" onClick={() => setMode("idle")}>
          {text.cancel}
        </button>
      )}
      <button type="button" class="primary small" onClick={onDone}>
        {text.done}
      </button>
      {error && (
        <p class="field-error batch-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
