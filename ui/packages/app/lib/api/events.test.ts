import { afterEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

const mockResponse = {
  items: [
    {
      event_id: "1700000000000-0",
      zombie_id: "z_1",
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

describe("listZombieEvents", () => {
  it("hits the per-zombie events endpoint without a cursor by default", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockResponse });
    const { listZombieEvents } = await import("./events");
    const page = await listZombieEvents("ws_1", "z_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/zombies/z_1/events"),
      expect.anything(),
    );
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).not.toContain("cursor=");
    expect(page.items[0]!.actor).toBe("steer:kishore");
  });

  it("forwards actor / since / cursor / limit", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) });
    const { listZombieEvents } = await import("./events");
    await listZombieEvents("ws_1", "z_1", "tok", {
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

  it("forwards a zombie_id drill-down filter", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockResponse });
    const { listWorkspaceEvents } = await import("./events");
    await listWorkspaceEvents("ws_1", "tok", { zombie_id: "z_2" });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("zombie_id=z_2");
  });
});

describe("streamZombieEventsUrl", () => {
  it("returns a same-origin path the Next Route Handler intercepts", async () => {
    const { streamZombieEventsUrl } = await import("./events");
    expect(streamZombieEventsUrl("ws_1", "z_1")).toBe(
      "/backend/v1/workspaces/ws_1/zombies/z_1/events/stream",
    );
  });

  it("encodes path segments so a slashy id can not escape the URL", async () => {
    const { streamZombieEventsUrl } = await import("./events");
    expect(streamZombieEventsUrl("ws/1", "z 2")).toBe(
      "/backend/v1/workspaces/ws%2F1/zombies/z%202/events/stream",
    );
  });
});
