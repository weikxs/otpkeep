import { useState } from "preact/hooks";
import { groupCode, monogram, tileColor } from "../panel/format";
import { Ring } from "../shared/Ring";
import { formatShortcut } from "../shared/shortcut";
import type { Entry, History, MainApi, MoveTo } from "./api";
import { type EntryChange, EntryEditor } from "./EntryEditor";
import type { MainStrings } from "./i18n";

/** What a dragged row carries, so a drop on the window is not mistaken for a file. */
export const ENTRY_DRAG_TYPE = "application/x-otpkeep-entry";

/** Codes ending within this many seconds also show the next one (PRD FR-1.4). */
const NEXT_CODE_WINDOW_S = 5;

export interface EntryRowProps {
  entry: Entry;
  coreNow: number;
  copied: boolean;
  text: MainStrings;
  isMac: boolean;
  previous: string | undefined;
  next: string | undefined;
  /** Rows can be dragged into another place (not while searching or filtering). */
  sortable: boolean;
  onCopy: (entry: Entry) => void;
  onSave: (change: EntryChange) => Promise<void>;
  keyApi: Pick<MainApi, "keyInfo" | "previewKey" | "replaceKey" | "setShortcut">;
  onChanged: (message: string) => void;
  onMove: (to: MoveTo) => Promise<void>;
  onTrash: () => Promise<void>;
  loadHistory: () => Promise<History>;
  /** Another row was dropped on this one. */
  onDropRow: (moved: string, place: "before" | "after") => void;
  /** In selection mode: whether the row is chosen; a click chooses instead of copying. */
  selection?: { chosen: boolean; onToggle: () => void } | undefined;
}

/** One account in the main window: click to copy; names and the note are edited in place. */
export function EntryRow(props: EntryRowProps) {
  const { entry, coreNow, copied, text, isMac, sortable, onCopy, onDropRow, selection } = props;
  const [editing, setEditing] = useState(false);
  const [dropAt, setDropAt] = useState<"before" | "after" | null>(null);
  const subtitle = [entry.issuer, entry.account].filter((s) => s !== "" && s !== entry.title).join(" · ");
  const remainingS =
    entry.validUntilMs === undefined ? undefined : Math.max(0, Math.ceil((entry.validUntilMs - coreNow) / 1000));
  const ending = remainingS !== undefined && remainingS <= NEXT_CODE_WINDOW_S;

  if (editing) {
    return (
      <li class="entry-row editing" data-testid="entry">
        <EntryEditor
          entry={entry}
          text={text}
          isMac={isMac}
          keyApi={props.keyApi}
          onChanged={props.onChanged}
          previous={props.previous}
          next={props.next}
          onSave={props.onSave}
          onMove={props.onMove}
          onTrash={props.onTrash}
          loadHistory={props.loadHistory}
          onDone={() => setEditing(false)}
        />
      </li>
    );
  }

  return (
    <li
      class={`entry-row${dropAt ? ` drop-${dropAt}` : ""}`}
      data-testid="entry"
      draggable={sortable}
      onDragStart={(e) => e.dataTransfer?.setData(ENTRY_DRAG_TYPE, entry.id)}
      onDragOver={(e) => {
        if (!sortable || !e.dataTransfer?.types.includes(ENTRY_DRAG_TYPE)) return;
        e.preventDefault();
        e.stopPropagation();
        const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
        setDropAt(e.clientY < box.top + box.height / 2 ? "before" : "after");
      }}
      onDragLeave={() => setDropAt(null)}
      onDrop={(e) => {
        const moved = e.dataTransfer?.getData(ENTRY_DRAG_TYPE);
        if (!moved) return;
        e.preventDefault();
        e.stopPropagation();
        const place = dropAt ?? "before";
        setDropAt(null);
        if (moved !== entry.id) onDropRow(moved, place);
      }}
    >
      {selection && (
        <input
          type="checkbox"
          class="row-check"
          checked={selection.chosen}
          aria-label={entry.title}
          onChange={selection.onToggle}
        />
      )}
      {sortable && !selection && (
        <span class="drag-handle" title={text.dragHint} aria-hidden="true">
          ⋮⋮
        </span>
      )}
      <button
        type="button"
        class="entry-main"
        onClick={() => (selection ? selection.onToggle() : onCopy(entry))}
        aria-label={`${entry.title} ${text.copied}`}
      >
        <span class={`tile tile-${tileColor(entry.issuer || entry.title)}`} aria-hidden="true">
          {monogram(entry.issuer, entry.title)}
        </span>
        <span class="entry-names">
          <span class="entry-title">
            {entry.pinned && (
              <span class="pin-mark" title={text.pinned} aria-label={text.pinned}>
                ●
              </span>
            )}
            {entry.title}
          </span>
          {subtitle !== "" && <span class="entry-subtitle">{subtitle}</span>}
          {entry.tags.length > 0 && (
            <span class="entry-tags">
              {entry.tags.map((tag) => (
                <span key={tag} class="tag small">
                  {tag}
                </span>
              ))}
            </span>
          )}
          {entry.note !== "" && <span class="entry-note">{entry.note}</span>}
        </span>
        <span class="entry-code">
          {copied ? (
            <span class="copied">{text.copied} ✓</span>
          ) : entry.code === undefined && entry.kind === "hotp" ? (
            <span class="generate">{text.generate}</span>
          ) : entry.code === undefined ? (
            <span class="no-code">{text.noCode}</span>
          ) : (
            <>
              <span class={`code${ending ? " ending" : ""}`} data-testid="code">
                {groupCode(entry.code)}
              </span>
              {ending && entry.nextCode && (
                <span class="next-code">
                  {text.next} {groupCode(entry.nextCode)}
                </span>
              )}
            </>
          )}
        </span>
        {remainingS !== undefined && entry.periodS !== undefined ? (
          <Ring remaining={remainingS} period={entry.periodS} ending={ending} label={`${remainingS}`} />
        ) : (
          <span class="ring-space" />
        )}
      </button>
      <span class={`entry-actions${selection ? " hidden" : ""}`}>
        {entry.shortcut !== undefined && <kbd class="keys">{formatShortcut(entry.shortcut, isMac)}</kbd>}
        <button type="button" class="link-button" onClick={() => setEditing(true)}>
          {text.edit}
        </button>
      </span>
    </li>
  );
}
