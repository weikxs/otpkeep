import { useEffect, useState } from "preact/hooks";
import { CommandFailure } from "../api";
import { svgUrl } from "../QrDialogs";
import type { SyncStrings } from "./i18n";
import { codeText } from "./i18n";
import type { SessionView, SyncApi } from "./types";

export interface SessionPanelProps {
  session: SessionView;
  api: Pick<SyncApi, "answerAsk" | "closeSession" | "pairInvite">;
  text: SyncStrings;
  clock: () => number;
}

/** Groups digits for reading aloud: `482 913`, `7391 5206`. */
export function spaced(code: string): string {
  const size = code.length === 8 ? 4 : 3;
  return code.replace(new RegExp(`(\\d{${size}})(?=\\d)`, "gu"), "$1 ");
}

/** A window or a joining session in progress (design/06-sync.md §8, "窗口状态"). */
export function SessionPanel({ session, api, text, clock }: SessionPanelProps) {
  const [now, setNow] = useState(clock());
  const [invite, setInvite] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const timer = setInterval(() => setNow(clock()), 500);
    return () => clearInterval(timer);
  }, [clock]);

  const showInvite = async () => {
    try {
      setInvite(await api.pairInvite());
    } catch (thrown) {
      if (!(thrown instanceof CommandFailure && thrown.code === "CANCELLED")) {
        setError(codeText(thrown instanceof CommandFailure ? thrown.code : "UNKNOWN"));
      }
    }
  };

  const answer = async (yes: boolean) => {
    if (!session.ask) return;
    try {
      await api.answerAsk(session.ask.id, yes);
    } catch (thrown) {
      setError(codeText(thrown instanceof CommandFailure ? thrown.code : "UNKNOWN"));
    }
  };

  const peerName = session.peer?.name ?? "";
  const pair = session.kind === "pairWindow";
  const window = session.kind === "syncWindow" || pair;
  const left = session.closesAtMs === undefined ? undefined : Math.max(0, Math.ceil((session.closesAtMs - now) / 1000));

  return (
    <section class="session" aria-live="polite">
      {session.ask ? (
        <div class="ask" role="alertdialog" aria-label={session.ask.kind === "allow" ? text.allowTitle : text.checkTitle}>
          <p class="preview-title">{session.ask.kind === "allow" ? text.allowTitle : text.checkTitle}</p>
          {session.ask.kind === "allow" && session.peer ? (
            <p>{text.allowBody(session.peer.name, session.peer.fingerprint, session.peer.platform)}</p>
          ) : (
            <>
              <p>{text.checkBody(peerName)}</p>
              <p class="big-code" data-testid="sas">
                {spaced(session.ask.sas ?? "")}
              </p>
            </>
          )}
          <p class="muted">{text.answerWithin(Math.max(0, Math.ceil((session.ask.deadlineMs - now) / 1000)))}</p>
          <div class="form-actions">
            <button type="button" class="primary" onClick={() => void answer(true)}>
              {session.ask.kind === "allow" ? text.allow : text.same}
            </button>
            <button type="button" class="secondary" onClick={() => void answer(false)}>
              {session.ask.kind === "allow" ? text.decline : text.different}
            </button>
          </div>
        </div>
      ) : session.phase === "exchanging" ? (
        <p class="session-line">{text.exchanging(peerName)}</p>
      ) : window && session.code ? (
        <div class="window">
          <p class="field-label">{pair ? text.pairCodeTitle : text.syncCodeTitle}</p>
          <p class="big-code" data-testid="window-code">
            {spaced(session.code)}
          </p>
          <p class="muted">{pair ? text.pairCodeHint : text.syncCodeHint}</p>
          {left !== undefined && <p class="muted">{text.closesIn(left)}</p>}
          {pair && session.hasInvite && !invite && (
            <button type="button" class="link-button" onClick={() => void showInvite()}>
              {text.showInvite}
            </button>
          )}
          {invite && (
            <>
              <img class="qr small-qr" src={svgUrl(invite)} alt={text.showInvite} />
              <p class="muted">{text.inviteHint}</p>
            </>
          )}
          {session.skipped.map((s) => (
            <p key={s.via} class="preview-note">
              {text.skipped(s.via, s.code)}
            </p>
          ))}
        </div>
      ) : (
        <p class="session-line">
          {session.kind === "syncJoin" ? text.waitingAllow(peerName) : peerName ? text.connecting(peerName) : text.waiting}
        </p>
      )}
      {error && (
        <p class="field-error" role="alert">
          {error}
        </p>
      )}
      <div class="form-actions">
        <button type="button" class="secondary" onClick={() => void api.closeSession()}>
          {text.stop}
        </button>
      </div>
    </section>
  );
}
