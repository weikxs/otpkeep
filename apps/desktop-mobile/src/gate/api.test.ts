import { describe, expect, it, vi } from "vitest";
import { GateFailure, connect, forget } from "./api";

function reply(status: number, body?: unknown): Response {
  return new Response(body === undefined ? null : JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function fakeWindow(hash: string) {
  const storage = new Map<string, string>();
  const replaced: string[] = [];
  const win = {
    location: { hash, pathname: "/" },
    history: { replaceState: (_s: unknown, _t: string, url: string) => replaced.push(url) },
    sessionStorage: {
      getItem: (k: string) => storage.get(k) ?? null,
      setItem: (k: string, v: string) => storage.set(k, v),
      removeItem: (k: string) => storage.delete(k),
    },
  } as unknown as Window;
  return { win, storage, replaced };
}

describe("connect", () => {
  it("trades the link for a session and removes the token from the address", async () => {
    const { win, storage, replaced } = fakeWindow("#k=abc");
    const fetchFn = vi.fn().mockResolvedValueOnce(reply(200, { session: "s1" }));

    await connect(win, fetchFn);

    expect(fetchFn).toHaveBeenCalledWith("/api/session", expect.objectContaining({ method: "POST", body: '{"link":"abc"}' }));
    expect(replaced).toEqual(["/"]);
    expect(storage.get("otpkeep-session")).toBe("s1");
  });

  it("reports a used link", async () => {
    const { win } = fakeWindow("#k=abc");
    const fetchFn = vi.fn().mockResolvedValueOnce(reply(401, { error: { code: "LINK_USED" } }));

    await expect(connect(win, fetchFn)).rejects.toEqual(new GateFailure("LINK_USED"));
  });

  it("reuses this tab's session after a reload, and fails without one", async () => {
    const { win, storage } = fakeWindow("");
    await expect(connect(win, vi.fn())).rejects.toEqual(new GateFailure("NO_SESSION"));

    storage.set("otpkeep-session", "s1");
    await expect(connect(win, vi.fn())).resolves.toBeTruthy();
    forget(win);
    expect(storage.size).toBe(0);
  });

  it("sends the session with every request and reads refusals", async () => {
    const { win, storage } = fakeWindow("");
    storage.set("otpkeep-session", "s1");
    const fetchFn = vi
      .fn()
      .mockResolvedValueOnce(reply(200, { entries: [] }))
      .mockResolvedValueOnce(reply(409, { error: { code: "SHORTCUT_TAKEN", detail: "ENTRY:Mail" } }))
      .mockResolvedValueOnce(reply(204));
    const api = await connect(win, fetchFn);

    await api.entries("a b");
    const refused = api.setShortcut("e/1", "Control+KeyM");
    await expect(refused).rejects.toEqual(new GateFailure("SHORTCUT_TAKEN", "ENTRY:Mail"));
    await api.heartbeat();

    expect(fetchFn.mock.calls[0]?.[0]).toBe("/api/entries?q=a%20b");
    expect(fetchFn.mock.calls[1]?.[0]).toBe("/api/entries/e%2F1/shortcut");
    for (const call of fetchFn.mock.calls) {
      expect((call[1] as RequestInit).headers).toMatchObject({ Authorization: "Bearer s1" });
    }
  });

  it("treats an unreadable refusal as unknown", async () => {
    const { win, storage } = fakeWindow("");
    storage.set("otpkeep-session", "s1");
    const fetchFn = vi.fn().mockResolvedValueOnce(new Response("oops", { status: 500 }));
    const api = await connect(win, fetchFn);

    await expect(api.entries("")).rejects.toEqual(new GateFailure("UNKNOWN"));
  });
});
