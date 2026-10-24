import { useState } from "preact/hooks";
import type { ManualForm } from "./api";
import type { MainStrings } from "./i18n";

export interface ManualFormViewProps {
  text: MainStrings;
  busy: boolean;
  error: string | null;
  onPreview: (form: ManualForm) => void;
}

const EMPTY: ManualForm = {
  title: "",
  issuer: "",
  account: "",
  secret: "",
  kind: "totp",
  algorithm: "SHA1",
  digits: 6,
  periodS: 30,
  counter: 0,
};

/** For a secret without a link (PRD FR-2.2): the preview shows a code to compare before adding. */
export function ManualFormView({ text, busy, error, onPreview }: ManualFormViewProps) {
  const [form, setForm] = useState<ManualForm>(EMPTY);
  const set = <K extends keyof ManualForm>(name: K, value: ManualForm[K]) => setForm({ ...form, [name]: value });
  const input = (e: Event) => (e.target as HTMLInputElement).value;

  return (
    <form
      class="manual-form"
      onSubmit={(e) => {
        e.preventDefault();
        onPreview(form);
      }}
    >
      <label>
        <span class="field-label">{text.issuer}</span>
        <input value={form.issuer} placeholder="GitHub" onInput={(e) => set("issuer", input(e))} />
      </label>
      <label>
        <span class="field-label">{text.account}</span>
        <input value={form.account} placeholder="name@example.com" onInput={(e) => set("account", input(e))} />
      </label>
      <label>
        <span class="field-label">{text.secret}</span>
        <input
          value={form.secret}
          class="mono"
          autoComplete="off"
          spellcheck={false}
          placeholder={text.secretPlaceholder}
          onInput={(e) => set("secret", input(e))}
        />
      </label>
      <details class="advanced">
        <summary>{text.advanced}</summary>
        <div class="advanced-grid">
          <label>
            <span class="field-label">{text.kind}</span>
            <select value={form.kind} onChange={(e) => set("kind", input(e) as ManualForm["kind"])}>
              {(["totp", "hotp", "steam"] as const).map((k) => (
                <option key={k} value={k}>
                  {text.kinds[k]}
                </option>
              ))}
            </select>
          </label>
          {form.kind !== "steam" && (
            <>
              <label>
                <span class="field-label">{text.algorithm}</span>
                <select value={form.algorithm} onChange={(e) => set("algorithm", input(e) as ManualForm["algorithm"])}>
                  {(["SHA1", "SHA256", "SHA512"] as const).map((a) => (
                    <option key={a} value={a}>
                      {a}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span class="field-label">{text.digits}</span>
                <select value={String(form.digits)} onChange={(e) => set("digits", Number(input(e)))}>
                  {[6, 7, 8].map((d) => (
                    <option key={d} value={String(d)}>
                      {d}
                    </option>
                  ))}
                </select>
              </label>
            </>
          )}
          {form.kind === "hotp" ? (
            <label>
              <span class="field-label">{text.counter}</span>
              <input type="number" min={0} value={form.counter} onInput={(e) => set("counter", Number(input(e)))} />
            </label>
          ) : (
            <label>
              <span class="field-label">{text.period}</span>
              <input
                type="number"
                min={1}
                max={300}
                value={form.periodS}
                onInput={(e) => set("periodS", Number(input(e)))}
              />
            </label>
          )}
        </div>
      </details>
      {error && (
        <p class="field-error" role="alert">
          {error}
        </p>
      )}
      <div class="form-actions">
        <button type="submit" class="primary" disabled={busy}>
          {text.preview}
        </button>
      </div>
    </form>
  );
}
