/** The theme and language the person chose in the settings, applied to a page. */

export type ThemeSetting = "system" | "light" | "dark";

/** Sets the page's colours; "system" follows the system's light or dark appearance. */
export function applyTheme(theme: ThemeSetting, root: HTMLElement = document.documentElement): void {
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}

/** The language to show: the chosen one, or the system's. */
export function languageOf(setting: string, systemLanguage: string): string {
  return setting === "system" ? systemLanguage : setting;
}
