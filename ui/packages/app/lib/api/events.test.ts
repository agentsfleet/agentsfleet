import { afterEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

const mockResponse = {
  items: [
    {
      event_id: "1700000000000-0",
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
      checkpoint_id: null,
      resumes_event_id: null,
      created_at: 1_700_000_000_000,
      updated_at: 1_700_000_000_340,
    },
  ],
  next_cursor: null,
};

describe("listFleetEvents", () => {
  it("hits the per-fleet events endpoint without a cursor by default", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockResponse });
    const { listFleetEvents } = await import("./events");
    const page = await listFleetEvents("ws_1", "z_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/z_1/events"),
      expect.anything(),
    );
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).not.toContain("cursor=");
    expect(page.items[0]!.actor).toBe("steer:kishore");
  });

  it("forwards actor / since / cursor / limit", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) });
    const { listFleetEvents } = await import("./events");
    await listFleetEvents("ws_1", "z_1", "tok", {
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

  it("omits since from the query string when opts provides other params but not since", async () => {
    // Exercises the false branch of `if (opts.since)` in buildQuery.
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) });
    const { listFleetEvents } = await import("./events");
    await listFleetEvents("ws_1", "z_1", "tok", { actor: "cron", limit: 10 });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("actor=cron");
    expect(url).not.toContain("since=");
  });

  it("produces a clean URL (no trailing ?) when opts is an empty object", async () => {
    // Exercises the false branch of `qs.length > 0 ? ... : ""` in buildQuery.
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) });
    const { listFleetEvents } = await import("./events");
    await listFleetEvents("ws_1", "z_1", "tok", {});
    const url = fetchMock.mock.calls[0]![0] as string;
    // With an empty opts object, buildQuery produces "" so the URL must not end in "?".
    expect(url).not.toContain("?");
    expect(url).toContain("/v1/workspaces/ws_1/fleets/z_1/events");
  });
});

describe("listWorkspaceEvents", () => {
  it("hits the workspace-aggregate events endpoint", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockResponse });
    const { listWorkspaceEvents } = await import("./events");
    await listWorkspaceEvents("ws_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/events"),
      expect.anything(),
    );
  });

  it("forwards a fleet_id drill-down filter", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockResponse });
    const { listWorkspaceEvents } = await import("./events");
    await listWorkspaceEvents("ws_1", "tok", { fleet_id: "z_2" });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("fleet_id=z_2");
  });

  it("includes since param when provided alongside other opts", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockResponse });
    const { listWorkspaceEvents } = await import("./events");
    await listWorkspaceEvents("ws_1", "tok", { since: "1d", limit: 50 });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("since=1d");
    expect(url).toContain("limit=50");
  });
});

describe("streamFleetEventsUrl", () => {
  it("returns a same-origin path the Next Route Handler intercepts", async () => {
    const { streamFleetEventsUrl } = await import("./events");
    expect(streamFleetEventsUrl("ws_1", "z_1")).toBe(
      "/backend/v1/workspaces/ws_1/fleets/z_1/events/stream",
    );
  });

  it("encodes path segments so a slashy id can not escape the URL", async () => {
    const { streamFleetEventsUrl } = await import("./events");
    expect(streamFleetEventsUrl("ws/1", "z 2")).toBe(
      "/backend/v1/workspaces/ws%2F1/fleets/z%202/events/stream",
    );
  });
});

describe("backfillFleetEventsUrl", () => {
  it("returns a clean same-origin path when no query opts are given", async () => {
    const { backfillFleetEventsUrl } = await import("./events");
    expect(backfillFleetEventsUrl("ws_1", "z_1")).toBe(
      "/backend/v1/workspaces/ws_1/fleets/z_1/events",
    );
  });

  it("appends since/limit through the shared query builder", async () => {
    const { backfillFleetEventsUrl } = await import("./events");
    expect(
      backfillFleetEventsUrl("ws_1", "z_1", { since: "2026-05-15T18:29:58Z", limit: 200 }),
    ).toBe(
      "/backend/v1/workspaces/ws_1/fleets/z_1/events?since=2026-05-15T18%3A29%3A58Z&limit=200",
    );
  });

  it("encodes path segments so a slashy id can not escape the URL", async () => {
    const { backfillFleetEventsUrl } = await import("./events");
    expect(backfillFleetEventsUrl("ws/1", "z 2")).toBe(
      "/backend/v1/workspaces/ws%2F1/fleets/z%202/events",
    );
  });
});
