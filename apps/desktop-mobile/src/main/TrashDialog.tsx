import { useEffect, useState } from "preact/hooks";
import { monogram, tileColor } from "../panel/format";
import { CommandFailure, type MainApi, type TrashItem } from "./api";
import { type MainStrings, describe } from "./i18n";

const DAY_MS = 24 * 60 * 60 * 1000;

export interface TrashDialogProps {
  api: MainApi;
  text: MainStrings;
  now: number;
  onChanged: (message: string) => void;
  onClose: () => void;
}

/** The trash: entries stay 30 days, then go for good; deleting earlier asks first (PRD FR-4.3). */
export function TrashDialog({ api, text, now, onChanged, onClose }: TrashDialogProps) {
  const [items, setItems] = useState<TrashItem[] | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = () => api.trashList().then(setItems);
  useEffect(() => void reload(), []);

  const act = async (action: () => Promise<void>, message: string) => {
    setError(null);
    try {
      await action();
      setConfirming(null);
      onChanged(message);
      await reload();
    } catch (thrown) {
      const failure = thrown instanceof CommandFailure ? thrown : new CommandFailure("UNKNOWN");
      setError(describe(text, failure.code, failure.detail));
    }
  };

  return (
    <div class="overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <section class="dialog" role="dialog" aria-modal="true" aria-label={text.trashTitle} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <header class="dialog-header">
          <h2>{text.trashTitle}</h2>
          <button type="button" class="icon-button" aria-label={text.close} onClick={onClose}>
            ×
          </button>
        </header>
        <p class="muted">{text.trashExplain}</p>
        {items?.length === 0 && <p class="empty-search">{text.trashEmpty}</p>}
        {items && items.length > 0 && (
          <ul class="preview-rows">
            {items.map((item) => (
              <li key={item.id} class="trash-row" data-testid="trash-row">
                <span class={`tile tile-${tileColor(item.issuer || item.title)}`} aria-hidden="true">
                  {monogram(item.issuer, item.title)}
                </span>
                <span class="entry-names">
                  <span class="entry-title">{item.title}</span>
                  <span class="entry-subtitle">{text.purgeIn(Math.max(0, Math.ceil((item.purgeAtMs - now) / DAY_MS)))}</span>
                </span>
                {confirming === item.id ? (
                  <span class="confirm">
                    <span class="muted">{text.confirmPurge}</span>
                    <button type="button" class="danger" onClick={() => void act(() => api.purge(item.id), text.purged)}>
                      {text.purge}
                    </button>
                    <button type="button" class="secondary" onClick={() => setConfirming(null)}>
                      {text.cancel}
                    </button>
                  </span>
                ) : (
                  <span class="confirm">
                    <button type="button" class="secondary" onClick={() => void act(() => api.restore(item.id), text.restored)}>
                      {text.restore}
                    </button>
                    <button type="button" class="link-button danger-link" onClick={() => setConfirming(item.id)}>
                      {text.purge}
                    </button>
                  </span>
                )}
              </li>
            ))}
          </ul>
        )}
        {error && (
          <p class="field-error" role="alert">
            {error}
          </p>
        )}
      </section>
    </div>
  );
}
