import { useEffect, useState } from "preact/hooks";
import type { GoogleExport, QrImage } from "./api";
import type { MainStrings } from "./i18n";

/** A single QR code closes itself after this long (design/10-reference.md: 60 seconds). */
export const QR_SHOWN_S = 60;

/** An SVG as an image address: drawn as a picture, never inserted into the page. */
export function svgUrl(svg: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

function Overlay({ label, onClose, children }: { label: string; onClose: () => void; children: preact.ComponentChildren }) {
  return (
    <div class="overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <section class="dialog qr-dialog" role="dialog" aria-modal="true" aria-label={label} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        {children}
      </section>
    </div>
  );
}

export interface SingleQrProps {
  qr: QrImage;
  title: string;
  text: MainStrings;
  onClose: () => void;
  clock?: () => number;
}

/** One account's QR code, closed after {@link QR_SHOWN_S} seconds. */
export function SingleQr({ qr, title, text, onClose, clock = Date.now }: SingleQrProps) {
  const [opened] = useState(clock);
  const [left, setLeft] = useState(QR_SHOWN_S);

  useEffect(() => {
    const timer = setInterval(() => {
      const remaining = QR_SHOWN_S - Math.floor((clock() - opened) / 1000);
      if (remaining <= 0) onClose();
      else setLeft(remaining);
    }, 250);
    return () => clearInterval(timer);
  }, [clock, opened, onClose]);

  return (
    <Overlay label={text.qrTitle} onClose={onClose}>
      <header class="dialog-header">
        <h2>{title}</h2>
        <button type="button" class="icon-button" aria-label={text.close} onClick={onClose}>
          ×
        </button>
      </header>
      <img class="qr" src={svgUrl(qr.svg)} alt={text.qrTitle} />
      <p class="muted">{text.qrHint}</p>
      <p class="muted" role="timer">
        {text.qrCloses(left)}
      </p>
    </Overlay>
  );
}

export interface GoogleCodesProps {
  codes: GoogleExport;
  text: MainStrings;
  onClose: () => void;
}

/** Transfer codes one after another, then the accounts to scan on their own. */
export function GoogleCodes({ codes, text, onClose }: GoogleCodesProps) {
  const [index, setIndex] = useState(0);
  const total = codes.codes.length;
  const code = codes.codes[index];

  return (
    <Overlay label={text.googleTitle} onClose={onClose}>
      <header class="dialog-header">
        <h2>{text.googleTitle}</h2>
        <button type="button" class="icon-button" aria-label={text.close} onClick={onClose}>
          ×
        </button>
      </header>
      <p class="muted">{text.googleHint}</p>
      {code && <img class="qr" src={svgUrl(code.svg)} alt={text.googleStep(index + 1, total)} />}
      {total > 0 && (
        <div class="qr-steps">
          <button type="button" class="secondary small" disabled={index === 0} onClick={() => setIndex(index - 1)}>
            {text.previous}
          </button>
          <span>{text.googleStep(index + 1, total)}</span>
          <button type="button" class="secondary small" disabled={index + 1 >= total} onClick={() => setIndex(index + 1)}>
            {text.nextCode}
          </button>
        </div>
      )}
      {codes.separate.length > 0 && (
        <details class="separate">
          <summary>{text.separateTitle}</summary>
          <ul>
            {codes.separate.map((s) => (
              <li key={s.title}>
                <span>
                  {s.title} · {text.leftReasons[s.reason] ?? s.reason}
                </span>
                <img class="qr small-qr" src={svgUrl(s.svg)} alt={s.title} />
              </li>
            ))}
          </ul>
        </details>
      )}
      {codes.checkAfter.length > 0 && <p class="preview-note">{text.checkAfter(codes.checkAfter.join("、"))}</p>}
    </Overlay>
  );
}
