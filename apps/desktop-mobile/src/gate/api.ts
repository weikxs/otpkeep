/**
 * The management page's connection to OTPKeep (design/07-platform.md §2.5): the one-time link token after
 * `#k=` is traded for a session token, which every request carries. Replies never contain secrets or codes:
 * copying happens inside OTPKeep, which writes this computer's clipboard.
 */

export interface GateEntry {
  id: string;
  title: string;
  issuer: string;
  account: string;
  note: string;
  pinned: boolean;
  kind: "totp" | "hotp";
  /** Its own global shortcut, canonical (`control+alt+super+KeyG`). */
  shortcut?: string;
  /** The default its place in the list gives it. */
  defaultShortcut?: string;
}

export interface EntryEdit {
  title?: string;
  note?: string;
}

/** A refusal from OTPKeep: `code` names the rule, `detail` says more where there is more to say. */
export class GateFailure extends Error {
  constructor(
    readonly code: string,
    readonly detail?: string,
  ) {
    super(code);
  }
}

export interface GateApi {
  entries(query: string): Promise<GateEntry[]>;
  edit(id: string, edit: EntryEdit): Promise<GateEntry>;
  setShortcut(id: string, shortcut: string | null): Promise<GateEntry>;
  /** OTPKeep copies the entry's current code; returns seconds until the clipboard is cleared (0: never). */
  copy(id: string): Promise<{ clearsInS: number }>;
  heartbeat(): Promise<void>;
  end(): void;
}

const SESSION_KEY = "otpkeep-session";

type Fetch = typeof fetch;

/** The session for this tab: from the link in the address, or one this tab opened before. */
export async function connect(win: Window, fetchFn: Fetch = fetch): Promise<GateApi> {
  const link = new URLSearchParams(win.location.hash.slice(1)).get("k");
  if (link) {
    // The token must not stay in the address bar or the history.
    win.history.replaceState(null, "", win.location.pathname);
    const reply = await fetchFn("/api/session", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ link }),
    });
    const body = await readJson(reply);
    if (!reply.ok) throw failure(body);
    const session = (body as { session: string }).session;
    store(win, session);
    return client(session, fetchFn);
  }
  const saved = load(win);
  if (saved) return client(saved, fetchFn);
  throw new GateFailure("NO_SESSION");
}

function store(win: Window, session: string): void {
  try {
    win.sessionStorage.setItem(SESSION_KEY, session);
  } catch {
    // Without storage a reload needs a new link; the page still works.
  }
}

function load(win: Window): string | null {
  try {
    return win.sessionStorage.getItem(SESSION_KEY);
  } catch {
    return null;
  }
}

/** Forgets this tab's session. */
export function forget(win: Window): void {
  try {
    win.sessionStorage.removeItem(SESSION_KEY);
  } catch {
    // Nothing stored.
  }
}

async function readJson(reply: Response): Promise<unknown> {
  try {
    return await reply.json();
  } catch {
    return null;
  }
}

function failure(body: unknown): GateFailure {
  const error = (body as { error?: { code?: unknown; detail?: unknown } } | null)?.error;
  const code = typeof error?.code === "string" ? error.code : "UNKNOWN";
  const detail = typeof error?.detail === "string" ? error.detail : undefined;
  return new GateFailure(code, detail);
}

function client(session: string, fetchFn: Fetch): GateApi {
  const call = async (method: string, path: string, body?: unknown): Promise<unknown> => {
    const reply = await fetchFn(path, {
      method,
      headers: {
        Authorization: `Bearer ${session}`,
        ...(body === undefined ? {} : { "Content-Type": "application/json" }),
      },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    if (reply.status === 204) return null;
    const parsed = await readJson(reply);
    if (!reply.ok) throw failure(parsed);
    return parsed;
  };
  return {
    entries: async (query) =>
      ((await call("GET", `/api/entries?q=${encodeURIComponent(query)}`)) as { entries: GateEntry[] }).entries,
    edit: async (id, edit) =>
      ((await call("PATCH", `/api/entries/${encodeURIComponent(id)}`, edit)) as { entry: GateEntry }).entry,
    setShortcut: async (id, shortcut) =>
      ((await call("PUT", `/api/entries/${encodeURIComponent(id)}/shortcut`, { shortcut })) as { entry: GateEntry })
        .entry,
    copy: async (id) =>
      (await call("POST", `/api/entries/${encodeURIComponent(id)}/copy`)) as { clearsInS: number },
    heartbeat: async () => {
      await call("POST", "/api/heartbeat");
    },
    end: () => {
      void fetchFn("/api/end", {
        method: "POST",
        headers: { Authorization: `Bearer ${session}` },
        keepalive: true,
      }).catch(() => {});
    },
  };
}
