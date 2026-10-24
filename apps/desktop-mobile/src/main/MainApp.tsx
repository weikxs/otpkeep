import { useCallback, useEffect, useState } from "preact/hooks";
import { type AddTab, AddDialog } from "./AddDialog";
import { CommandFailure, type Entry, type MainApi, type MainState, type Settings } from "./api";
import type { EntryChange } from "./EntryEditor";
import { ENTRY_DRAG_TYPE, EntryRow } from "./EntryRow";
import { BatchBar } from "./BatchBar";
import { ExportDialog } from "./ExportDialog";
import { SettingsDialog } from "./SettingsDialog";
import { syncStrings } from "./sync/i18n";
import { SyncDialog } from "./sync/SyncDialog";
import { TrashDialog } from "./TrashDialog";
import { type MainStrings, describe } from "./i18n";

const TICK_MS = 250;
const COPIED_MS = 1200;
const TOAST_MS = 2600;

export interface MainAppProps {
  api: MainApi;
  text: MainStrings;
  isMac: boolean;
  clock?: () => number;
  /** The settings were changed here: the page follows the theme and language. */
  onSettingsSaved?: (settings: Settings) => void;
}

type Dialog = { tab: AddTab; dropped?: Uint8Array } | null;

/** The main window: every account, search, and adding (design/07-platform.md §2.4a). */
export function MainApp({ api, text, isMac, clock = Date.now, onSettingsSaved }: MainAppProps) {
  const [state, setState] = useState<MainState | null>(null);
  const [query, setQuery] = useState("");
  const [offset, setOffset] = useState(0);
  const [now, setNow] = useState(clock());
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [dragging, setDragging] = useState(false);
  const [tag, setTag] = useState<string | null>(null);
  const [trashOpen, setTrashOpen] = useState(false);
  const [trashCount, setTrashCount] = useState(0);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [syncOpen, setSyncOpen] = useState(false);
  const [exporting, setExporting] = useState<{ ids: string[] | null; count: number } | null>(null);
  const [allCount, setAllCount] = useState(0);
  const [selecting, setSelecting] = useState(false);
  const [chosen, setChosen] = useState<string[]>([]);

  const load = useCallback(
    async (q: string) => {
      const next = await api.state(q);
      setOffset(next.nowMs - clock());
      setNow(clock());
      setState(next);
      if (q === "") setAllCount(next.entries.length);
      if (next.status === "ready") setTrashCount((await api.trashList()).length);
    },
    [api, clock],
  );

  useEffect(() => void load(query), [query, load]);
  useEffect(() => api.onChange(() => void load(query)), [api, load, query]);
  useEffect(
    () =>
      api.onPage((page) => {
        if (page === "add" || page === "import") setDialog({ tab: page === "add" ? "paste" : "file" });
        if (page === "settings") setSettingsOpen(true);
        if (page === "sync") setSyncOpen(true);
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

  const save = async (id: string, change: EntryChange) => {
    const { tags, pinned, issuer, account, ...names } = change;
    if (names.title !== undefined || names.note !== undefined) await api.edit(id, names);
    if (issuer !== undefined || account !== undefined) {
      await api.rename(id, { ...(issuer === undefined ? {} : { issuer }), ...(account === undefined ? {} : { account }) });
    }
    if (tags !== undefined) await api.setTags(id, tags);
    if (pinned !== undefined) await api.setPinned(id, pinned);
    setToast(text.saved);
    await load(query);
  };

  const after = async (action: Promise<void>, message?: string) => {
    try {
      await action;
      if (message) setToast(message);
    } catch (thrown) {
      show(thrown);
    }
    await load(query);
  };

  const isEntryDrag = (event: DragEvent) => event.dataTransfer?.types.includes(ENTRY_DRAG_TYPE) ?? false;

  const onDrop = async (event: DragEvent) => {
    if (isEntryDrag(event)) return;
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

  const empty = state.entries.length === 0 && query === "" && trashCount === 0;
  const allTags = [...new Set(state.entries.flatMap((e) => e.tags))].sort((a, b) => a.localeCompare(b));
  const shown = tag === null ? state.entries : state.entries.filter((e) => e.tags.includes(tag));
  const sortable = query === "" && tag === null && !selecting;
  const toggle = (id: string) =>
    setChosen((current) => (current.includes(id) ? current.filter((c) => c !== id) : [...current, id]));
  const chosenTags = [...new Set(shown.filter((e) => chosen.includes(e.id)).flatMap((e) => e.tags))].sort();
  const grouped = sortable && shown.some((e) => e.pinned);
  return (
    <div
      class={`main${dragging ? " dragging" : ""}`}
      onDragOver={(e) => {
        if (isEntryDrag(e)) return;
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
          {!empty && (
            <button type="button" class="secondary" onClick={() => setTrashOpen(true)}>
              {text.trashButton(trashCount)}
            </button>
          )}
          <button type="button" class="secondary" onClick={() => setSyncOpen(true)}>
            {text.syncButton}
          </button>
          {!empty && (
            <button type="button" class="secondary" onClick={() => setExporting({ ids: null, count: allCount })}>
              {text.exportButton}
            </button>
          )}
          <button type="button" class="secondary" onClick={() => setSettingsOpen(true)}>
            {text.settings}
          </button>
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
          <div class="list-head">
            <span class="muted">{text.count(shown.length)}</span>
            {!selecting && shown.length > 0 && (
              <button type="button" class="link-button" onClick={() => setSelecting(true)}>
                {text.select}
              </button>
            )}
            {allTags.length > 0 && (
              <span class="tag-filter" role="group" aria-label={text.tags}>
                {[null, ...allTags].map((t) => (
                  <button
                    key={t ?? ""}
                    type="button"
                    class={`tag-chip${tag === t ? " active" : ""}`}
                    aria-pressed={tag === t}
                    onClick={() => setTag(t)}
                  >
                    {t ?? text.allTags}
                  </button>
                ))}
              </span>
            )}
          </div>
          {shown.length === 0 && <p class="empty-search">{text.noMatch}</p>}
          <ul class="entries-list">
            {shown.flatMap((entry, index) => {
              // A flat list of keyed rows: headings are their own items, so moving a row keeps its element.
              const heading =
                grouped && index === 0
                  ? text.pinned
                  : grouped && !entry.pinned && shown[index - 1]?.pinned
                    ? text.others
                    : null;
              const row = (
                <EntryRow
                  key={entry.id}
                  entry={entry}
                  coreNow={coreNow}
                  copied={copiedId === entry.id}
                  text={text}
                  isMac={isMac}
                  sortable={sortable}
                  previous={shown[index - 1]?.id}
                  next={shown[index + 1]?.id}
                  onCopy={(e) => void copy(e)}
                  onSave={(change) => save(entry.id, change)}
                  keyApi={api}
                  onChanged={(message) => {
                    setToast(message);
                    void load(query);
                  }}
                  onMove={(to) => after(api.move(entry.id, to))}
                  onTrash={() => after(api.trash(entry.id), text.trashed)}
                  loadHistory={() => api.history(entry.id)}
                  onDropRow={(moved, place) => void after(api.move(moved, { to: place, other: entry.id }))}
                  selection={selecting ? { chosen: chosen.includes(entry.id), onToggle: () => toggle(entry.id) } : undefined}
                />
              );
              return heading === null
                ? [row]
                : [
                    <li key={`heading-${heading}`} class="group-title">
                      {heading}
                    </li>,
                    row,
                  ];
            })}
          </ul>
        </main>
      )}
      {selecting && (
        <BatchBar
          count={chosen.length}
          tags={chosenTags}
          text={text}
          onSelectAll={() => setChosen(shown.map((e) => e.id))}
          onSelectNone={() => setChosen([])}
          onApply={async (action) => {
            const changed = await api.batch(chosen, action);
            setToast(action.action === "trash" ? text.batchTrashed(changed) : text.batchDone(changed));
            if (action.action === "trash") setChosen([]);
            await load(query);
          }}
          onExport={() => setExporting({ ids: chosen, count: chosen.length })}
          onDone={() => {
            setSelecting(false);
            setChosen([]);
          }}
        />
      )}
      {exporting && (
        <ExportDialog
          api={api}
          text={text}
          ids={exporting.ids}
          count={exporting.count}
          onExported={setToast}
          onClose={() => setExporting(null)}
        />
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
      {syncOpen && <SyncDialog api={api} text={syncStrings(text.locale)} clock={clock} onClose={() => setSyncOpen(false)} />}
      {settingsOpen && (
        <SettingsDialog
          api={api}
          text={text}
          isMac={isMac}
          onSaved={(settings) => {
            setToast(text.saved);
            onSettingsSaved?.(settings);
          }}
          onClose={() => setSettingsOpen(false)}
        />
      )}
      {trashOpen && (
        <TrashDialog
          api={api}
          text={text}
          now={coreNow}
          onChanged={(message) => {
            setToast(message);
            void load(query);
          }}
          onClose={() => setTrashOpen(false)}
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
