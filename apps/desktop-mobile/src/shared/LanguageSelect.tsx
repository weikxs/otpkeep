/** The interface language: one setting for every window and the management page. */
export type LanguageSetting = "system" | "zh" | "en";

export interface LanguageControl {
  value: LanguageSetting;
  onChange: (language: LanguageSetting) => void;
}

export interface LanguageSelectProps {
  control: LanguageControl;
  /** "Follow the system" in the language shown now. */
  systemLabel: string;
  /** The field's name for screen readers ("Language"). */
  label: string;
}

/** Each language is named in itself, so it can be found whatever language is shown now. */
const NATIVE_NAMES = { zh: "简体中文", en: "English" } as const;

/** A small language menu: follow the system, 简体中文 or English. */
export function LanguageSelect({ control, systemLabel, label }: LanguageSelectProps) {
  return (
    <select
      class="language-select"
      aria-label={label}
      title={label}
      value={control.value}
      onChange={(e) => control.onChange((e.target as HTMLSelectElement).value as LanguageSetting)}
    >
      <option value="system">{systemLabel}</option>
      <option value="zh">{NATIVE_NAMES.zh}</option>
      <option value="en">{NATIVE_NAMES.en}</option>
    </select>
  );
}

/** Whether a stored value is one of the settings. */
export function isLanguageSetting(value: string): value is LanguageSetting {
  return value === "system" || value === "zh" || value === "en";
}
