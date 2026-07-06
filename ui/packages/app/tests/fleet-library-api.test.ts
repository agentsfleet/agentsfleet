import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Pure API-client tests for lib/api/fleet-library against a stubbed fetch. The
// dashboard/install component + route tests mock this module, so these cover the
// real transport (GET/POST + the cache() wrapper) directly.
const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => {
  fetchMock.mockReset();
});

describe("lib/api/fleet-library", () => {
  it("listWorkspaceFleetLibrary sends GET with bearer to the gallery path", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [{ id: "tmpl_1", name: "T", visibility: "platform" }], total: 1 }),
    });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.listWorkspaceFleetLibrary("ws_1", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleet-libraries"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tkn" }),
      }),
    );
    expect(res.items[0]?.id).toBe("tmpl_1");
  });

  it("the cached reader forwards to the same GET", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [] }) });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.listWorkspaceFleetLibraryCached("ws_2", "tkn2");
    expect(res.items).toHaveLength(0);
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_2/fleet-libraries"),
      expect.objectContaining({ method: "GET" }),
    );
  });

  it("onboardWorkspaceFleetLibrary POSTs the body to the gallery path", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ id: "tmpl_new", name: "N", visibility: "tenant", content_hash: "sha256:x" }),
    });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.onboardWorkspaceFleetLibrary("ws_1", { source_kind: "github", source_ref: "acme/fleet" }, "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleet-libraries"),
      expect.objectContaining({ method: "POST" }),
    );
    expect(res.id).toBe("tmpl_new");
  });
});
