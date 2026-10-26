import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import type { PanelEntry } from "../panel/api";
import type { Strings } from "../panel/i18n";
import { useLiveState } from "../panel/live";
import { Unavailable } from "../panel/Unavailable";
import { EntryList, PasswordPrompt, SearchIcon, generateIfNeeded } from "../panel/Panel";
import { type LanguageControl, LanguageSelect } from "../shared/LanguageSelect";
import type { SearchApi, TypeResult } from "./api";
import type { SearchStrings } from "./i18n";

export interface SearchAppProps {
  api: SearchApi;
  text: Strings;
  words: SearchStrings;
  isMac: boolean;
  /** The local clock; tests pass their own. */
  clock?: () => number;
  /** The language menu in the footer; absent hides it. */
  language?: LanguageControl;
}

/** Longest wait for Shift to be let go before typing (design/07 §2.3). */
const RELEASE_WAIT_MS = 1000;

/**
 * Resolves once Shift is let go in this window, or after a second. Only this window's own key events are
 * watched, so the code typed next is not turned into symbols.
 */
function shiftReleased(): Promise<void> {
  return new Promise((resolve) => {
    const done = () => {
      window.removeEventListener("keyup", onUp);
      clearTimeout(timer);
      resolve();
    };
    const onUp = (event: KeyboardEvent) => {
      if (!event.shiftKey) done();
    };
    const timer = setTimeout(done, RELEASE_WAIT_MS);
    window.addEventListener("keyup", onUp);
  });
}

/**
 * The search panel (design/04 §6): opened anywhere with a global shortcut, type to find an account, Enter
 * copies its code and closes, ⌘E edits it in the main window, Esc clears and then closes.
 */
export function SearchApp({ api, text, words, isMac, clock = Date.now, language }: SearchAppProps) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [failed, setFailed] = useState(false);
  const [fallback, setFallback] = useState<TypeResult | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const { state, setState, coreNow, reload } = useLiveState(api, query, clock);
  const entries = state?.entries ?? [];

  useEffect(() => input.current?.focus(), [state?.status]);
  // Each opening starts afresh.
  useEffect(
    () =>
      api.onShown(() => {
        setQuery("");
        setSelected(0);
        setFailed(false);
        setFallback(null);
        input.current?.focus();
      }),
    [api],
  );
  useEffect(() => {
    document.querySelector(".entry.selected")?.scrollIntoView?.({ block: "nearest" });
  }, [selected]);

  const copy = useCallback(
    async (entry: PanelEntry) => {
      try {
        await generateIfNeeded(api, entry, setState);
        await api.copy(entry.id);
        await api.hide();
      } catch {
        setFailed(true);
      }
    },
    [api, setState],
  );

  const typeOut = useCallback(
    async (entry: PanelEntry) => {
      // Listen before anything else, so a quick release is not missed.
      const released = shiftReleased();
      try {
        await generateIfNeeded(api, entry, setState);
        await released;
        const result = await api.typeCode(entry.id);
        if (!result.typed) setFallback(result);
      } catch {
        setFailed(true);
      }
    },
    [api, setState],
  );

  const edit = useCallback(
    async (entry: PanelEntry) => {
      await api.openMain(`edit:${entry.id}`);
      await api.hide();
    },
    [api],
  );

  const onKey = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      if (query === "") void api.hide();
      else {
        setQuery("");
        setSelected(0);
      }
      return;
    }
    if (state?.status !== "ready") return;
    const entry = entries[selected];
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelected((n) => Math.min(n + 1, Math.max(entries.length - 1, 0)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelected((n) => Math.max(n - 1, 0));
    } else if (event.key === "Enter" && entry) {
      event.preventDefault();
      void (event.shiftKey ? typeOut(entry) : copy(entry));
    } else if ((isMac ? event.metaKey : event.ctrlKey) && event.key.toLowerCase() === "e" && entry) {
      event.preventDefault();
      void edit(entry);
    }
  };

  if (state === null) return <div class="search" aria-busy="true" onKeyDown={onKey} />;

  const modifier = isMac ? "⌘" : "Ctrl+";
  return (
    <div class="search" onKeyDown={onKey}>
      {state.status === "ready" && (
        <header class="search-field">
          <SearchIcon />
          <input
            ref={input}
            type="search"
            value={query}
            placeholder={text.search}
            aria-label={text.search}
            onInput={(e) => {
              // Reset here, not in an effect: an effect runs after a key pressed right after typing.
              setQuery((e.target as HTMLInputElement).value);
              setSelected(0);
              setFailed(false);
              setFallback(null);
            }}
          />
        </header>
      )}
      {state.status === "needsPassword" && <PasswordPrompt api={api} text={text} onOpened={setState} />}
      {state.status === "unavailable" && (
        <Unavailable
          state={state}
          text={text}
          onRetry={async () => {
            await api.retryOpen();
            await reload();
          }}
          onOpenMain={() => void api.openMain()}
        />
      )}
      {state.status === "ready" && entries.length === 0 && (
        <p class="panel-none">{query === "" ? text.emptyTitle : text.noMatch}</p>
      )}
      {state.status === "ready" && entries.length > 0 && (
        <EntryList
          entries={entries}
          query={query}
          selected={selected}
          copiedId={null}
          coreNow={coreNow}
          text={text}
          isMac={isMac}
          onCopy={(entry) => void copy(entry)}
          onHover={setSelected}
        />
      )}
      <footer class="search-hints">
        {language && (
          <span class="search-language">
            <LanguageSelect control={language} systemLabel={text.followSystem} label={text.language} />
          </span>
        )}
        {failed ? (
          <span class="error" role="alert">
            {text.copyFailed}
          </span>
        ) : fallback ? (
          <span class="search-notice" role="status">
            {fallback.fallback === "UNSUPPORTED"
              ? words.unsupported(fallback.clearsInS)
              : words.noPermission(fallback.clearsInS)}
            {fallback.fallback === "NO_PERMISSION" && (
              <button
                type="button"
                class="link-button"
                onClick={() => void api.askTypingPermission().then(() => setFallback(null))}
              >
                {words.grant}
              </button>
            )}
          </span>
        ) : (
          <>
            <span>
              <kbd>↵</kbd> {words.copy}
            </span>
            <span>
              <kbd>⇧↵</kbd> {words.type}
            </span>
            <span>
              <kbd>{modifier}E</kbd> {words.edit}
            </span>
            <span>
              <kbd>esc</kbd> {words.close}
            </span>
          </>
        )}
      </footer>
    </div>
  );
}
