import type { CopyResult, PanelApi, PanelEntry, PanelState } from "./api";

/** Made-up accounts for the browser preview and tests; the codes are derived from the clock, not secrets. */
const DEMO_ENTRIES: Omit<PanelEntry, "code" | "nextCode" | "validUntilMs" | "shortcut">[] = [
  { id: "d1", title: "GitHub", issuer: "GitHub", account: "demo-user", kind: "totp", pinned: true, periodS: 30 },
  { id: "d2", title: "邮箱", issuer: "示例邮箱", account: "demo@example.com", kind: "totp", pinned: true, periodS: 30 },
  { id: "d3", title: "Cloud Console", issuer: "Example Cloud", account: "ops@example.com", kind: "totp", pinned: false, periodS: 30 },
  { id: "d4", title: "VPN", issuer: "Example VPN", account: "demo", kind: "totp", pinned: false, periodS: 60 },
  { id: "d5", title: "Bank token", issuer: "Example Bank", account: "0000", kind: "hotp", pinned: false },
  { id: "d6", title: "Domain registrar", issuer: "Registrar", account: "demo@example.org", kind: "totp", pinned: false, periodS: 30 },
];

const CLEARS_IN_S = 30;
const NEXT_WINDOW_MS = 5000;

function fakeCode(id: string, step: number): string {
  let hash = step >>> 0;
  for (const char of id) hash = Math.imul(hash ^ (char.codePointAt(0) ?? 0), 2654435761) >>> 0;
  return String(hash % 1_000_000).padStart(6, "0");
}

export interface DemoOptions {
  clock?: () => number;
  status?: PanelState["status"];
  empty?: boolean;
}

/** A PanelApi with demo data, for previews and tests. */
export function demoApi({ clock = Date.now, status = "ready", empty = false }: DemoOptions = {}): PanelApi & {
  copied: string[];
} {
  let current = status;
  const hotpCodes = new Map<string, string>();
  const copied: string[] = [];

  const entries = (query: string): PanelEntry[] => {
    const now = clock();
    const q = query.trim().toLowerCase();
    const source = empty ? [] : DEMO_ENTRIES;
    const matched = source.filter((e) => q === "" || `${e.title} ${e.issuer} ${e.account}`.toLowerCase().includes(q));
    const ordered = q === "" ? [...matched.filter((e) => e.pinned), ...matched.filter((e) => !e.pinned)] : matched;
    return ordered.map((e, index) => {
      const shortcut = index < 10 ? { shortcut: `control+alt+super+Digit${(index + 1) % 10}` } : {};
      if (e.kind === "hotp") {
        const code = hotpCodes.get(e.id);
        return { ...e, ...shortcut, ...(code === undefined ? {} : { code }) };
      }
      const periodMs = (e.periodS ?? 30) * 1000;
      const step = Math.floor(now / periodMs);
      const validUntilMs = (step + 1) * periodMs;
      const ending = validUntilMs - now <= NEXT_WINDOW_MS;
      return {
        ...e,
        ...shortcut,
        code: fakeCode(e.id, step),
        validUntilMs,
        ...(ending ? { nextCode: fakeCode(e.id, step + 1) } : {}),
      };
    });
  };

  const state = (query: string): PanelState => ({
    status: current,
    entries: current === "ready" ? entries(query) : [],
    nowMs: clock(),
    usesMasterPassword: true,
    inGroup: false,
    readOnly: false,
    ...(current === "unavailable" ? { problem: "DATA_MISSING" as const } : {}),
  });

  return {
    copied,
    state: async (query) => state(query),
    copy: async (id): Promise<CopyResult> => {
      copied.push(id);
      return { clearsInS: CLEARS_IN_S };
    },
    nextHotp: async (id) => {
      hotpCodes.set(id, fakeCode(id, (hotpCodes.size + 1) * 7));
      const entry = entries("").find((e) => e.id === id);
      if (entry === undefined) throw new Error(`no entry ${id}`);
      return entry;
    },
    unlock: async (password) => {
      if (password === "demo") current = "ready";
      return state("");
    },
    openMain: async () => {},
    openGate: async () => {},
    onChange: () => () => {},
    hide: async () => {},
  };
}
