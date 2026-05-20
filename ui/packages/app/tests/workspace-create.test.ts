import { beforeEach, describe, expect, it, vi } from "vitest";

const cookieSet = vi.fn();
const revalidatePath = vi.fn();
const getToken = vi.fn();
const createTenantWorkspace = vi.fn();

vi.mock("next/headers", () => ({ cookies: vi.fn(async () => ({ set: cookieSet })) }));
vi.mock("next/cache", () => ({ revalidatePath }));
// Post-Stage-1 single-token: createWorkspaceAction resolves its Bearer via
// withToken → auth().getToken() (mock the named clerk export).
vi.mock("@clerk/nextjs/server", () => ({ auth: vi.fn(async () => ({ getToken })) }));
// lib/workspace.ts wraps listTenantWorkspaces in React `cache()` at import,
// and actions.ts imports ACTIVE_WORKSPACE_COOKIE from there — keep the real
// export shape so that module initialises.
vi.mock("@/lib/api/workspaces", () => ({
  createTenantWorkspace,
  listTenantWorkspaces: vi.fn(),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

describe("createWorkspaceAction", () => {
  it("creates a workspace and switches the active cookie on success", async () => {
    getToken.mockResolvedValue("tok_1");
    createTenantWorkspace.mockResolvedValue({ workspace_id: "ws_new", name: "fresh" });
    const { createWorkspaceAction } = await import("../app/(dashboard)/actions");

    const result = await createWorkspaceAction({ name: "fresh" });

    expect(result.ok).toBe(true);
    expect(result.ok && result.data.workspace_id).toBe("ws_new");
    expect(createTenantWorkspace).toHaveBeenCalledWith("tok_1", { name: "fresh" });
    expect(cookieSet).toHaveBeenCalledWith(expect.objectContaining({ value: "ws_new" }));
    expect(revalidatePath).toHaveBeenCalled();
  });

  it("maps a missing token to UZ-AUTH-401 and does not switch the cookie", async () => {
    getToken.mockResolvedValue(null);
    const { createWorkspaceAction } = await import("../app/(dashboard)/actions");

    const result = await createWorkspaceAction({});

    expect(result.ok).toBe(false);
    expect(!result.ok && result.errorCode).toBe("UZ-AUTH-401");
    expect(createTenantWorkspace).not.toHaveBeenCalled();
    expect(cookieSet).not.toHaveBeenCalled();
  });

  it("propagates a backend rejection without switching the cookie", async () => {
    getToken.mockResolvedValue("tok_1");
    const { ApiError } = await import("@/lib/api/errors");
    createTenantWorkspace.mockRejectedValue(
      new ApiError("Missing tenant context on session", 401, "UZ-AUTH-401", "req_1"),
    );
    const { createWorkspaceAction } = await import("../app/(dashboard)/actions");

    const result = await createWorkspaceAction({ name: "x" });

    expect(result.ok).toBe(false);
    expect(cookieSet).not.toHaveBeenCalled();
  });
});
