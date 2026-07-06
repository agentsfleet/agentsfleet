import { beforeEach, describe, expect, it, vi } from "vitest";

const getToken = vi.fn();
const createTenantWorkspace = vi.fn();

// Post-M118 single-token: createWorkspaceAction resolves its Bearer via
// withToken → auth().getToken() (mock the named clerk export). It no longer
// writes an active-workspace cookie or revalidates — selection is a client
// navigation (router.push('/w/<newId>')); the action just proxies
// createTenantWorkspace and threads the envelope back.
vi.mock("@clerk/nextjs/server", () => ({ auth: vi.fn(async () => ({ getToken })) }));
vi.mock("@/lib/api/workspaces", () => ({ createTenantWorkspace }));

beforeEach(() => {
  vi.clearAllMocks();
});

describe("createWorkspaceAction", () => {
  it("creates a workspace and returns the new id envelope on success", async () => {
    getToken.mockResolvedValue("tok_1");
    createTenantWorkspace.mockResolvedValue({ workspace_id: "ws_new", name: "fresh" });
    const { createWorkspaceAction } = await import("../app/(dashboard)/actions");

    const result = await createWorkspaceAction({ name: "fresh" });

    expect(result.ok).toBe(true);
    expect(result.ok && result.data.workspace_id).toBe("ws_new");
    expect(createTenantWorkspace).toHaveBeenCalledWith("tok_1", { name: "fresh" });
  });

  it("maps a missing token to UZ-AUTH-401 and never calls the client", async () => {
    getToken.mockResolvedValue(null);
    const { createWorkspaceAction } = await import("../app/(dashboard)/actions");

    const result = await createWorkspaceAction({});

    expect(result.ok).toBe(false);
    expect(!result.ok && result.errorCode).toBe("UZ-AUTH-401");
    expect(createTenantWorkspace).not.toHaveBeenCalled();
  });

  it("propagates a backend rejection as a failure envelope", async () => {
    getToken.mockResolvedValue("tok_1");
    const { ApiError } = await import("@/lib/api/errors");
    createTenantWorkspace.mockRejectedValue(
      new ApiError("Missing tenant context on session", 401, "UZ-AUTH-401", "req_1"),
    );
    const { createWorkspaceAction } = await import("../app/(dashboard)/actions");

    const result = await createWorkspaceAction({ name: "x" });

    expect(result.ok).toBe(false);
    expect(!result.ok && result.errorCode).toBe("UZ-AUTH-401");
  });
});
