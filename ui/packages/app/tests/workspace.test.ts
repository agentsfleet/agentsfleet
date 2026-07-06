import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// M118: `lib/workspace.ts` is slimmed to a single survivor. The active workspace
// is now an explicit URL segment (see `lib/workspace-routes.ts`), so the whole
// cookie/claim resolution + retry machinery (`resolveActiveWorkspaceId`,
// `withWorkspaceScope`, `orFallback`, `ACTIVE_WORKSPACE_COOKIE`, …) is gone.

const listTenantWorkspaces = vi.fn();
vi.mock("../lib/api/workspaces", () => ({ listTenantWorkspaces }));

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => {
  vi.resetModules();
});

describe("lib/workspace module surface (Dimension 4.2)", () => {
  it("exports only listTenantWorkspacesCached — the resolution machinery is gone", async () => {
    const mod = await import("../lib/workspace");
    expect(Object.keys(mod).sort()).toEqual(["listTenantWorkspacesCached"]);
    // The deleted exports must not resurface.
    for (const gone of [
      "resolveActiveWorkspaceId",
      "withWorkspaceScope",
      "resolveFromList",
      "readWorkspaceClaim",
      "isWorkspaceRejection",
      "orFallback",
      "ACTIVE_WORKSPACE_COOKIE",
    ]) {
      expect(gone in mod).toBe(false);
    }
  });
});

describe("listTenantWorkspacesCached", () => {
  it("forwards the token to listTenantWorkspaces and returns its result", async () => {
    listTenantWorkspaces.mockResolvedValue({
      items: [{ id: "ws_1", name: "Alpha", created_at: 0 }],
      total: 1,
    });
    const { listTenantWorkspacesCached } = await import("../lib/workspace");
    const result = await listTenantWorkspacesCached("tok_1");
    expect(listTenantWorkspaces).toHaveBeenCalledWith("tok_1");
    expect(result.items[0]?.id).toBe("ws_1");
  });
});
