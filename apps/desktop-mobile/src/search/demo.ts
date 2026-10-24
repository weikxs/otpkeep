import { type DemoOptions, demoApi } from "../panel/demo";
import type { SearchApi, Typing } from "./api";

/** A search panel with demo data; typing falls back to copying until "permission" is given. */
export function demoSearchApi(options: DemoOptions = {}): SearchApi & { copied: string[] } {
  const base = demoApi(options);
  let typing: Typing = "needsPermission";
  return {
    ...base,
    typeCode: async (id) => {
      if (typing === "ready") return { typed: true, clearsInS: 0 };
      const copied = await base.copy(id);
      return { typed: false, fallback: typing === "unsupported" ? "UNSUPPORTED" : "NO_PERMISSION", ...copied };
    },
    askTypingPermission: async () => {
      typing = "ready";
      return typing;
    },
    onShown: () => () => undefined,
  };
}
