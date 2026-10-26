import { useState } from "preact/hooks";
import type { ComponentChildren } from "preact";
import { canRetry } from "../panel/Unavailable";
import type { MainState } from "./api";
import type { MainStrings } from "./i18n";

export interface MainUnavailableProps {
  state: MainState;
  text: MainStrings;
  /** The language menu, when shown. */
  corner?: ComponentChildren;
  /** Why the last attempt failed, when it did. */
  notice?: string | null;
  onRetry: () => Promise<void>;
  /** Puts the previous version back; asked only after the person confirmed. */
  onRestore: () => Promise<void>;
}

/** Why the data cannot be opened, and the way out: try again, or put back the previous version of a damaged file. */
export function MainUnavailable({ state, text, corner, notice, onRetry, onRestore }: MainUnavailableProps) {
  const [confirming, setConfirming] = useState(false);
  const [working, setWorking] = useState(false);
  const run = async (action: () => Promise<void>) => {
    setWorking(true);
    try {
      await action();
    } finally {
      setWorking(false);
      setConfirming(false);
    }
  };
  return (
    <main class="welcome" role="alert">
      {corner && <span class="corner-language">{corner}</span>}
      <h1>{text.unavailable}</h1>
      {state.problem && <p class="muted">{text.problems[state.problem]}</p>}
      {canRetry(state.problem) && (
        <button type="button" class="primary" disabled={working} onClick={() => void run(onRetry)}>
          {text.retry}
        </button>
      )}
      {state.canRestore && !confirming && (
        <button type="button" class="primary" disabled={working} onClick={() => setConfirming(true)}>
          {text.restorePrevious}
        </button>
      )}
      {state.canRestore && confirming && (
        <div class="welcome-confirm">
          <p>{text.restoreConfirm}</p>
          <button type="button" class="secondary" disabled={working} onClick={() => setConfirming(false)}>
            {text.cancel}
          </button>
          <button type="button" class="primary" disabled={working} onClick={() => void run(onRestore)}>
            {text.restore}
          </button>
        </div>
      )}
      {notice && <p role="status">{notice}</p>}
    </main>
  );
}
