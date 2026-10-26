import { useCallback, useEffect, useMemo, useRef, useState } from "preact/hooks";
import { type LanguageControl, LanguageSelect } from "../shared/LanguageSelect";
import { type AddTab, AddDialog } from "./AddDialog";
import { CommandFailure, type Entry, type MainApi, type MainState, type MoveTo, type Settings } from "./api";
import type { EntryChange } from "./EntryEditor";
import { ENTRY_DRAG_TYPE, EntryRow } from "./EntryRow";
import { BatchBar } from "./BatchBar";
import { ExportDialog } from "./ExportDialog";
import { SettingsDialog } from "./SettingsDialog";
import { syncStrings } from "./sync/i18n";
import { SyncDialog } from "./sync/SyncDialog";
import { ToolButton } from "./Toolbar";
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
  /** The language menu in the top bar; absent hides it. */
  language?: LanguageControl;
}

type Dialog = { tab: AddTab; dropped?: Uint8Array } | null;

/** The main window: every account, search, and adding (design/07-platform.md §2.4a). */
export function MainApp({ api, text, isMac, clock = Date.now, onSettingsSaved, language }: MainAppProps) {
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
  /** An entry the search panel asked to edit; its row opens its editor once shown. */
  const [editId, setEditId] = useState<string | null>(null);

  /** Asks for the list again; the trash only changes with the data, so a code rolling over skips it. */
  const load = useCallback(
    async (q: string, withTrash = true) => {
      const next = await api.state(q);
      setOffset(next.nowMs - clock());
      setNow(clock());
      setState(next);
      if (q === "") setAllCount(next.entries.length);
      if (withTrash && next.status === "ready") setTrashCount((await api.trashList()).length);
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
        if (page.startsWith("edit:")) {
          // Show every entry so the one asked for is on screen.
          setQuery("");
          setTag(null);
          setSelecting(false);
          setEditId(page.slice("edit:".length));
        }
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
    if (expired) void load(query, false);
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
    // One call for every changed field: the vault is written once.
    await api.update(id, change);
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

  const entries = state?.entries;
  const allTags = useMemo(
    () => [...new Set((entries ?? []).flatMap((e) => e.tags))].sort((a, b) => a.localeCompare(b)),
    [entries],
  );
  const shown = useMemo(
    () => (tag === null ? (entries ?? []) : (entries ?? []).filter((e) => e.tags.includes(tag))),
    [entries, tag],
  );
  const chosenTags = useMemo(
    () => [...new Set(shown.filter((e) => chosen.includes(e.id)).flatMap((e) => e.tags))].sort(),
    [shown, chosen],
  );
  const toggle = (id: string) =>
    setChosen((current) => (current.includes(id) ? current.filter((c) => c !== id) : [...current, id]));
  // Rows are drawn again only when what they show changes (EntryRow's sameRow), so their handlers go through
  // this reference to the current render's functions instead of the ones from when the row was last drawn.
  const current = useRef({ copy, save, after, load: () => load(query), setToast, setEditId, toggle });
  current.current = { copy, save, after, load: () => load(query), setToast, setEditId, toggle };
  const rowActions = useMemo(
    () => ({
      copy: (entry: Entry) => void current.current.copy(entry),
      save: (id: string, change: EntryChange) => current.current.save(id, change),
      changed: (message: string) => {
        current.current.setToast(message);
        void current.current.load();
      },
      move: (id: string, to: MoveTo) => current.current.after(api.move(id, to)),
      trash: (id: string) => current.current.after(api.trash(id), text.trashed),
      dropRow: (id: string, moved: string, place: "before" | "after") =>
        void current.current.after(api.move(moved, { to: place, other: id })),
      toggle: (id: string) => current.current.toggle(id),
      editStarted: () => current.current.setEditId(null),
    }),
    [api, text],
  );

  if (state === null) return <div class="main" aria-busy="true" />;
  const languageMenu = language && (
    <LanguageSelect control={language} systemLabel={text.languages.system} label={text.language} />
  );
  if (state.status === "needsPassword") {
    return (
      <>
        {languageMenu && <span class="corner-language">{languageMenu}</span>}
        <PasswordScreen api={api} text={text} onOpened={() => void load(query)} />
      </>
    );
  }
  if (state.status === "unavailable") {
    return (
      <main class="welcome" role="alert">
        {languageMenu && <span class="corner-language">{languageMenu}</span>}
        <h1>{text.unavailable}</h1>
        {state.problem && <p class="muted">{text.problems[state.problem]}</p>}
      </main>
    );
  }

  const empty = state.entries.length === 0 && query === "" && trashCount === 0;
  const sortable = query === "" && tag === null && !selecting;
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
          {!empty && <ToolButton icon="trash" label={text.trashButton(trashCount)} onClick={() => setTrashOpen(true)} />}
          <ToolButton icon="sync" label={text.syncButton} onClick={() => setSyncOpen(true)} />
          {!empty && (
            <ToolButton icon="export" label={text.exportButton} onClick={() => setExporting({ ids: null, count: allCount })} />
          )}
          <ToolButton icon="settings" label={text.settings} onClick={() => setSettingsOpen(true)} />
          <ToolButton icon="browser" label={text.manage} onClick={() => void api.openGate()} />
          {languageMenu}
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
                  onCopy={rowActions.copy}
                  onSave={(change) => rowActions.save(entry.id, change)}
                  keyApi={api}
                  onChanged={rowActions.changed}
                  onMove={(to) => rowActions.move(entry.id, to)}
                  onTrash={() => rowActions.trash(entry.id)}
                  loadHistory={() => api.history(entry.id)}
                  onDropRow={(moved, place) => rowActions.dropRow(entry.id, moved, place)}
                  selection={selecting ? { chosen: chosen.includes(entry.id), onToggle: () => rowActions.toggle(entry.id) } : undefined}
                  startEditing={editId === entry.id}
                  onEditStarted={rowActions.editStarted}
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
  // An empty field is not a wrong password: it is not tried at all.
  const [problem, setProblem] = useState<"empty" | "wrong" | null>(null);
  return (
    <main class="welcome">
      <h1>{text.needsPassword}</h1>
      <form
        class="password-step"
        onSubmit={(e) => {
          e.preventDefault();
          if (password === "") {
            setProblem("empty");
            return;
          }
          void api.unlock(password).then((next) => {
            setPassword("");
            if (next.status === "needsPassword") setProblem("wrong");
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
            setProblem(null);
          }}
        />
        <button type="submit" class="primary">
          {text.open}
        </button>
        {problem && (
          <p class="field-error" role="alert">
            {problem === "empty" ? text.enterPassword : text.wrongPassword}
          </p>
        )}
      </form>
    </main>
  );
}
