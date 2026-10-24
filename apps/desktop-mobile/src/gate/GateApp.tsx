import { useCallback, useEffect, useState } from "preact/hooks";
import { GateFailure, type GateApi, type GateEntry } from "./api";
import { EntryCard } from "./EntryCard";
import { type GateStrings, describeFailure } from "./i18n";

/** The page sends a heartbeat this often; the session ends 45 seconds after the last one. */
export const HEARTBEAT_MS = 15_000;
const NOTICE_MS = 2400;

type Connection =
  | { kind: "connecting" }
  | { kind: "ready"; api: GateApi }
  | { kind: "failed"; code: string };

export interface GateAppProps {
  connect: () => Promise<GateApi>;
  text: GateStrings;
  isMac: boolean;
}

/** The browser management page: search, edit names and notes, set shortcuts. */
export function GateApp({ connect, text, isMac }: GateAppProps) {
  const [connection, setConnection] = useState<Connection>({ kind: "connecting" });

  useEffect(() => {
    connect().then(
      (api) => setConnection({ kind: "ready", api }),
      (failure: unknown) =>
        setConnection({ kind: "failed", code: failure instanceof GateFailure ? failure.code : "NO_SESSION" }),
    );
  }, [connect]);

  const lost = useCallback((code: string) => setConnection({ kind: "failed", code }), []);

  return (
    <div class="gate">
      <header class="gate-header">
        <h1>{text.title}</h1>
      </header>
      {connection.kind === "connecting" && <p class="gate-message">{text.connecting}</p>}
      {connection.kind === "failed" && <Ended code={connection.code} text={text} />}
      {connection.kind === "ready" && <Entries api={connection.api} text={text} isMac={isMac} onLost={lost} />}
    </div>
  );
}

function Ended({ code, text }: { code: string; text: GateStrings }) {
  if (code === "NEEDS_PASSWORD") return <p class="gate-message">{text.needsPassword}</p>;
  const used = code === "LINK_USED";
  return (
    <section class="gate-message" role="alert">
      <h2>{used ? text.linkUsed : text.ended}</h2>
      <p>{used ? text.linkUsedBody : text.endedBody}</p>
    </section>
  );
}

interface EntriesProps {
  api: GateApi;
  text: GateStrings;
  isMac: boolean;
  onLost: (code: string) => void;
}

function Entries({ api, text, isMac, onLost }: EntriesProps) {
  const [query, setQuery] = useState("");
  const [entries, setEntries] = useState<GateEntry[] | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  const handle = useCallback(
    (failure: unknown) => {
      const code = failure instanceof GateFailure ? failure.code : "UNKNOWN";
      if (code === "NO_SESSION" || code === "NEEDS_PASSWORD") onLost(code);
      else setProblem(describeFailure(text, code));
    },
    [onLost, text],
  );

  const load = useCallback(
    (q: string) =>
      api.entries(q).then(
        (found) => {
          setEntries(found);
          setProblem(null);
        },
        handle,
      ),
    [api, handle],
  );

  useEffect(() => {
    void load(query);
  }, [query, load]);

  useEffect(() => {
    const timer = setInterval(() => void api.heartbeat().catch(handle), HEARTBEAT_MS);
    return () => clearInterval(timer);
  }, [api, handle]);

  useEffect(() => {
    if (notice === null) return;
    const timer = setTimeout(() => setNotice(null), NOTICE_MS);
    return () => clearTimeout(timer);
  }, [notice]);

  const replace = (updated: GateEntry) => {
    setEntries((current) => current?.map((e) => (e.id === updated.id ? updated : e)) ?? null);
    return updated;
  };

  const guarded = async (call: () => Promise<GateEntry>) => {
    try {
      return replace(await call());
    } catch (failure) {
      if (failure instanceof GateFailure && (failure.code === "NO_SESSION" || failure.code === "NEEDS_PASSWORD")) {
        onLost(failure.code);
      }
      throw failure;
    }
  };

  return (
    <main class="gate-main">
      <div class="gate-toolbar">
        <input
          type="search"
          class="gate-search"
          value={query}
          placeholder={text.search}
          aria-label={text.search}
          onInput={(e) => setQuery((e.target as HTMLInputElement).value)}
        />
        {entries && <span class="muted">{text.count(entries.length)}</span>}
      </div>
      {problem && (
        <p class="field-error" role="alert">
          {problem}
        </p>
      )}
      {entries?.length === 0 && <p class="gate-message">{query === "" ? text.empty : text.noMatch}</p>}
      {entries && entries.length > 0 && (
        <ul class="cards">
          {entries.map((entry) => (
            <EntryCard
              key={entry.id}
              entry={entry}
              text={text}
              isMac={isMac}
              onEdit={(edit) => guarded(() => api.edit(entry.id, edit))}
              onShortcut={(shortcut) => guarded(() => api.setShortcut(entry.id, shortcut))}
              onNotice={setNotice}
            />
          ))}
        </ul>
      )}
      {notice && (
        <div class="toast" role="status">
          {notice}
        </div>
      )}
    </main>
  );
}
