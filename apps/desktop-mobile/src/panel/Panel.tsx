import { Fragment } from "preact";
import { useCallback, useEffect, useMemo, useRef, useState } from "preact/hooks";
import type { PanelApi, PanelEntry, PanelState } from "./api";
import { groupCode, monogram, shortcutLabel, tileColor } from "./format";
import type { Strings } from "./i18n";

/** Codes ending within this many seconds also show the next one (FR-1.4). */
const NEXT_CODE_WINDOW_S = 5;
const TICK_MS = 250;
const COPIED_MS = 1200;
const TOAST_MS = 2600;

export interface PanelProps {
  api: PanelApi;
  text: Strings;
  isMac: boolean;
  /** The local clock; tests pass their own. */
  clock?: () => number;
}

/** The menu bar quick panel (design/12). */
export function Panel({ api, text, isMac, clock = Date.now }: PanelProps) {
  const [state, setState] = useState<PanelState | null>(null);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [offset, setOffset] = useState(0);
  const [now, setNow] = useState(clock());
  const search = useRef<HTMLInputElement>(null);

  const load = useCallback(
    async (q: string) => {
      const next = await api.state(q);
      setOffset(next.nowMs - clock());
      setNow(clock());
      setState(next);
    },
    [api, clock],
  );

  useEffect(() => {
    void load(query);
    setSelected(0);
  }, [query, load]);

  const coreNow = now + offset;
  const expired = state?.entries.some((e) => e.validUntilMs !== undefined && e.validUntilMs <= coreNow) ?? false;
  useEffect(() => {
    if (expired) void load(query);
  }, [expired, load, query]);

  useEffect(() => {
    const timer = setInterval(() => setNow(clock()), TICK_MS);
    return () => clearInterval(timer);
  }, [clock]);

  useEffect(() => search.current?.focus(), [state?.status]);

  const entries = state?.entries ?? [];

  const copy = useCallback(
    async (entry: PanelEntry) => {
      if (entry.kind === "hotp" && entry.code === undefined) {
        const generated = await api.nextHotp(entry.id);
        setState((current) =>
          current && { ...current, entries: current.entries.map((e) => (e.id === entry.id ? generated : e)) },
        );
      }
      try {
        const result = await api.copy(entry.id);
        setCopiedId(entry.id);
        setToast(text.copiedToast(result.clearsInS));
        setTimeout(() => setCopiedId((id) => (id === entry.id ? null : id)), COPIED_MS);
        setTimeout(() => setToast(null), TOAST_MS);
      } catch {
        setToast(text.copyFailed);
      }
    },
    [api, text],
  );

  const onKey = (event: KeyboardEvent) => {
    if (state?.status !== "ready") return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelected((n) => Math.min(n + 1, Math.max(entries.length - 1, 0)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelected((n) => Math.max(n - 1, 0));
    } else if (event.key === "Enter") {
      const entry = entries[selected];
      if (entry) void copy(entry);
    } else if (event.key === "Escape") {
      if (query !== "") setQuery("");
      else void api.hide();
    } else if ((event.metaKey || event.ctrlKey) && /^[0-9]$/u.test(event.key)) {
      const index = event.key === "0" ? 9 : Number(event.key) - 1;
      const entry = entries[index];
      if (entry) {
        event.preventDefault();
        void copy(entry);
      }
    }
  };

  if (state === null) return <div class="panel" aria-busy="true" />;

  return (
    <div class="panel" onKeyDown={onKey}>
      {state.status === "ready" && (
        <header class="panel-search">
          <SearchIcon />
          <input
            ref={search}
            type="search"
            value={query}
            placeholder={text.search}
            aria-label={text.search}
            onInput={(e) => setQuery((e.target as HTMLInputElement).value)}
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
      {state.status === "ready" && entries.length === 0 && query === "" && (
        <section class="panel-message">
          <p class="message-title">{text.emptyTitle}</p>
          <p class="message-body">{text.emptyBody}</p>
          <button type="button" class="primary" onClick={() => void api.openMain("import")}>
            {text.importGp}
          </button>
          <button type="button" class="secondary" onClick={() => void api.openMain("add")}>
            {text.add}
          </button>
        </section>
      )}
      {state.status === "ready" && entries.length === 0 && query !== "" && <p class="panel-none">{text.noMatch}</p>}
      {state.status === "ready" && entries.length > 0 && (
        <EntryList
          entries={entries}
          query={query}
          selected={selected}
          copiedId={copiedId}
          coreNow={coreNow}
          text={text}
          isMac={isMac}
          onCopy={(entry) => void copy(entry)}
          onHover={setSelected}
        />
      )}
      <footer class="panel-footer">
        <span class="status-line">{state.readOnly ? text.readOnly : state.inGroup ? text.inGroup : text.notInGroup}</span>
        <span class="footer-actions">
          <button type="button" class="icon-button" title={text.addShort} aria-label={text.add} onClick={() => void api.openMain("add")}>
            <PlusIcon />
          </button>
          <button type="button" class="icon-button" title={text.openMain} aria-label={text.openMain} onClick={() => void api.openMain()}>
            <WindowIcon />
          </button>
          <button type="button" class="icon-button" title={text.settings} aria-label={text.settings} onClick={() => void api.openMain("settings")}>
            <GearIcon />
          </button>
        </span>
      </footer>
      {toast && (
        <div class="toast" role="status">
          {toast}
        </div>
      )}
    </div>
  );
}

interface ListProps {
  entries: PanelEntry[];
  query: string;
  selected: number;
  copiedId: string | null;
  coreNow: number;
  text: Strings;
  isMac: boolean;
  onCopy: (entry: PanelEntry) => void;
  onHover: (index: number) => void;
}

function EntryList({ entries, query, selected, copiedId, coreNow, text, isMac, onCopy, onHover }: ListProps) {
  const pinnedCount = query === "" ? entries.filter((e) => e.pinned).length : 0;
  return (
    <ul class="entries" role="listbox" aria-label={query === "" ? text.all : text.results}>
      {entries.map((entry, index) => (
        <Fragment key={entry.id}>
          {query === "" && index === 0 && pinnedCount > 0 && <li class="group-title">{text.pinned}</li>}
          {query === "" && index === pinnedCount && pinnedCount > 0 && <li class="group-title">{text.all}</li>}
          <Row
            entry={entry}
            index={index}
            selected={index === selected}
            copied={copiedId === entry.id}
            coreNow={coreNow}
            text={text}
            isMac={isMac}
            onCopy={onCopy}
            onHover={onHover}
          />
        </Fragment>
      ))}
    </ul>
  );
}

interface RowProps {
  entry: PanelEntry;
  index: number;
  selected: boolean;
  copied: boolean;
  coreNow: number;
  text: Strings;
  isMac: boolean;
  onCopy: (entry: PanelEntry) => void;
  onHover: (index: number) => void;
}

function Row({ entry, index, selected, copied, coreNow, text, isMac, onCopy, onHover }: RowProps) {
  const subtitle = [entry.issuer, entry.account].filter((s) => s !== "" && s !== entry.title).join(" · ");
  const remainingS =
    entry.validUntilMs === undefined ? undefined : Math.max(0, Math.ceil((entry.validUntilMs - coreNow) / 1000));
  const ending = remainingS !== undefined && remainingS <= NEXT_CODE_WINDOW_S;
  const label = useMemo(() => monogram(entry.issuer, entry.title), [entry.issuer, entry.title]);
  return (
    <li
      class={`entry${selected ? " selected" : ""}`}
      role="option"
      aria-selected={selected}
      data-testid="entry"
      onClick={() => onCopy(entry)}
      onMouseEnter={() => onHover(index)}
    >
      <span class={`tile tile-${tileColor(entry.issuer || entry.title)}`} aria-hidden="true">
        {label}
      </span>
      <span class="names">
        <span class="title">{entry.title}</span>
        {subtitle !== "" && <span class="subtitle">{subtitle}</span>}
      </span>
      <span class="code-area">
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
        {entry.shortcut !== undefined && <span class="shortcut">{shortcutLabel(entry.shortcut, isMac)}</span>}
      </span>
      {remainingS !== undefined && entry.periodS !== undefined && (
        <Ring remaining={remainingS} period={entry.periodS} ending={ending} label={text.seconds(remainingS)} />
      )}
    </li>
  );
}

const RING_RADIUS = 7;
const RING_LENGTH = 2 * Math.PI * RING_RADIUS;

function Ring({ remaining, period, ending, label }: { remaining: number; period: number; ending: boolean; label: string }) {
  const fraction = Math.min(1, remaining / period);
  return (
    <span class={`ring${ending ? " ending" : ""}`} title={label} aria-label={label}>
      <svg viewBox="0 0 18 18" width="18" height="18" aria-hidden="true">
        <circle class="ring-track" cx="9" cy="9" r={RING_RADIUS} />
        <circle
          class="ring-value"
          cx="9"
          cy="9"
          r={RING_RADIUS}
          stroke-dasharray={`${RING_LENGTH * fraction} ${RING_LENGTH}`}
          transform="rotate(-90 9 9)"
        />
      </svg>
      <span class="ring-seconds">{remaining}</span>
    </span>
  );
}

interface PasswordPromptProps {
  api: PanelApi;
  text: Strings;
  onOpened: (state: PanelState) => void;
}

/** Asked once per run when the data is protected by a master password. */
function PasswordPrompt({ api, text, onOpened }: PasswordPromptProps) {
  const [password, setPassword] = useState("");
  const [wrong, setWrong] = useState(false);
  const submit = async (event: Event) => {
    event.preventDefault();
    const next = await api.unlock(password);
    setPassword("");
    if (next.status === "needsPassword") setWrong(true);
    else onOpened(next);
  };
  return (
    <section class="panel-message">
      <KeyIcon />
      <p class="message-title">{text.needsPassword}</p>
      <form class="unlock" onSubmit={(e) => void submit(e)}>
        <input
          type="password"
          value={password}
          placeholder={text.password}
          aria-label={text.password}
          autoFocus
          onInput={(e) => {
            setPassword((e.target as HTMLInputElement).value);
            setWrong(false);
          }}
        />
        <button type="submit" class="primary">
          {text.unlock}
        </button>
        {wrong && <p class="error">{text.wrongPassword}</p>}
      </form>
    </section>
  );
}

function SearchIcon() {
  return (
    <svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="7" cy="7" r="4.5" />
      <path d="M10.5 10.5 14 14" />
    </svg>
  );
}

function KeyIcon() {
  return (
    <svg class="icon icon-large" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="5.5" cy="8" r="2.5" />
      <path d="M8 8h6M12 8v2M14 8v1.5" />
    </svg>
  );
}

function PlusIcon() {
  return (
    <svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
      <path d="M8 3.5v9M3.5 8h9" />
    </svg>
  );
}

function WindowIcon() {
  return (
    <svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
      <rect x="2.5" y="3.5" width="11" height="9" rx="1.5" />
      <path d="M2.5 6h11" />
    </svg>
  );
}

function GearIcon() {
  return (
    <svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="8" cy="8" r="2" />
      <path d="M8 2v1.5M8 12.5V14M2 8h1.5M12.5 8H14M3.8 3.8l1 1M11.2 11.2l1 1M3.8 12.2l1-1M11.2 4.8l1-1" />
    </svg>
  );
}
