import { describe, expect, it, vi } from "vitest";

const requestMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/api/client", () => ({
  request: requestMock,
  requestWithEtag: vi.fn(),
}));

import { firstTenantWorkspace } from "@/lib/api/workspaces";

const TOKEN = "tok";
const WORKSPACE = {
  id: "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
  name: "primary",
  created_at: 1_777_507_200_000,
};

function pageWith(items: unknown[]) {
  return {
    items,
    tenant_id: "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01",
    total: null,
    next_cursor: null,
  };
}

describe("firstTenantWorkspace", () => {
  it("test_entry_redirect_single_page: sends limit=1 and never walks continuations", async () => {
    requestMock.mockReset();
    // A next_cursor is present — a walker would follow it; the redirect must not.
    requestMock.mockResolvedValue({ ...pageWith([WORKSPACE]), next_cursor: "cur_1" });

    const first = await firstTenantWorkspace(TOKEN);

    expect(requestMock).toHaveBeenCalledTimes(1);
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/tenants/me/workspaces?limit=1",
      { method: "GET" },
      TOKEN,
    );
    expect(first?.id).toBe(WORKSPACE.id);
  });

  it("a tenant with no workspaces resolves null (the create-first empty state)", async () => {
    requestMock.mockReset();
    requestMock.mockResolvedValue(pageWith([]));
    await expect(firstTenantWorkspace(TOKEN)).resolves.toBeNull();
  });
});
