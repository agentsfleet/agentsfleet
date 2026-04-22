import { afterEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

const mockPage = { events: [{ id: "evt_1", zombie_id: "z_1", workspace_id: "ws_1", event_type: "event_received", detail: "", created_at: 1000 }], next_cursor: null };

describe("listWorkspaceActivity", () => {
  it("calls /v1/workspaces/:ws/activity without cursor param", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockPage });
    const { listWorkspaceActivity } = await import("./activity");
    const page = await listWorkspaceActivity("ws_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/activity"),
      expect.anything(),
    );
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).not.toContain("cursor=");
    expect(page.events).toHaveLength(1);
  });

  it("appends cursor param when provided", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ events: [], next_cursor: null }) });
    const { listWorkspaceActivity } = await import("./activity");
    await listWorkspaceActivity("ws_1", "tok", "abc123");
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("cursor=abc123");
  });
});

describe("listZombieActivity", () => {
  it("calls /v1/workspaces/:ws/zombies/:id/activity", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => mockPage });
    const { listZombieActivity } = await import("./activity");
    await listZombieActivity("ws_1", "z_1", "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/zombies/z_1/activity"),
      expect.anything(),
    );
  });

  it("appends cursor param on zombie activity", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ events: [], next_cursor: "next" }) });
    const { listZombieActivity } = await import("./activity");
    const page = await listZombieActivity("ws_1", "z_1", "tok", "prev_cursor");
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("cursor=prev_cursor");
    expect(page.next_cursor).toBe("next");
  });
});
