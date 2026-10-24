import { useState } from "preact/hooks";
import { monogram, tileColor } from "../../panel/format";
import { CommandFailure } from "../api";
import type { SyncStrings } from "./i18n";
import { codeText } from "./i18n";
import type { GroupView, SyncApi } from "./types";

export interface GroupHomeProps {
  group: GroupView;
  api: Pick<SyncApi, "openSyncWindow" | "openPairWindow" | "renameDevice" | "removeDevice" | "leaveGroup">;
  text: SyncStrings;
  now: number;
  onJoin: (looking: "sync" | "pair") => void;
  onChanged: (message: string) => void;
}

/** The sync page without a session: this device, the actions and the devices (design/06-sync.md §7.1, §8). */
export function GroupHome({ group, api, text, now, onJoin, onChanged }: GroupHomeProps) {
  const [name, setName] = useState("");
  const [renaming, setRenaming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (action: () => Promise<void>, message?: string) => {
    setError(null);
    try {
      await action();
      if (message) onChanged(message);
    } catch (thrown) {
      const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
      // Saying no to the native question is not an error.
      if (failure.code !== "CANCELLED") setError(codeText(failure.code === "INVALID" && failure.detail ? failure.detail : failure.code));
    }
  };

  if (!group.inGroup) {
    return (
      <section class="sync-home">
        <p class="preview-title">{text.startTitle}</p>
        <label>
          <span class="field-label">{text.nameLabel}</span>
          <input value={name} maxLength={32} placeholder={text.namePlaceholder} onInput={(e) => setName((e.target as HTMLInputElement).value)} />
        </label>
        <div class="sync-actions">
          <button type="button" class="choice" onClick={() => void run(() => api.openPairWindow(name.trim() === "" ? null : name.trim()))}>
            <span class="choice-title">{text.startFirst}</span>
            <span class="muted">{text.startFirstHint}</span>
          </button>
          <button type="button" class="choice" onClick={() => onJoin("pair")}>
            <span class="choice-title">{text.startJoin}</span>
            <span class="muted">{text.startJoinHint}</span>
          </button>
        </div>
        {error && (
          <p class="field-error" role="alert">
            {error}
          </p>
        )}
      </section>
    );
  }

  return (
    <section class="sync-home">
      {group.me && (
        <div class="me">
          <span class="field-label">{text.me}</span>
          {renaming ? (
            <form
              class="batch-inline"
              onSubmit={(e) => {
                e.preventDefault();
                void run(() => api.renameDevice(name.trim()), text.renamed).then(() => setRenaming(false));
              }}
            >
              <input value={name} maxLength={32} autoFocus aria-label={text.nameLabel} onInput={(e) => setName((e.target as HTMLInputElement).value)} />
              <button type="submit" class="primary small">
                {text.rename}
              </button>
            </form>
          ) : (
            <span class="me-name">
              <span class="entry-title">{group.me.name}</span>
              <span class="muted">{group.me.fingerprint}</span>
              <button
                type="button"
                class="link-button"
                onClick={() => {
                  setName(group.me?.name ?? "");
                  setRenaming(true);
                }}
              >
                {text.rename}
              </button>
            </span>
          )}
        </div>
      )}
      <div class="sync-actions three">
        <button type="button" class="choice" onClick={() => void run(() => api.openSyncWindow())}>
          <span class="choice-title">{text.waitSync}</span>
          <span class="muted">{text.waitSyncHint}</span>
        </button>
        <button type="button" class="choice" onClick={() => onJoin("sync")}>
          <span class="choice-title">{text.syncNow}</span>
          <span class="muted">{text.syncNowHint}</span>
        </button>
        <button type="button" class="choice" disabled={group.full} onClick={() => void run(() => api.openPairWindow(null))}>
          <span class="choice-title">{text.addDevice}</span>
          <span class="muted">{group.full ? codeText("GROUP_FULL") : text.addDeviceHint}</span>
        </button>
      </div>
      <p class="field-label">{text.devices}</p>
      <ul class="devices">
        {group.devices.map((d) => (
          <li key={d.fingerprint} class="device-row" data-testid="device">
            <span class={`tile tile-${tileColor(d.name)}`} aria-hidden="true">
              {monogram(d.name, d.name)}
            </span>
            <span class="entry-names">
              <span class="entry-title">{d.name}</span>
              <span class="entry-subtitle">
                {d.platform} · {d.shortFingerprint} · {d.thisDevice ? text.thisDevice : text.lastSync(d.lastSyncMs, now)}
              </span>
              {d.staleness === "mayBeBehind" && <span class="preview-note">{text.mayBeBehind}</span>}
              {d.staleness === "considerRemoving" && <span class="preview-note">{text.considerRemoving}</span>}
              {d.addedByRemoved && <span class="preview-note">{text.addedByRemoved}</span>}
            </span>
            {!d.thisDevice && (
              <button type="button" class="link-button danger-link" onClick={() => void run(() => api.removeDevice(d.fingerprint), text.removed)}>
                {text.remove}
              </button>
            )}
          </li>
        ))}
      </ul>
      {error && (
        <p class="field-error" role="alert">
          {error}
        </p>
      )}
      <button type="button" class="link-button danger-link align-start" onClick={() => void run(() => api.leaveGroup(), text.left)}>
        {text.leave}
      </button>
    </section>
  );
}
