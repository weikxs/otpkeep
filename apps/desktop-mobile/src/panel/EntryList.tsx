/**
 * The account list, the master password prompt and their helpers, shared by the search panel and the password
 * window (design/12). The menu bar menu itself is drawn by the system.
 */
import { memo } from "preact/compat";
import { useEffect, useMemo, useState } from "preact/hooks";
import type { PanelApi, PanelEntry, PanelState } from "./api";
import { Ring } from "../shared/Ring";
import { formatShortcut } from "../shared/shortcut";
import { groupCode, monogram, tileColor } from "./format";
import type { Strings } from "./i18n";
import { useListWindow } from "../shared/useListWindow";
import type { ListItem } from "../shared/windowing";

/** Codes ending within this many seconds also show the next one (FR-1.4). */
const NEXT_CODE_WINDOW_S = 5;

/** An HOTP entry shows no code until one is generated; copying generates it first. */
export async function generateIfNeeded(
  api: Pick<PanelApi, "nextHotp">,
  entry: PanelEntry,
  setState: (update: (current: PanelState | null) => PanelState | null) => void,
) {
  if (entry.kind !== "hotp" || entry.code !== undefined) return;
  const generated = await api.nextHotp(entry.id);
  setState((current) =>
    current && { ...current, entries: current.entries.map((e) => (e.id === entry.id ? generated : e)) },
  );
}

interface ListProps {
  entries: PanelEntry[];
  query: string;
  selected: number;
  copiedId: string | null;
  coreNow: number;
  /** Changes with each load of the codes (see {@link Ring}). */
  epoch: number;
  text: Strings;
  isMac: boolean;
  /** Kept the same between ticks, so rows that show the same thing are not drawn again. */
  onCopy: (entry: PanelEntry) => void;
  onHover: (index: number) => void;
}

/** A list item: a group heading ("Pinned", "All") or the entry at `index`. */
interface Item {
  heading?: string;
  index: number;
}

/** The entries, with headings before the pinned ones and the rest when not searching. */
function listItems(entries: readonly PanelEntry[], query: string, text: Strings): Item[] {
  const pinnedCount = query === "" ? entries.filter((e) => e.pinned).length : 0;
  return entries.flatMap((_, index) => {
    const heading = pinnedCount === 0 ? null : index === 0 ? text.pinned : index === pinnedCount ? text.all : null;
    return heading === null ? [{ index }] : [{ heading, index }, { index }];
  });
}

/** The panels' list: only the rows on screen are drawn (see useListWindow). */
export function EntryList({ entries, query, selected, copiedId, coreNow, epoch, text, isMac, onCopy, onHover }: ListProps) {
  const items = useMemo(() => listItems(entries, query, text), [entries, query, text]);
  const kinds = useMemo<ListItem[]>(() => items.map((item) => (item.heading === undefined ? "row" : "heading")), [items]);
  const windowed = useListWindow(kinds, true, ".entry");
  const span = windowed.span ?? { first: 0, end: items.length, before: 0, after: 0 };
  const selectedAt = items.findIndex((item) => item.heading === undefined && item.index === selected);
  const { reveal } = windowed;
  const drawn = selectedAt >= span.first && selectedAt < span.end;
  // A row chosen with the keys beyond the drawn part is scrolled to first, so it is drawn.
  useEffect(() => {
    if (selectedAt >= 0 && !drawn) reveal(selectedAt);
  }, [selectedAt, drawn, reveal]);
  return (
    <ul
      class="entries"
      role="listbox"
      aria-label={query === "" ? text.all : text.results}
      ref={(element) => {
        windowed.scroller.current = element;
        windowed.list.current = element;
      }}
      onScroll={windowed.onScroll}
      style={{ "--list-before": `${span.before}px`, "--list-after": `${span.after}px` }}
    >
      {items.slice(span.first, span.end).map((item) => {
        const entry = entries[item.index];
        if (item.heading !== undefined) {
          return (
            <li key={`heading-${item.heading}`} class="group-title">
              {item.heading}
            </li>
          );
        }
        if (!entry) return null;
        return (
          <Row
            key={entry.id}
            entry={entry}
            index={item.index}
            position={item.index + 1}
            size={entries.length}
            selected={item.index === selected}
            copied={copiedId === entry.id}
            coreNow={coreNow}
            epoch={epoch}
            text={text}
            isMac={isMac}
            onCopy={onCopy}
            onHover={onHover}
          />
        );
      })}
    </ul>
  );
}

interface RowProps {
  entry: PanelEntry;
  index: number;
  /** Its place among the accounts, for screen readers when only part of the list is drawn. */
  position: number;
  size: number;
  selected: boolean;
  copied: boolean;
  coreNow: number;
  epoch: number;
  text: Strings;
  isMac: boolean;
  onCopy: (entry: PanelEntry) => void;
  onHover: (index: number) => void;
}

/** Whole seconds left of the code shown, as the countdown shows them. */
export function remainingSeconds(entry: Pick<PanelEntry, "validUntilMs">, coreNow: number): number | undefined {
  return entry.validUntilMs === undefined ? undefined : Math.max(0, Math.ceil((entry.validUntilMs - coreNow) / 1000));
}

/** Whether a row would look the same: every prop but the clock, and the countdown's second. */
export function samePanelRow(before: RowProps, after: RowProps): boolean {
  const keys = Object.keys(before) as (keyof RowProps)[];
  if (keys.some((key) => key !== "coreNow" && before[key] !== after[key])) return false;
  return remainingSeconds(before.entry, before.coreNow) === remainingSeconds(after.entry, after.coreNow);
}

/** One account in the panel, drawn again only when what it shows changes (see {@link samePanelRow}). */
const Row = memo(RowView, samePanelRow);

function RowView({ entry, index, position, size, selected, copied, coreNow, epoch, text, isMac, onCopy, onHover }: RowProps) {
  const subtitle = [entry.issuer, entry.account].filter((s) => s !== "" && s !== entry.title).join(" · ");
  const remainingS = remainingSeconds(entry, coreNow);
  const ending = remainingS !== undefined && remainingS <= NEXT_CODE_WINDOW_S;
  const label = useMemo(() => monogram(entry.issuer, entry.title), [entry.issuer, entry.title]);
  return (
    <li
      class={`entry${selected ? " selected" : ""}`}
      role="option"
      aria-selected={selected}
      aria-posinset={position}
      aria-setsize={size}
      data-testid="entry"
      onClick={() => onCopy(entry)}
      onMouseEnter={() => onHover(index)}
    >
      <span class={`tile tile-${tileColor(entry.issuer || entry.title)}`} aria-hidden="true">
        {label}
      </span>
      <span class="names">
        <span class="title">{entry.title}</span>
        {subtitle !== "" && <span class="subtitle">{subtitle}</span>}
      </span>
      {/* Always shown: the row's shortcut is how it is copied without opening the panel. */}
      <span class="shortcut">{entry.shortcut === undefined ? "" : formatShortcut(entry.shortcut, isMac)}</span>
      <span class="code-area">
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
      {remainingS !== undefined && entry.periodS !== undefined && (
        <Ring
          remaining={remainingS}
          period={entry.periodS}
          ending={ending}
          label={text.seconds(remainingS)}
          validUntilMs={entry.validUntilMs ?? coreNow}
          coreNow={coreNow}
          epoch={epoch}
        />
      )}
    </li>
  );
}

interface PasswordPromptProps {
  api: Pick<PanelApi, "unlock">;
  text: Strings;
  onOpened: (state: PanelState) => void;
}

/** Asked once per run when the data is protected by a master password. */
export function PasswordPrompt({ api, text, onOpened }: PasswordPromptProps) {
  const [password, setPassword] = useState("");
  // An empty field is not a wrong password: it is not tried at all.
  const [problem, setProblem] = useState<"empty" | "wrong" | null>(null);
  const submit = async (event: Event) => {
    event.preventDefault();
    if (password === "") {
      setProblem("empty");
      return;
    }
    const next = await api.unlock(password);
    setPassword("");
    if (next.status === "needsPassword") setProblem("wrong");
    else onOpened(next);
  };
  return (
    <section class="panel-message">
      <KeyIcon />
      <p class="message-title">{text.needsPassword}</p>
      <form class="unlock" onSubmit={(e) => void submit(e)}>
        <input
          type="password"
          value={password}
          placeholder={text.password}
          aria-label={text.password}
          autocomplete="off"
          autoFocus
          onInput={(e) => {
            setPassword((e.target as HTMLInputElement).value);
            setProblem(null);
          }}
        />
        <button type="submit" class="primary">
          {text.unlock}
        </button>
        {problem && (
          <p class="error" role="alert">
            {problem === "empty" ? text.enterPassword : text.wrongPassword}
          </p>
        )}
      </form>
    </section>
  );
}

export function SearchIcon() {
  return (
    <svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="7" cy="7" r="4.5" />
      <path d="M10.5 10.5 14 14" />
    </svg>
  );
}

function KeyIcon() {
  return (
    <svg class="icon icon-large" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="5.5" cy="8" r="2.5" />
      <path d="M8 8h6M12 8v2M14 8v1.5" />
    </svg>
  );
}
