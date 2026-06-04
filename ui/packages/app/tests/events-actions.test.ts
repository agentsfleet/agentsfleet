import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── Shared mocks ───────────────────────────────────────────────────────────
// The events actions are thin forwarders: each wraps withToken((t) => apiFn(...)).
// We mock the token wrapper and the events API client so the only thing under
// test is the argument threading (token position) and the wrapped result shape.

// vi.mock is hoisted above the static actions import, so the mock fns must be
// created via vi.hoisted() to exist when the factories run (see runners.test.ts).
const { withTokenMock, listZombieEventsMock, listWorkspaceEventsMock } = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  listZombieEventsMock: vi.fn(),
  listWorkspaceEventsMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/events", () => ({
  listZombieEvents: listZombieEventsMock,
  listWorkspaceEvents: listWorkspaceEventsMock,
}));

import { listZombieEventsAction, listWorkspaceEventsAction } from "@/app/(dashboard)/events/actions";

const PAGE = { items: [], next_cursor: null };

beforeEach(() => {
  vi.clearAllMocks();
  // withToken just forwards a resolved token to its callback for the happy path.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => ({
    ok: true,
    data: await fn("tok"),
  }));
});
afterEach(() => vi.resetAllMocks());

describe("listZombieEventsAction — thin forwarder", () => {
  it("threads the token between zombieId and opts when opts is provided", async () => {
    listZombieEventsMock.mockResolvedValueOnce(PAGE);
    const opts = { cursor: "c1", actor: "alice", since: "2026-06-01", limit: 50 };
    const r = await listZombieEventsAction("ws1", "z1", opts);
    expect(listZombieEventsMock).toHaveBeenCalledWith("ws1", "z1", "tok", opts);
    expect(r).toEqual({ ok: true, data: PAGE });
  });

  it("forwards undefined opts (omitted) with the token still in position", async () => {
    listZombieEventsMock.mockResolvedValueOnce(PAGE);
    const r = await listZombieEventsAction("ws2", "z2");
    expect(listZombieEventsMock).toHaveBeenCalledWith("ws2", "z2", "tok", undefined);
    expect(r).toEqual({ ok: true, data: PAGE });
  });
});

describe("listWorkspaceEventsAction — thin forwarder", () => {
  it("threads the token between workspaceId and opts when opts is provided", async () => {
    listWorkspaceEventsMock.mockResolvedValueOnce(PAGE);
    const opts = { cursor: "c2", actor: "bob", limit: 25 };
    const r = await listWorkspaceEventsAction("ws3", opts);
    expect(listWorkspaceEventsMock).toHaveBeenCalledWith("ws3", "tok", opts);
    expect(r).toEqual({ ok: true, data: PAGE });
  });

  it("forwards undefined opts (omitted) with the token still in position", async () => {
    listWorkspaceEventsMock.mockResolvedValueOnce(PAGE);
    const r = await listWorkspaceEventsAction("ws4");
    expect(listWorkspaceEventsMock).toHaveBeenCalledWith("ws4", "tok", undefined);
    expect(r).toEqual({ ok: true, data: PAGE });
  });
});
