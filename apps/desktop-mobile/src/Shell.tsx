/** Where the shared interface is running. */
export type Surface = "app" | "gate";

/** Props for {@link Shell}. */
export interface ShellProps {
  surface: Surface;
}

/** The common frame of the app window and the browser management page. */
export function Shell({ surface }: ShellProps) {
  return (
    <main>
      <h1>OTPKeep</h1>
      <p data-testid="surface">{surface === "app" ? "App window" : "Browser management page"}</p>
    </main>
  );
}
