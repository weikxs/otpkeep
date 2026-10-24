import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import type { PanelEntry } from "../panel/api";
import type { Strings } from "../panel/i18n";
import { useLiveState } from "../panel/live";
import { EntryList, PasswordPrompt, SearchIcon, generateIfNeeded } from "../panel/Panel";
import type { SearchApi } from "./api";
import type { SearchStrings } from "./i18n";

export interface SearchAppProps {
  api: SearchApi;
  text: Strings;
  words: SearchStrings;
  isMac: boolean;
  /** The local clock; tests pass their own. */
  clock?: () => number;
}

/**
 * The search panel (design/04 §6): opened anywhere with a global shortcut, type to find an account, Enter
 * copies its code and closes, ⌘E edits it in the main window, Esc clears and then closes.
 */
export function SearchApp({ api, text, words, isMac, clock = Date.now }: SearchAppProps) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [failed, setFailed] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const { state, setState, coreNow } = useLiveState(api, query, clock);
  const entries = state?.entries ?? [];

  useEffect(() => input.current?.focus(), [state?.status]);
  // Each opening starts afresh.
  useEffect(
    () =>
      api.onShown(() => {
        setQuery("");
        setSelected(0);
        setFailed(false);
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
      // ⇧Enter will type the code (FR-5.5); until then it copies too.
      event.preventDefault();
      void copy(entry);
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
            }}
          />
        </header>
      )}
      {state.status === "needsPassword" && <PasswordPrompt api={api} text={text} onOpened={setState} />}
      {state.status === "unavailable" && (
        <section class="panel-message">
          <p class="message-title">{text.unavailable}</p>
          {state.problem && <p class="message-body">{text.problems[state.problem]}</p>}
          <button type="button" class="primary" onClick={() => void api.openMain()}>
            {text.handleInMain}
          </button>
        </section>
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
        {failed ? (
          <span class="error" role="alert">
            {text.copyFailed}
          </span>
        ) : (
          <>
            <span>
              <kbd>↵</kbd> {words.copy}
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
