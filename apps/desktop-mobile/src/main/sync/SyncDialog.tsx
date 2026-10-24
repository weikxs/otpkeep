import { useCallback, useEffect, useState } from "preact/hooks";
import { GroupHome } from "./GroupHome";
import type { SyncStrings } from "./i18n";
import { codeText } from "./i18n";
import { JoinFlow } from "./JoinFlow";
import { SessionPanel } from "./SessionPanel";
import type { ResultView, SyncApi, SyncState } from "./types";

export interface SyncDialogProps {
  api: SyncApi & { onChange: (listener: () => void) => () => void };
  text: SyncStrings;
  clock: () => number;
  onClose: () => void;
}

/** The sync page (design/06-sync.md §8). */
export function SyncDialog({ api, text, clock, onClose }: SyncDialogProps) {
  const [state, setState] = useState<SyncState | null>(null);
  const [looking, setLooking] = useState<"sync" | "pair" | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = useCallback(() => api.syncState().then(setState), [api]);

  useEffect(() => void load(), [load]);
  useEffect(() => api.onChange(() => void load()), [api, load]);
  useEffect(
    () =>
      api.onSyncView((sync) => {
        setState((current) => current && { ...current, sync });
        // A session that started or ended changes the group too (a pairing founds or joins one).
        void load();
        if (sync.session) setLooking(null);
      }),
    [api, load],
  );

  const session = state?.sync.session;
  const last = state?.sync.last;

  return (
    <div class="overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <section class="dialog sync-dialog" role="dialog" aria-modal="true" aria-label={text.title} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <header class="dialog-header">
          <h2>{text.title}</h2>
          <button type="button" class="icon-button" aria-label={text.close} onClick={onClose}>
            ×
          </button>
        </header>
        <p class="muted">{text.intro}</p>
        {last && !session && <LastResult last={last} text={text} />}
        {notice && !session && <p class="sample-line">{notice}</p>}
        {state &&
          (session ? (
            <SessionPanel session={session} api={api} text={text} clock={clock} />
          ) : looking ? (
            <JoinFlow looking={looking} hasEntries={state.group.hasEntries} api={api} text={text} onBack={() => setLooking(null)} />
          ) : (
            <GroupHome
              group={state.group}
              api={api}
              text={text}
              now={clock()}
              onJoin={setLooking}
              onChanged={(message) => {
                setNotice(message);
                void load();
              }}
            />
          ))}
      </section>
    </div>
  );
}

function LastResult({ last, text }: { last: ResultView; text: SyncStrings }) {
  const name = last.peer?.name ?? "";
  // The window's own person said no: word it from their side.
  const mine = last.kind === "syncWindow" || last.kind === "pairWindow";
  const message = last.ok
    ? last.code === "PAIRED"
      ? text.paired(name)
      : text.synced(name, last.entries ?? 0)
    : mine && last.code === "REJECTED"
      ? text.youDeclined
      : mine && last.code === "SAS_MISMATCH"
        ? text.checkMismatch
        : codeText(last.code);
  return (
    <p class={last.ok ? "result ok" : "result failed"} role="status">
      {message}
    </p>
  );
}
