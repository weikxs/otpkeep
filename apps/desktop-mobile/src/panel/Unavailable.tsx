import { useState } from "preact/hooks";
import type { PanelState, Problem } from "./api";
import type { Strings } from "./i18n";

/** Problems that can pass on their own: another OTPKeep quits, the keychain unlocks, the disk or file is back. */
const RETRYABLE: readonly Problem[] = ["BUSY", "KEYSTORE_UNAVAILABLE", "IO", "DATA_MISSING"];

/** Whether trying to open the data again can help. */
export function canRetry(problem: Problem | undefined): boolean {
  return problem !== undefined && RETRYABLE.includes(problem);
}

export interface UnavailableProps {
  state: PanelState;
  text: Strings;
  /** Tries again and shows what came of it. */
  onRetry: () => Promise<void>;
  onOpenMain: () => void;
}

/** Why the data cannot be opened, in the quick and search panels, with what can be done about it here. */
export function Unavailable({ state, text, onRetry, onOpenMain }: UnavailableProps) {
  const [trying, setTrying] = useState(false);
  const retry = async () => {
    setTrying(true);
    try {
      await onRetry();
    } finally {
      setTrying(false);
    }
  };
  const retryable = canRetry(state.problem);
  return (
    <section class="panel-message">
      <p class="message-title">{text.unavailable}</p>
      {state.problem && <p class="message-body">{text.problems[state.problem]}</p>}
      {retryable && (
        <button type="button" class="primary" disabled={trying} onClick={() => void retry()}>
          {text.retry}
        </button>
      )}
      <button type="button" class={retryable ? "secondary" : "primary"} onClick={onOpenMain}>
        {text.handleInMain}
      </button>
    </section>
  );
}
