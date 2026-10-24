/** The sync page's data, as the core sends it (design/06-sync.md §8). */

export interface DeviceView {
  name: string;
  fingerprint: string;
  shortFingerprint: string;
  platform: string;
  lastSyncMs?: number;
  staleness: "fresh" | "mayBeBehind" | "considerRemoving";
  addedByRemoved: boolean;
  thisDevice: boolean;
}

export interface GroupView {
  inGroup: boolean;
  me?: DeviceView;
  devices: DeviceView[];
  full: boolean;
  hasEntries: boolean;
}

export type SessionKind = "syncWindow" | "pairWindow" | "syncJoin" | "pairJoin";

export interface PeerView {
  name: string;
  fingerprint: string;
  shortFingerprint: string;
  platform: string;
}

export interface AskView {
  id: number;
  kind: "allow" | "checkCode";
  sas?: string;
  deadlineMs: number;
}

export interface SessionView {
  kind: SessionKind;
  phase: "waiting" | "connecting" | "asking" | "exchanging";
  code?: string;
  hasInvite: boolean;
  closesAtMs?: number;
  skipped: { via: "lan" | "bluetooth"; code: string }[];
  peer?: PeerView;
  ask?: AskView;
}

export interface ResultView {
  kind: SessionKind;
  ok: boolean;
  code: string;
  peer?: PeerView;
  entries?: number;
  atMs: number;
}

export interface SyncView {
  session?: SessionView;
  last?: ResultView;
}

export interface SyncState {
  group: GroupView;
  sync: SyncView;
}

export interface FoundView {
  index: number;
  name: string;
  shortFingerprint: string;
  via: "lan" | "bluetooth";
  remainingS: number;
}

export type PairWith = { by: "found"; index: number; code: string } | { by: "invite"; text: string };

export interface SyncApi {
  syncState(): Promise<SyncState>;
  openSyncWindow(): Promise<void>;
  openPairWindow(name: string | null): Promise<void>;
  /** The pairing invite's QR code (SVG), after a native confirmation. */
  pairInvite(): Promise<string>;
  searchWindows(looking: "sync" | "pair"): Promise<FoundView[]>;
  joinSync(index: number, code: string): Promise<void>;
  joinPair(withWindow: PairWith, name: string | null, replace: boolean): Promise<void>;
  answerAsk(id: number, yes: boolean): Promise<void>;
  closeSession(): Promise<void>;
  renameDevice(name: string): Promise<void>;
  removeDevice(fingerprint: string): Promise<void>;
  leaveGroup(): Promise<void>;
  /** Calls `listener` whenever the session or the last result changes. */
  onSyncView(listener: (view: SyncView) => void): () => void;
}
