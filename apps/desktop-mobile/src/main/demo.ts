import {
  CommandFailure,
  type Entry,
  type History,
  type ImportRow,
  type ImportView,
  type MainApi,
  type MainState,
  type TrashItem,
} from "./api";

/** Made-up accounts and a pretend import, for the browser preview and tests. Codes come from the clock. */

const BASE32 = /^[A-Z2-7]+$/u;
const PERIOD_MS = 30_000;
const NEXT_WINDOW_MS = 5_000;
const TRASH_KEEP_MS = 30 * 24 * 60 * 60 * 1000;
const TAG_MAX = 30;
const TAGS_MAX = 20;

function fakeCode(seed: string, step: number, steam = false): string {
  let hash = step >>> 0;
  for (const char of seed) hash = Math.imul(hash ^ (char.codePointAt(0) ?? 0), 2654435761) >>> 0;
  if (!steam) return String(hash % 1_000_000).padStart(6, "0");
  const alphabet = "23456789BCDFGHJKMNPQRTVWXY";
  return Array.from({ length: 5 }, (_, i) => alphabet[Math.floor(hash / 26 ** i) % 26]).join("");
}

interface DemoAccount {
  id: string;
  title: string;
  issuer: string;
  account: string;
  note: string;
  kind: Entry["kind"];
  secret: string;
  pinned?: boolean;
  tags?: string[];
  trashedAt?: number;
}

const START: DemoAccount[] = [
  { id: "a1", title: "GitHub", issuer: "GitHub", account: "demo-user", note: "", kind: "totp", secret: "GH" },
  { id: "a2", title: "邮箱", issuer: "示例邮箱", account: "demo@example.com", note: "备用码在抽屉里", kind: "totp", secret: "MAIL", tags: ["工作"] },
  { id: "a3", title: "Steam", issuer: "Steam", account: "player", note: "", kind: "steam", secret: "STEAM" },
  { id: "a4", title: "Bank token", issuer: "Example Bank", account: "0000", note: "", kind: "hotp", secret: "BANK" },
];

export interface DemoOptions {
  clock?: () => number;
  empty?: boolean;
  needsPassword?: boolean;
}

export function demoMain({ clock = Date.now, empty = false, needsPassword = false }: DemoOptions = {}): MainApi & {
  copied: string[];
} {
  let accounts = empty ? [] : START.map((a) => ({ ...a }));
  let locked = needsPassword;
  const generated = new Map<string, string>();
  const staged = new Map<string, { rows: ImportRow[]; accounts: DemoAccount[] }>();
  const copied: string[] = [];
  const history = new Map<string, History>();
  let tokens = 0;
  const live = () => {
    const kept = accounts.filter((a) => a.trashedAt === undefined);
    return [...kept.filter((a) => a.pinned), ...kept.filter((a) => !a.pinned)];
  };
  const change = (id: string, update: (a: DemoAccount) => DemoAccount) => {
    if (!accounts.some((a) => a.id === id)) throw new CommandFailure("NOT_FOUND");
    accounts = accounts.map((a) => (a.id === id ? update(a) : a));
  };

  const entry = (a: DemoAccount, now: number): Entry => {
    if (a.kind === "hotp") {
      const code = generated.get(a.id);
      return { ...a, tags: a.tags ?? [], pinned: a.pinned ?? false, ...(code === undefined ? {} : { code }) };
    }
    const step = Math.floor(now / PERIOD_MS);
    const validUntilMs = (step + 1) * PERIOD_MS;
    const steam = a.kind === "steam";
    return {
      ...a,
      tags: a.tags ?? [],
      pinned: a.pinned ?? false,
      code: fakeCode(a.secret, step, steam),
      periodS: 30,
      validUntilMs,
      ...(validUntilMs - now <= NEXT_WINDOW_MS ? { nextCode: fakeCode(a.secret, step + 1, steam) } : {}),
    };
  };

  const state = (query: string): MainState => {
    const now = clock();
    const q = query.trim().toLowerCase();
    const found = live().filter(
      (a) => q === "" || `${a.title} ${a.issuer} ${a.account} ${a.note} ${(a.tags ?? []).join(" ")}`.toLowerCase().includes(q),
    );
    return {
      status: locked ? "needsPassword" : "ready",
      entries: locked ? [] : found.map((a) => entry(a, now)),
      nowMs: now,
      usesMasterPassword: needsPassword,
      inGroup: false,
      readOnly: false,
    };
  };

  const stage = (incoming: DemoAccount[], skipped: ImportView["skipped"] = []): ImportView => {
    const token = `t${++tokens}`;
    let fresh = 0;
    let dup = 0;
    const rows: ImportRow[] = incoming.map((a) => {
      const existing = accounts.find((e) => e.secret === a.secret);
      const base = {
        title: a.title,
        issuer: a.issuer,
        account: a.account,
        kind: a.kind,
        algorithm: "SHA1",
        digits: a.kind === "steam" ? 5 : 6,
        titleTaken: false,
        lost: [],
        ...(a.kind === "hotp" ? { counter: 0 } : { periodS: 30, sampleCode: fakeCode(a.secret, Math.floor(clock() / PERIOD_MS), a.kind === "steam") }),
      };
      return existing
        ? { ...base, group: "duplicate" as const, index: dup++, existingTitle: existing.title }
        : { ...base, group: "fresh" as const, index: fresh++ };
    });
    staged.set(token, { rows, accounts: incoming });
    return { token, rows, skipped };
  };

  const fromLink = (line: string, n: number): DemoAccount => {
    const label = decodeURIComponent(line.split("/")[3]?.split("?")[0] ?? "Demo");
    const [issuer = "", account = label] = label.includes(":") ? label.split(":") : ["", label];
    const secret = /secret=([^&]+)/u.exec(line)?.[1] ?? `S${n}`;
    return { id: `n${n}`, title: issuer || account, issuer, account, note: "", kind: "totp", secret };
  };

  return {
    copied,
    state: async (query) => state(query),
    copy: async (id) => {
      copied.push(id);
      return { clearsInS: 30 };
    },
    nextHotp: async (id) => {
      generated.set(id, fakeCode(id, generated.size + 7));
      const account = accounts.find((a) => a.id === id);
      if (!account) throw new CommandFailure("NOT_FOUND");
      return entry(account, clock());
    },
    edit: async (id, edit) => {
      if (edit.title !== undefined && edit.title.trim() === "") throw new CommandFailure("INVALID", "TITLE_EMPTY");
      const before = accounts.find((a) => a.id === id);
      if (before) {
        const old = history.get(id) ?? { title: [], note: [] };
        history.set(id, {
          title: edit.title !== undefined ? [{ value: before.title, atMs: clock() }, ...old.title] : old.title,
          note: edit.note !== undefined ? [{ value: before.note, atMs: clock() }, ...old.note] : old.note,
        });
      }
      change(id, (a) => ({ ...a, ...edit }));
    },
    setPinned: async (id, pinned) => change(id, (a) => ({ ...a, pinned })),
    setTags: async (id, tags) => {
      const clean = [...new Set(tags.map((t) => t.trim()).filter((t) => t !== ""))];
      if (clean.some((t) => t.length > TAG_MAX)) throw new CommandFailure("INVALID", "TAG_TOO_LONG");
      if (clean.length > TAGS_MAX) throw new CommandFailure("INVALID", "TOO_MANY_TAGS");
      change(id, (a) => ({ ...a, tags: clean }));
    },
    move: async (id, to) => {
      const moving = accounts.find((a) => a.id === id);
      if (!moving) throw new CommandFailure("NOT_FOUND");
      const rest = accounts.filter((a) => a.id !== id);
      const at =
        to.to === "first" ? 0 : to.to === "last" ? rest.length : rest.findIndex((a) => a.id === to.other) + (to.to === "after" ? 1 : 0);
      accounts = [...rest.slice(0, at), moving, ...rest.slice(at)];
    },
    trash: async (id) => change(id, (a) => ({ ...a, trashedAt: clock() })),
    restore: async (id) => change(id, ({ trashedAt: _gone, ...a }) => a),
    purge: async (id) => {
      accounts = accounts.filter((a) => a.id !== id);
    },
    trashList: async (): Promise<TrashItem[]> =>
      accounts
        .filter((a) => a.trashedAt !== undefined)
        .map((a) => ({
          id: a.id,
          title: a.title,
          issuer: a.issuer,
          account: a.account,
          trashedAtMs: a.trashedAt ?? 0,
          purgeAtMs: (a.trashedAt ?? 0) + TRASH_KEEP_MS,
        })),
    history: async (id) => history.get(id) ?? { title: [], note: [] },
    unlock: async (password) => {
      if (password === "demo") locked = false;
      return state("");
    },
    stageText: async (text) => {
      const lines = text.split("\n").map((l) => l.trim()).filter((l) => l !== "");
      const skipped: ImportView["skipped"] = [];
      const found: DemoAccount[] = [];
      lines.forEach((line, i) => {
        if (line.toLowerCase().includes("otpauth://")) found.push(fromLink(line.slice(line.toLowerCase().indexOf("otpauth://")), tokens * 10 + i));
        else if (line.startsWith("phonefactor://")) skipped.push({ place: { kind: "line", line: i + 1 }, reason: "MICROSOFT_PUSH" });
        else skipped.push({ place: { kind: "line", line: i + 1 }, reason: "NOT_A_LINK" });
      });
      return stage(found, skipped);
    },
    stageFile: async (bytes, password) => {
      const head = new TextDecoder().decode(bytes.slice(0, 3));
      if (head === "OKV") {
        if (password === undefined) throw new CommandFailure("BACKUP_PASSWORD_NEEDED");
        if (password !== "demo") throw new CommandFailure("BACKUP_PASSWORD_WRONG");
        return stage([{ id: "b1", title: "Backed up", issuer: "Demo", account: "alex", note: "", kind: "totp", secret: "BACKUP" }]);
      }
      if (head !== "PNG" && bytes[1] !== 0x50) throw new CommandFailure("UNKNOWN_FORMAT");
      return stage([{ id: "q1", title: "From picture", issuer: "Picture", account: "alex", note: "", kind: "totp", secret: "PICTURE" }]);
    },
    stageManual: async (form) => {
      const secret = form.secret.toUpperCase().replace(/[\s-]/gu, "");
      if (secret === "") throw new CommandFailure("INVALID", "SECRET_MISSING");
      const bad = [...secret].findIndex((c) => !BASE32.test(c));
      if (bad >= 0) throw new CommandFailure("SECRET_INVALID", `${bad + 1}:${secret[bad]}`);
      if (form.issuer === "" && form.account === "") throw new CommandFailure("INVALID", "NAME_MISSING");
      return stage([{ id: `m${tokens}`, title: form.issuer || form.account, issuer: form.issuer, account: form.account, note: "", kind: form.kind, secret }]);
    },
    stagedView: async (token) => {
      const found = staged.get(token);
      if (!found) throw new CommandFailure("IMPORT_GONE");
      return { token, rows: found.rows, skipped: [] };
    },
    commit: async (token, fresh, duplicates) => {
      const found = staged.get(token);
      if (!found) throw new CommandFailure("IMPORT_GONE");
      staged.delete(token);
      const chosen = found.rows
        .map((row, i) => ((row.group === "fresh" ? fresh[row.index] : duplicates[row.index]) ? found.accounts[i] : undefined))
        .filter((a): a is DemoAccount => a !== undefined)
        .map((a, i) => ({ ...a, id: `${token}-${i}` }));
      accounts = [...accounts, ...chosen];
      return chosen.length;
    },
    cancel: async (token) => {
      staged.delete(token);
    },
    openGate: async () => {},
    onChange: () => () => {},
    onPage: () => () => {},
  };
}
