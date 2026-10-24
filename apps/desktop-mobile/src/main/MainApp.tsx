import { useCallback, useEffect, useState } from "preact/hooks";
import { type AddTab, AddDialog } from "./AddDialog";
import { CommandFailure, type Entry, type MainApi, type MainState } from "./api";
import { EntryRow } from "./EntryRow";
import { type MainStrings, describe } from "./i18n";

const TICK_MS = 250;
const COPIED_MS = 1200;
const TOAST_MS = 2600;

export interface MainAppProps {
  api: MainApi;
  text: MainStrings;
  isMac: boolean;
  clock?: () => number;
}

type Dialog = { tab: AddTab; dropped?: Uint8Array } | null;

/** The main window: every account, search, and adding (design/07-platform.md §2.4a). */
export function MainApp({ api, text, isMac, clock = Date.now }: MainAppProps) {
  const [state, setState] = useState<MainState | null>(null);
  const [query, setQuery] = useState("");
  const [offset, setOffset] = useState(0);
  const [now, setNow] = useState(clock());
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [dragging, setDragging] = useState(false);

  const load = useCallback(
    async (q: string) => {
      const next = await api.state(q);
      setOffset(next.nowMs - clock());
      setNow(clock());
      setState(next);
    },
    [api, clock],
  );

  useEffect(() => void load(query), [query, load]);
  useEffect(() => api.onChange(() => void load(query)), [api, load, query]);
  useEffect(
    () =>
      api.onPage((page) => {
        if (page === "add" || page === "import") setDialog({ tab: page === "add" ? "paste" : "file" });
      }),
    [api],
  );
  useEffect(() => {
    const timer = setInterval(() => setNow(clock()), TICK_MS);
    return () => clearInterval(timer);
  }, [clock]);
  useEffect(() => {
    if (toast === null) return;
    const timer = setTimeout(() => setToast(null), TOAST_MS);
    return () => clearTimeout(timer);
  }, [toast]);

  const coreNow = now + offset;
  const expired = state?.entries.some((e) => e.validUntilMs !== undefined && e.validUntilMs <= coreNow) ?? false;
  useEffect(() => {
    if (expired) void load(query);
  }, [expired, load, query]);

  const show = (thrown: unknown) => {
    const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
    setToast(describe(text, failure.code, failure.detail));
  };

  const copy = async (entry: Entry) => {
    try {
      if (entry.kind === "hotp" && entry.code === undefined) {
        const generated = await api.nextHotp(entry.id);
        setState((current) => current && { ...current, entries: current.entries.map((e) => (e.id === entry.id ? generated : e)) });
      }
      const result = await api.copy(entry.id);
      setCopiedId(entry.id);
      setToast(text.copiedToast(result.clearsInS));
      setTimeout(() => setCopiedId((id) => (id === entry.id ? null : id)), COPIED_MS);
    } catch (thrown) {
      show(thrown);
    }
  };

  const save = async (id: string, change: { title?: string; note?: string }) => {
    await api.edit(id, change);
    setToast(text.saved);
    await load(query);
  };

  const onDrop = async (event: DragEvent) => {
    event.preventDefault();
    setDragging(false);
    const file = event.dataTransfer?.files?.[0];
    if (file) setDialog({ tab: "file", dropped: new Uint8Array(await file.arrayBuffer()) });
  };

  if (state === null) return <div class="main" aria-busy="true" />;
  if (state.status === "needsPassword") return <PasswordScreen api={api} text={text} onOpened={() => void load(query)} />;
  if (state.status === "unavailable") {
    return (
      <main class="welcome" role="alert">
        <h1>{text.unavailable}</h1>
        {state.problem && <p class="muted">{text.problems[state.problem]}</p>}
      </main>
    );
  }

  const empty = state.entries.length === 0 && query === "";
  return (
    <div
      class={`main${dragging ? " dragging" : ""}`}
      onDragOver={(e) => {
        e.preventDefault();
        setDragging(true);
      }}
      onDragLeave={(e) => e.target === e.currentTarget && setDragging(false)}
      onDrop={(e) => void onDrop(e)}
    >
      <header class="topbar">
        <span class="brand">{text.appName}</span>
        {!empty && (
          <input
            type="search"
            class="main-search"
            value={query}
            placeholder={text.search}
            aria-label={text.search}
            onInput={(e) => setQuery((e.target as HTMLInputElement).value)}
          />
        )}
        <span class="topbar-actions">
          <button type="button" class="secondary" onClick={() => void api.openGate()}>
            {text.manage}
          </button>
          <button type="button" class="primary" onClick={() => setDialog({ tab: "paste" })}>
            + {text.add}
          </button>
        </span>
      </header>
      {empty ? (
        <Welcome text={text} onChoose={(tab) => setDialog({ tab })} />
      ) : (
        <main class="list">
          <p class="muted list-count">{text.count(state.entries.length)}</p>
          {state.entries.length === 0 && <p class="empty-search">{text.noMatch}</p>}
          <ul class="entries-list">
            {state.entries.map((entry) => (
              <EntryRow
                key={entry.id}
                entry={entry}
                coreNow={coreNow}
                copied={copiedId === entry.id}
                text={text}
                isMac={isMac}
                onCopy={(e) => void copy(e)}
                onSave={(change) => save(entry.id, change)}
              />
            ))}
          </ul>
        </main>
      )}
      {dragging && <div class="drop-veil">{text.dropHere}</div>}
      {dialog && (
        <AddDialog
          api={api}
          text={text}
          tab={dialog.tab}
          {...(dialog.dropped ? { dropped: dialog.dropped } : {})}
          onAdded={(count) => {
            setDialog(null);
            setToast(text.added(count));
            void load(query);
          }}
          onClose={() => setDialog(null)}
        />
      )}
      {toast && (
        <div class="toast" role="status">
          {toast}
        </div>
      )}
    </div>
  );
}

function Welcome({ text, onChoose }: { text: MainStrings; onChoose: (tab: AddTab) => void }) {
  const choices: [AddTab, string, string][] = [
    ["paste", text.welcomePaste, text.welcomePasteHint],
    ["file", text.welcomeFile, text.welcomeFileHint],
    ["manual", text.welcomeManual, text.welcomeManualHint],
  ];
  return (
    <main class="welcome">
      <h1>{text.welcomeTitle}</h1>
      <p class="muted">{text.welcomeBody}</p>
      <div class="welcome-choices">
        {choices.map(([tab, title, hint]) => (
          <button key={tab} type="button" class="choice" onClick={() => onChoose(tab)}>
            <span class="choice-title">{title}</span>
            <span class="muted">{hint}</span>
          </button>
        ))}
      </div>
      <p class="muted">{text.dropHint}</p>
    </main>
  );
}

function PasswordScreen({ api, text, onOpened }: { api: MainApi; text: MainStrings; onOpened: () => void }) {
  const [password, setPassword] = useState("");
  const [wrong, setWrong] = useState(false);
  return (
    <main class="welcome">
      <h1>{text.needsPassword}</h1>
      <form
        class="password-step"
        onSubmit={(e) => {
          e.preventDefault();
          void api.unlock(password).then((next) => {
            setPassword("");
            if (next.status === "needsPassword") setWrong(true);
            else onOpened();
          });
        }}
      >
        <input
          type="password"
          value={password}
          aria-label={text.password}
          placeholder={text.password}
          autoFocus
          onInput={(e) => {
            setPassword((e.target as HTMLInputElement).value);
            setWrong(false);
          }}
        />
        <button type="submit" class="primary">
          {text.open}
        </button>
        {wrong && <p class="field-error">{text.wrongPassword}</p>}
      </form>
    </main>
  );
}
