import { useCallback, useEffect, useState } from "preact/hooks";
import { applyTheme, languageOf } from "../shared/appearance";
import type { MainApi, Settings } from "./api";
import { mainStrings } from "./i18n";
import { MainApp } from "./MainApp";

export interface RootProps {
  api: MainApi;
  systemLanguage: string;
  isMac: boolean;
  clock?: () => number;
}

/** The main window with the person's theme and language, following changes from any window. */
export function Root({ api, systemLanguage, isMac, clock }: RootProps) {
  const [language, setLanguage] = useState(systemLanguage);

  const follow = useCallback(
    (settings: Settings) => {
      applyTheme(settings.theme);
      setLanguage(languageOf(settings.language, systemLanguage));
    },
    [systemLanguage],
  );

  useEffect(() => void api.settings().then(follow), [api, follow]);
  useEffect(() => api.onSettings(follow), [api, follow]);

  return (
    <MainApp
      api={api}
      text={mainStrings(language)}
      isMac={isMac}
      onSettingsSaved={follow}
      {...(clock === undefined ? {} : { clock })}
    />
  );
}
