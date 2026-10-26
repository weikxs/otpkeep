import { memo } from "preact/compat";
import { useEffect, useRef, useState } from "preact/hooks";
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
  /** Changes with each load of the codes (see {@link Ring}). */
  epoch: number;
  copied: boolean;
  text: MainStrings;
  isMac: boolean;
  previous: string | undefined;
  next: string | undefined;
  /** Rows can be dragged into another place (not while searching or filtering). */
  sortable: boolean;
  onCopy: (entry: Entry) => void;
  onSave: (change: EntryChange) => Promise<void>;
  keyApi: Pick<MainApi, "keyInfo" | "previewKey" | "replaceKey" | "setShortcut" | "entryQr">;
  onChanged: (message: string) => void;
  onMove: (to: MoveTo) => Promise<void>;
  onTrash: () => Promise<void>;
  loadHistory: () => Promise<History>;
  /** Another row was dropped on this one. */
  onDropRow: (moved: string, place: "before" | "after") => void;
  /** In selection mode: whether the row is chosen; a click chooses instead of copying. */
  selection?: { chosen: boolean; onToggle: () => void } | undefined;
  /** The row's place in the whole list, for screen readers when only part of it is drawn. */
  posInSet?: number;
  setSize?: number;
  /** Told when the editor opens and closes. */
  onEditing?: (id: string, open: boolean) => void;
  /** Opens the editor now (asked for from the search panel). */
  startEditing?: boolean;
  onEditStarted?: () => void;
}

/** Whole seconds left of the code shown, as the row's countdown shows them. */
function remainingSeconds(entry: Entry, coreNow: number): number | undefined {
  return entry.validUntilMs === undefined ? undefined : Math.max(0, Math.ceil((entry.validUntilMs - coreNow) / 1000));
}

/**
 * Whether a row would look the same: its data, place and countdown second. The window ticks once a second, so
 * only rows with a countdown are drawn again on a tick. Handlers are not compared: the window passes ones that always act on
 * its current state.
 */
export function sameRow(before: EntryRowProps, after: EntryRowProps): boolean {
  const keys = new Set([...Object.keys(before), ...Object.keys(after)] as (keyof EntryRowProps)[]);
  for (const key of keys) {
    const [a, b] = [before[key], after[key]];
    if (typeof a === "function" || typeof b === "function" || key === "coreNow" || key === "selection") continue;
    if (a !== b) return false;
  }
  if (before.selection?.chosen !== after.selection?.chosen) return false;
  return remainingSeconds(before.entry, before.coreNow) === remainingSeconds(after.entry, after.coreNow);
}

/** One account in the main window, drawn again only when what it shows changes (see {@link sameRow}). */
export const EntryRow = memo(EntryRowView, sameRow);

/** One account in the main window: click to copy; names and the note are edited in place. */
function EntryRowView(props: EntryRowProps) {
  const { entry, coreNow, epoch, copied, text, isMac, sortable, onCopy, onDropRow, selection } = props;
  const [editing, setEditing] = useState(false);
  const [dropAt, setDropAt] = useState<"before" | "after" | null>(null);
  const row = useRef<HTMLLIElement>(null);
  const { startEditing, onEditStarted } = props;
  useEffect(() => {
    if (!startEditing) return;
    setEditing(true);
    onEditStarted?.();
  }, [startEditing, onEditStarted]);
  useEffect(() => {
    if (editing) row.current?.scrollIntoView?.({ block: "nearest" });
  }, [editing]);
  const { onEditing } = props;
  const id = entry.id;
  useEffect(() => {
    if (!editing) return;
    onEditing?.(id, true);
    return () => onEditing?.(id, false);
  }, [editing, onEditing, id]);
  // One line under the name: service, account and the note, cut short with "…" (rows keep one height).
  const details = [entry.issuer, entry.account, entry.note].filter((s) => s !== "" && s !== entry.title).join(" · ");
  const remainingS = remainingSeconds(entry, coreNow);
  const ending = remainingS !== undefined && remainingS <= NEXT_CODE_WINDOW_S;

  if (editing) {
    return (
      <li class="entry-row editing" data-testid="entry" ref={row} aria-posinset={props.posInSet} aria-setsize={props.setSize}>
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
      aria-posinset={props.posInSet}
      aria-setsize={props.setSize}
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
            <span class="entry-title-text">{entry.title}</span>
            {entry.tags.map((tag) => (
              <span key={tag} class="tag small">
                {tag}
              </span>
            ))}
          </span>
          {details !== "" && (
            <span class="entry-subtitle" title={details}>
              {details}
            </span>
          )}
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
          <Ring
            remaining={remainingS}
            period={entry.periodS}
            ending={ending}
            label={`${remainingS}`}
            validUntilMs={entry.validUntilMs ?? coreNow}
            coreNow={coreNow}
            epoch={epoch}
          />
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
