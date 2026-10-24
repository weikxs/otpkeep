import { useState } from "preact/hooks";
import { formatShortcut, recordShortcut } from "./shortcut";

/** The words the field uses; each page supplies them in its language. */
export interface ShortcutTexts {
  defaultShortcut: (label: string) => string;
  noShortcut: string;
  record: string;
  change: string;
  clear: string;
  recording: string;
  /** Shown when only Shift or no modifier was held. */
  needsModifier: string;
}

export interface ShortcutFieldProps {
  shortcut?: string;
  defaultShortcut?: string;
  isMac: boolean;
  text: ShortcutTexts;
  /** Saves the recorded shortcut, or removes it with `null`; rejects with the reason shown to the person. */
  onSave: (shortcut: string | null) => Promise<void>;
}

/** An entry's own global shortcut: click "Set", press the combination, done. */
export function ShortcutField({ shortcut, defaultShortcut, isMac, text, onSave }: ShortcutFieldProps) {
  const [recording, setRecording] = useState(false);
  const [hint, setHint] = useState<string | null>(null);

  const onKeyDown = (event: KeyboardEvent) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.code === "Escape" && !event.ctrlKey && !event.altKey && !event.metaKey) {
      setRecording(false);
      setHint(null);
      return;
    }
    const recorded = recordShortcut(event);
    if (recorded.kind === "incomplete") return;
    if (recorded.kind === "needsModifier") {
      setHint(text.needsModifier);
      return;
    }
    setRecording(false);
    setHint(null);
    void onSave(recorded.shortcut);
  };

  if (recording) {
    return (
      <div class="shortcut-field">
        <button
          type="button"
          class="recorder"
          autoFocus
          onKeyDown={onKeyDown}
          onBlur={() => setRecording(false)}
          data-testid="recorder"
        >
          {text.recording}
        </button>
        {hint && <p class="field-error">{hint}</p>}
      </div>
    );
  }

  return (
    <div class="shortcut-field">
      {shortcut ? (
        <kbd class="keys">{formatShortcut(shortcut, isMac)}</kbd>
      ) : (
        <span class="muted">
          {defaultShortcut ? text.defaultShortcut(formatShortcut(defaultShortcut, isMac)) : text.noShortcut}
        </span>
      )}
      <button type="button" class="link-button" onClick={() => setRecording(true)}>
        {shortcut ? text.change : text.record}
      </button>
      {shortcut && (
        <button type="button" class="link-button" onClick={() => void onSave(null)}>
          {text.clear}
        </button>
      )}
    </div>
  );
}
