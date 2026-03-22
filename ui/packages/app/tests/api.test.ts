import { afterEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.fn();

vi.stubGlobal("fetch", fetchMock);

afterEach(() => {
  fetchMock.mockReset();
});

describe("app api client", () => {
  it("sends bearer auth and parses successful workspace list responses", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: async () => ({
        data: [{ id: "ws_1" }],
        total: 1,
        page: 1,
        per_page: 20,
      }),
    });

    const mod = await import("../lib/api");
    const res = await mod.listWorkspaces("token_123");

    expect(fetchMock).toHaveBeenCalledWith(
      "https://api.usezombie.com/v1/workspaces",
      expect.objectContaining({
        headers: expect.objectContaining({
          "Content-Type": "application/json",
          Authorization: "Bearer token_123",
        }),
      }),
    );
    expect(res.total).toBe(1);
    expect(res.data[0]?.id).toBe("ws_1");
  });

  it("uses POST for mutating workspace and run actions", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: async () => ({ id: "run_1", status: "RETRYING" }),
    });

    const mod = await import("../lib/api");

    await mod.pauseWorkspace("ws_1", "token");
    await mod.resumeWorkspace("ws_1", "token");
    await mod.retryRun("run_1", "token");
    await mod.syncSpecs("ws_1", "token");

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "https://api.usezombie.com/v1/workspaces/ws_1/pause",
      expect.objectContaining({ method: "POST" }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "https://api.usezombie.com/v1/workspaces/ws_1/resume",
      expect.objectContaining({ method: "POST" }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      3,
      "https://api.usezombie.com/v1/runs/run_1/retry",
      expect.objectContaining({ method: "POST" }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      4,
      "https://api.usezombie.com/v1/workspaces/ws_1/specs/sync",
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("requests workspace runs, single run, transitions, and specs", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: async () => ({ data: [], total: 0, page: 1, per_page: 20 }),
    });

    const mod = await import("../lib/api");

    await mod.listRuns("ws_22", "token");
    await mod.getRun("run_22", "token");
    await mod.listRunTransitions("run_22", "token");
    await mod.listSpecs("ws_22", "token");

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "https://api.usezombie.com/v1/workspaces/ws_22/runs",
      expect.any(Object),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "https://api.usezombie.com/v1/runs/run_22",
      expect.any(Object),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      3,
      "https://api.usezombie.com/v1/runs/run_22/transitions",
      expect.any(Object),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      4,
      "https://api.usezombie.com/v1/workspaces/ws_22/specs",
      expect.any(Object),
    );
  });

  it("surfaces parsed API errors with status and code", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 403,
      statusText: "Forbidden",
      json: async () => ({ error: "workspace forbidden", code: "FORBIDDEN" }),
    });

    const mod = await import("../lib/api");

    await expect(mod.getWorkspace("ws_denied", "token")).rejects.toMatchObject({
      message: "workspace forbidden",
      status: 403,
      code: "FORBIDDEN",
    });
  });

  it("falls back to status text when error response JSON is unavailable", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 500,
      statusText: "Server exploded",
      json: async () => {
        throw new Error("bad json");
      },
    });

    const mod = await import("../lib/api");

    await expect(mod.getRun("run_bad", "token")).rejects.toMatchObject({
      message: "Server exploded",
      status: 500,
      code: undefined,
    });
  });
});
