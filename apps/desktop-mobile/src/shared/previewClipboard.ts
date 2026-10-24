/**
 * Copying in a browser preview, where there is no OTPKeep behind the page to write the clipboard. The demo
 * code really goes to the clipboard, so "Copied" is never shown for nothing; a refusal is passed on.
 */
export async function previewCopy(code: string): Promise<void> {
  if (typeof navigator === "undefined" || !navigator.clipboard) throw new Error("no clipboard in this browser");
  await navigator.clipboard.writeText(code);
}
