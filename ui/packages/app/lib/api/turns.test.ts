import { afterEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

const mockResponse = {
  items: [
    {
      turn_id: "1700000000000-0",
      fleet_id: "z_1",
      workspace_id: "ws_1",
      actor: "steer:kishore",
      event_type: "chat",
      status: "processed",
      request_json: "{\"message\":\"ping\"}",
      response_text: "pong",
      tokens: 12,
      wall_ms: 340,
      failure_label: null,
      resumes_turn_id: null,
      created_at: "2023-11-14T22:13:20Z",
      updated_at: "2023-11-14T22:13:20.34Z",
    },
  ],
  next_cursor: null,
};

describe("listWorkspaceTurns", () => {
  it("hits the workspace-aggregate turns endpoint", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockResponse });
    const { listWorkspaceTurns } = await import("./turns");
    await listWorkspaceTurns("ws_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/turns"),
      expect.anything(),
    );
  });

  it("forwards a fleet_id drill-down filter", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockResponse });
    const { listWorkspaceTurns } = await import("./turns");
    await listWorkspaceTurns("ws_1", "tok", { fleet_id: "z_2" });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("fleet_id=z_2");
  });

  it("includes since param when provided alongside other opts", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockResponse });
    const { listWorkspaceTurns } = await import("./turns");
    await listWorkspaceTurns("ws_1", "tok", { since: "1d", limit: 50 });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("since=1d");
    expect(url).toContain("limit=50");
  });
});

describe("streamFleetTurnsUrl", () => {
  it("returns a same-origin path the Next Route Handler intercepts", async () => {
    const { streamFleetTurnsUrl } = await import("./turns-types");
    expect(streamFleetTurnsUrl("ws_1", "z_1")).toBe(
      "/live/v1/workspaces/ws_1/fleets/z_1/turns/stream",
    );
  });

  it("encodes path segments so a slashy id can not escape the URL", async () => {
    const { streamFleetTurnsUrl } = await import("./turns-types");
    expect(streamFleetTurnsUrl("ws/1", "z 2")).toBe(
      "/live/v1/workspaces/ws%2F1/fleets/z%202/turns/stream",
    );
  });
});

describe("backfillFleetTurnsUrl", () => {
  it("returns a clean same-origin path when no query opts are given", async () => {
    const { backfillFleetTurnsUrl } = await import("./turns-types");
    expect(backfillFleetTurnsUrl("ws_1", "z_1")).toBe(
      "/live/v1/workspaces/ws_1/fleets/z_1/turns",
    );
  });

  it("appends since/limit through the shared query builder", async () => {
    const { backfillFleetTurnsUrl } = await import("./turns-types");
    expect(
      backfillFleetTurnsUrl("ws_1", "z_1", { since: "2026-05-15T18:29:58Z", limit: 200 }),
    ).toBe(
      "/live/v1/workspaces/ws_1/fleets/z_1/turns?since=2026-05-15T18%3A29%3A58Z&limit=200",
    );
  });

  it("appends a keyset cursor through the shared query builder", async () => {
    const { backfillFleetTurnsUrl } = await import("./turns-types");
    expect(backfillFleetTurnsUrl("ws_1", "z_1", { cursor: "abc123" })).toBe(
      "/live/v1/workspaces/ws_1/fleets/z_1/turns?cursor=abc123",
    );
  });

  it("encodes path segments so a slashy id can not escape the URL", async () => {
    const { backfillFleetTurnsUrl } = await import("./turns-types");
    expect(backfillFleetTurnsUrl("ws/1", "z 2")).toBe(
      "/live/v1/workspaces/ws%2F1/fleets/z%202/turns",
    );
  });
});

describe("fleetTurnDetailUrl", () => {
  it("returns the same-origin path the detail Route Handler intercepts", async () => {
    const { fleetTurnDetailUrl } = await import("./turns-types");
    expect(fleetTurnDetailUrl("ws_1", "z_1", "1790573387481-566")).toBe(
      "/live/v1/workspaces/ws_1/fleets/z_1/turns/1790573387481-566",
    );
  });

  it("encodes every path segment so a slashy id can not escape the URL", async () => {
    const { fleetTurnDetailUrl } = await import("./turns-types");
    expect(fleetTurnDetailUrl("ws/1", "z 2", "e/3")).toBe(
      "/live/v1/workspaces/ws%2F1/fleets/z%202/turns/e%2F3",
    );
  });
});

describe("workspace turn query parameters", () => {
  it("forwards actor / since / cursor / limit", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) });
    const { listWorkspaceTurns } = await import("./turns");
    await listWorkspaceTurns("ws_1", "tok", {
      cursor: "abc",
      actor: "webhook:*",
      since: "2h",
      limit: 25,
    });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("cursor=abc");
    // URLSearchParams encodes ":" as "%3A" but keeps "*" literal (sub-delim).
    expect(url).toContain("actor=webhook%3A*");
    expect(url).toContain("since=2h");
    expect(url).toContain("limit=25");
  });

  it("forwards actor_prefix — the steer-detection filter", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) });
    const { listWorkspaceTurns } = await import("./turns");
    await listWorkspaceTurns("ws_1", "tok", { actor_prefix: "steer:", limit: 1 });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("actor_prefix=steer%3A");
  });

  it("omits since from the query string when opts provides other params but not since", async () => {
    // Exercises the false branch of `if (opts.since)` in buildQuery.
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) });
    const { listWorkspaceTurns } = await import("./turns");
    await listWorkspaceTurns("ws_1", "tok", { actor: "cron", limit: 10 });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("actor=cron");
    expect(url).not.toContain("since=");
  });

  it("produces a clean URL (no trailing ?) when opts is an empty object", async () => {
    // Exercises the false branch of `qs.length > 0 ? ... : ""` in buildQuery.
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) });
    const { listWorkspaceTurns } = await import("./turns");
    await listWorkspaceTurns("ws_1", "tok", {});
    const url = fetchMock.mock.calls[0]![0] as string;
    // With an empty opts object, buildQuery produces "" so the URL must not end in "?".
    expect(url).not.toContain("?");
    expect(url).toContain("/v1/workspaces/ws_1/turns");
  });
});
