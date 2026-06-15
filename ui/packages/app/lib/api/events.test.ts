import { afterEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

const mockResponse = {
  items: [
    {
      event_id: "1700000000000-0",
      agent_id: "z_1",
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

describe("listAgentEvents", () => {
  it("hits the per-agent events endpoint without a cursor by default", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockResponse });
    const { listAgentEvents } = await import("./events");
    const page = await listAgentEvents("ws_1", "z_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/agents/z_1/events"),
      expect.anything(),
    );
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).not.toContain("cursor=");
    expect(page.items[0]!.actor).toBe("steer:kishore");
  });

  it("forwards actor / since / cursor / limit", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) });
    const { listAgentEvents } = await import("./events");
    await listAgentEvents("ws_1", "z_1", "tok", {
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
    const { listAgentEvents } = await import("./events");
    await listAgentEvents("ws_1", "z_1", "tok", { actor: "cron", limit: 10 });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("actor=cron");
    expect(url).not.toContain("since=");
  });

  it("produces a clean URL (no trailing ?) when opts is an empty object", async () => {
    // Exercises the false branch of `qs.length > 0 ? ... : ""` in buildQuery.
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) });
    const { listAgentEvents } = await import("./events");
    await listAgentEvents("ws_1", "z_1", "tok", {});
    const url = fetchMock.mock.calls[0]![0] as string;
    // With an empty opts object, buildQuery produces "" so the URL must not end in "?".
    expect(url).not.toContain("?");
    expect(url).toContain("/v1/workspaces/ws_1/agents/z_1/events");
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

  it("forwards a agent_id drill-down filter", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockResponse });
    const { listWorkspaceEvents } = await import("./events");
    await listWorkspaceEvents("ws_1", "tok", { agent_id: "z_2" });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("agent_id=z_2");
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

describe("streamAgentEventsUrl", () => {
  it("returns a same-origin path the Next Route Handler intercepts", async () => {
    const { streamAgentEventsUrl } = await import("./events");
    expect(streamAgentEventsUrl("ws_1", "z_1")).toBe(
      "/backend/v1/workspaces/ws_1/agents/z_1/events/stream",
    );
  });

  it("encodes path segments so a slashy id can not escape the URL", async () => {
    const { streamAgentEventsUrl } = await import("./events");
    expect(streamAgentEventsUrl("ws/1", "z 2")).toBe(
      "/backend/v1/workspaces/ws%2F1/agents/z%202/events/stream",
    );
  });
});
