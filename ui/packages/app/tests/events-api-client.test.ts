import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Pure API-client tests for lib/api/events. The thread read is paged, so every
// combination of its two optional query parameters is exercised here — the
// dashboard only ever asks for the first page, which would otherwise leave the
// continuation arms unproven.
const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

const okPage = {
  ok: true,
  status: 200,
  json: async () => ({ items: [], next_cursor: null }),
};

beforeEach(() => {
  vi.clearAllMocks();
  fetchMock.mockResolvedValue(okPage);
});
afterEach(() => {
  fetchMock.mockReset();
});

const calledUrl = () => String(fetchMock.mock.calls[0]?.[0] ?? "");

describe("lib/api/events listFleetMessages", () => {
  it("omits the query string entirely when no options are given", async () => {
    const mod = await import("../lib/api/events");
    await mod.listFleetMessages("ws_1", "zom_1", "tkn");
    expect(calledUrl()).toContain("/v1/workspaces/ws_1/fleets/zom_1/messages");
    expect(calledUrl()).not.toContain("?");
  });

  it("sends limit alone when only limit is given", async () => {
    const mod = await import("../lib/api/events");
    await mod.listFleetMessages("ws_1", "zom_1", "tkn", { limit: 20 });
    expect(calledUrl()).toContain("?limit=20");
    expect(calledUrl()).not.toContain("starting_after");
  });

  it("sends starting_after alone when only the cursor is given", async () => {
    const mod = await import("../lib/api/events");
    await mod.listFleetMessages("ws_1", "zom_1", "tkn", { starting_after: "cur_1" });
    expect(calledUrl()).toContain("starting_after=cur_1");
    expect(calledUrl()).not.toContain("limit=");
  });

  it("sends both parameters when paging a thread", async () => {
    const mod = await import("../lib/api/events");
    await mod.listFleetMessages("ws_1", "zom_1", "tkn", {
      starting_after: "cur_1",
      limit: 5,
    });
    expect(calledUrl()).toContain("starting_after=cur_1");
    expect(calledUrl()).toContain("limit=5");
  });

  it("carries the bearer token and reads the thread with GET", async () => {
    const mod = await import("../lib/api/events");
    await mod.listFleetMessages("ws_1", "zom_1", "tkn", { limit: 1 });
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/messages"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tkn" }),
      }),
    );
  });
});
