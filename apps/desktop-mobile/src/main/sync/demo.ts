import { CommandFailure } from "../api";
import type { DeviceView, FoundView, GroupView, SessionView, SyncApi, SyncView } from "./types";

/** A pretend sync for the preview and tests. Hooks let a test play the other device's part. */
export interface SyncDemo extends SyncApi {
  /** The other device asks to sync (a sync window must be open). */
  request(name: string): void;
  /** The pairing reaches the check code. */
  checkCode(): void;
}

const ME: DeviceView = {
  name: "本机 Mac",
  fingerprint: "AB12-CD34-EF56-7890",
  shortFingerprint: "AB12",
  platform: "macOS",
  staleness: "fresh",
  addedByRemoved: false,
  thisDevice: true,
};

const PHONE: DeviceView = {
  name: "我的手机",
  fingerprint: "9F8E-7D6C-5B4A-3210",
  shortFingerprint: "9F8E",
  platform: "iOS",
  lastSyncMs: 0,
  staleness: "fresh",
  addedByRemoved: false,
  thisDevice: false,
};

const WINDOW_MS = 120_000;

export function demoSync(clock: () => number, inGroup: boolean): SyncDemo {
  let group: GroupView = inGroup
    ? { inGroup: true, me: ME, devices: [ME, { ...PHONE, lastSyncMs: clock() - 3 * 86_400_000 }], full: false, hasEntries: true }
    : { inGroup: false, devices: [], full: false, hasEntries: true };
  let view: SyncView = {};
  const listeners = new Set<(view: SyncView) => void>();
  const publish = () => {
    for (const listener of listeners) listener(view);
  };
  const session = (s: SessionView) => {
    view = { ...view, session: s };
    publish();
  };
  const end = (ok: boolean, code: string) => {
    const kind = view.session?.kind ?? "syncWindow";
    const peer = view.session?.peer;
    view = { last: { kind, ok, code, atMs: clock(), entries: 4, ...(peer ? { peer } : {}) } };
    publish();
  };
  const found: FoundView[] = [{ index: 0, name: PHONE.name, shortFingerprint: PHONE.shortFingerprint, via: "bluetooth", remainingS: 90 }];
  const peer = { name: PHONE.name, fingerprint: PHONE.fingerprint, shortFingerprint: PHONE.shortFingerprint, platform: PHONE.platform };
  let asks = 0;

  return {
    syncState: async () => ({ group, sync: view }),
    openSyncWindow: async () => {
      if (!group.inGroup) throw new CommandFailure("NOT_IN_GROUP");
      session({ kind: "syncWindow", phase: "waiting", code: "482913", hasInvite: false, closesAtMs: clock() + WINDOW_MS, skipped: [] });
    },
    openPairWindow: async (name) => {
      if (!group.inGroup) {
        const me = { ...ME, name: name ?? ME.name };
        group = { ...group, inGroup: true, me, devices: [me] };
      }
      session({
        kind: "pairWindow",
        phase: "waiting",
        code: "73915206",
        hasInvite: true,
        closesAtMs: clock() + WINDOW_MS,
        skipped: [{ via: "bluetooth", code: "BLUETOOTH_OFF" }],
      });
    },
    pairInvite: async () =>
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><rect width="21" height="21" fill="#fff"/><path d="M1 1h7v7H1zM13 1h7v7h-7zM1 13h7v7H1z" fill="#000"/></svg>',
    searchWindows: async () => found,
    joinSync: async (index, code) => {
      if (!found[index]) throw new CommandFailure("NOT_FOUND");
      session({ kind: "syncJoin", phase: "connecting", hasInvite: false, skipped: [], peer });
      if (code.replace(/\D/gu, "") !== "482913") end(false, "CODE_WRONG");
      else end(true, "SYNCED");
    },
    joinPair: async (_with, _name, _replace) => {
      session({ kind: "pairJoin", phase: "connecting", hasInvite: false, skipped: [], peer });
    },
    answerAsk: async (id, yes) => {
      if (view.session?.ask?.id !== id) throw new CommandFailure("ASK_GONE");
      if (!yes) {
        end(false, view.session.ask.kind === "allow" ? "REJECTED" : "SAS_MISMATCH");
        return;
      }
      const paired = view.session.kind === "pairWindow" || view.session.kind === "pairJoin";
      if (paired) group = { ...group, inGroup: true, me: group.me ?? ME, devices: [group.me ?? ME, PHONE] };
      end(true, paired ? "PAIRED" : "SYNCED");
    },
    closeSession: async () => {
      if (view.session) end(false, "CANCELLED");
    },
    renameDevice: async (name) => {
      if (name.trim() === "" || name.length > 32) throw new CommandFailure("INVALID", "INVALID_NAME");
      const me = { ...(group.me ?? ME), name };
      group = { ...group, me, devices: group.devices.map((d) => (d.thisDevice ? me : d)) };
    },
    removeDevice: async (fingerprint) => {
      group = { ...group, devices: group.devices.filter((d) => d.fingerprint !== fingerprint) };
    },
    leaveGroup: async () => {
      group = { inGroup: false, devices: [], full: false, hasEntries: true };
    },
    onSyncView: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    request: (name) => {
      const s = view.session;
      if (!s) return;
      session({ ...s, phase: "asking", peer: { ...peer, name }, ask: { id: ++asks, kind: "allow", deadlineMs: clock() + 30_000 } });
    },
    checkCode: () => {
      const s = view.session;
      if (!s) return;
      session({ ...s, phase: "asking", peer, ask: { id: ++asks, kind: "checkCode", sas: "305118", deadlineMs: clock() + 60_000 } });
    },
  };
}
