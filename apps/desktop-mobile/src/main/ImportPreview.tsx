import { useState } from "preact/hooks";
import { groupCode, monogram, tileColor } from "../panel/format";
import type { ImportRow, ImportView } from "./api";
import { type MainStrings, describeSkipped } from "./i18n";

export interface ImportPreviewProps {
  view: ImportView;
  text: MainStrings;
  busy: boolean;
  onCommit: (fresh: boolean[], duplicates: boolean[]) => void;
  onBack: () => void;
}

/** What an import would add: new accounts are chosen, accounts already present are not (PRD FR-6.6). */
export function ImportPreview({ view, text, busy, onCommit, onBack }: ImportPreviewProps) {
  const [chosen, setChosen] = useState<Record<string, boolean>>(() =>
    Object.fromEntries(view.rows.map((row) => [key(row), row.group === "fresh"])),
  );
  const flags = (group: ImportRow["group"]) =>
    view.rows.filter((r) => r.group === group).map((r) => chosen[key(r)] ?? false);
  const count = Object.values(chosen).filter(Boolean).length;

  return (
    <div class="preview">
      <p class="preview-title">{text.previewTitle(count)}</p>
      {view.rows.some((r) => r.sampleCode !== undefined) && <p class="muted">{text.compareHint}</p>}
      {view.rows.length > 0 && (
        <ul class="preview-rows">
          {view.rows.map((row) => (
            <li key={key(row)} class={`preview-row ${row.group}`} data-testid="preview-row">
              <label>
                <input
                  type="checkbox"
                  checked={chosen[key(row)] ?? false}
                  onChange={(e) => setChosen({ ...chosen, [key(row)]: (e.target as HTMLInputElement).checked })}
                />
                <span class={`tile tile-${tileColor(row.issuer || row.title)}`} aria-hidden="true">
                  {monogram(row.issuer, row.title)}
                </span>
                <span class="entry-names">
                  <span class="entry-title">{row.title}</span>
                  <span class="entry-subtitle">{details(row, text)}</span>
                  {notes(row, text).map((note) => (
                    <span key={note} class="preview-note">
                      {note}
                    </span>
                  ))}
                </span>
                {row.sampleCode && <span class="sample-code">{groupCode(row.sampleCode)}</span>}
              </label>
            </li>
          ))}
        </ul>
      )}
      {view.skipped.length > 0 && (
        <details class="skipped" open={view.rows.length === 0}>
          <summary>{text.skippedTitle(view.skipped.length)}</summary>
          <ul>
            {view.skipped.map((part, i) => (
              <li key={i}>{describeSkipped(text, part)}</li>
            ))}
          </ul>
        </details>
      )}
      <div class="form-actions">
        <button type="button" class="primary" disabled={busy || count === 0} onClick={() => onCommit(flags("fresh"), flags("duplicate"))}>
          {text.addChosen(count)}
        </button>
        <button type="button" class="secondary" onClick={onBack}>
          {text.back}
        </button>
      </div>
    </div>
  );
}

function key(row: ImportRow): string {
  return `${row.group}:${row.index}`;
}

function details(row: ImportRow, text: MainStrings): string {
  const names = [row.issuer, row.account].filter((s) => s !== "" && s !== row.title);
  const kind = text.kinds[row.kind];
  return [...names, kind].join(" · ");
}

function notes(row: ImportRow, text: MainStrings): string[] {
  const out: string[] = [];
  if (row.existingTitle !== undefined) out.push(`${text.already(row.existingTitle)} · ${text.addAnyway}?`);
  else if (row.sameAsRow !== undefined) out.push(text.alreadyInImport);
  if (row.titleTaken) out.push(text.titleTaken);
  if (row.lost.length > 0) out.push(text.shortened);
  return out;
}
