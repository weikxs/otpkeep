import { useEffect, useState } from "preact/hooks";
import { CommandFailure } from "../api";
import type { SyncStrings } from "./i18n";
import { codeText } from "./i18n";
import type { FoundView, SyncApi } from "./types";

export interface JoinFlowProps {
  looking: "sync" | "pair";
  /** This device has entries: joining a group merges or replaces them. */
  hasEntries: boolean;
  api: Pick<SyncApi, "searchWindows" | "joinSync" | "joinPair">;
  text: SyncStrings;
  onBack: () => void;
}

/** Finding a waiting device and entering the code shown on it (design/06-sync.md §4.2, §5.3). */
export function JoinFlow({ looking, hasEntries, api, text, onBack }: JoinFlowProps) {
  const [found, setFound] = useState<FoundView[] | null>(null);
  const [chosen, setChosen] = useState<FoundView | null>(null);
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [invite, setInvite] = useState("");
  const [replace, setReplace] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pair = looking === "pair";

  const search = async () => {
    setFound(null);
    setChosen(null);
    setError(null);
    try {
      const windows = await api.searchWindows(looking);
      setFound(windows);
      if (windows.length === 1) setChosen(windows[0] ?? null);
    } catch (thrown) {
      setFound([]);
      setError(codeText(thrown instanceof CommandFailure ? thrown.code : "UNKNOWN"));
    }
  };

  useEffect(() => void search(), []);

  const join = async () => {
    setError(null);
    try {
      const deviceName = name.trim() === "" ? null : name.trim();
      if (pair && invite.trim() !== "") await api.joinPair({ by: "invite", text: invite.trim() }, deviceName, replace);
      else if (chosen && pair) await api.joinPair({ by: "found", index: chosen.index, code }, deviceName, replace);
      else if (chosen) await api.joinSync(chosen.index, code);
    } catch (thrown) {
      const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
      setError(codeText(failure.code === "INVALID" && failure.detail ? failure.detail : failure.code));
    }
  };

  const digits = pair ? 8 : 6;
  const ready = (pair && invite.trim() !== "") || (chosen !== null && code.replace(/\D/gu, "").length === digits);

  return (
    <section class="join">
      <p class="preview-title">{text.searchTitle(pair)}</p>
      {found === null && <p class="muted" role="status">{text.searching}</p>}
      {found?.length === 0 && !error && <p class="muted">{text.noneFound}</p>}
      {found && found.length > 0 && (
        <ul class="found" role="listbox" aria-label={text.searchTitle(pair)}>
          {found.map((w) => (
            <li key={w.index}>
              <button
                type="button"
                role="option"
                aria-selected={chosen?.index === w.index}
                class={`found-row${chosen?.index === w.index ? " active" : ""}`}
                onClick={() => setChosen(w)}
              >
                <span class="entry-title">{w.name}</span>
                <span class="muted">
                  {w.shortFingerprint} · {text.via(w.via)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
      {chosen && (
        <label>
          <span class="field-label">{text.codeFor(chosen.name, pair)}</span>
          <input
            class="mono code-input"
            inputMode="numeric"
            autoComplete="off"
            value={code}
            maxLength={digits + 2}
            aria-label={text.codeFor(chosen.name, pair)}
            onInput={(e) => setCode((e.target as HTMLInputElement).value)}
          />
        </label>
      )}
      {pair && (
        <>
          <label>
            <span class="field-label">{text.orInvite}</span>
            <input class="mono" value={invite} placeholder={text.invitePlaceholder} onInput={(e) => setInvite((e.target as HTMLInputElement).value)} />
          </label>
          <label>
            <span class="field-label">{text.nameLabel}</span>
            <input value={name} maxLength={32} placeholder={text.namePlaceholder} onInput={(e) => setName((e.target as HTMLInputElement).value)} />
          </label>
          {hasEntries && (
            <div class="field" role="radiogroup" aria-label={text.entriesChoice}>
              <span class="field-label">{text.entriesChoice}</span>
              <label class="check">
                <input type="radio" name="entries" checked={!replace} onChange={() => setReplace(false)} />
                {text.merge}
              </label>
              <label class="check">
                <input type="radio" name="entries" checked={replace} onChange={() => setReplace(true)} />
                {text.replace}
              </label>
            </div>
          )}
        </>
      )}
      {error && (
        <p class="field-error" role="alert">
          {error}
        </p>
      )}
      <div class="form-actions">
        <button type="button" class="primary" disabled={!ready} onClick={() => void join()}>
          {text.join}
        </button>
        <button type="button" class="secondary" onClick={() => void search()}>
          {text.searchAgain}
        </button>
        <button type="button" class="secondary" onClick={onBack}>
          {text.cancel}
        </button>
      </div>
    </section>
  );
}
