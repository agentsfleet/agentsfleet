import { describe, expect, it, vi } from "vitest";

const requestMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/api/client", () => ({
  request: requestMock,
  requestWithEtag: vi.fn(),
}));

import { ACCOUNT_ROLE, WORKSPACE_LIST_PAGE_LIMIT, firstTenantWorkspace } from "@/lib/api/workspaces";

const TOKEN = "tok";
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const WORKSPACE = {
  id: "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
  name: "primary",
  created_at: 1_777_507_200_000,
  account: { tenant_id: TENANT_ID, owner_name: "Primary" },
  role: ACCOUNT_ROLE.owner,
};
// The inviter's workspace, older than the invitee's own, so it lists first.
const JOINED = {
  ...WORKSPACE,
  id: "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22",
  name: "inviter",
  created_at: WORKSPACE.created_at - 1,
  account: { tenant_id: "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f02", owner_name: "John" },
  role: ACCOUNT_ROLE.member,
};

function pageWith(items: unknown[]) {
  return {
    items,
    tenant_id: TENANT_ID,
    total: null,
    next_cursor: null,
  };
}

describe("firstTenantWorkspace", () => {
  it("test_entry_redirect_single_page: reads one page and never walks continuations", async () => {
    requestMock.mockReset();
    // A next_cursor is present — a walker would follow it; the redirect must not.
    requestMock.mockResolvedValue({ ...pageWith([WORKSPACE]), next_cursor: "cur_1" });

    const first = await firstTenantWorkspace(TOKEN);

    expect(requestMock).toHaveBeenCalledTimes(1);
    expect(requestMock).toHaveBeenCalledWith(
      `/v1/tenants/me/workspaces?limit=${WORKSPACE_LIST_PAGE_LIMIT}`,
      { method: "GET" },
      TOKEN,
    );
    expect(first?.id).toBe(WORKSPACE.id);
  });

  it("should land an invitee in their own workspace when the inviter's older one lists first", async () => {
    requestMock.mockReset();
    requestMock.mockResolvedValue(pageWith([JOINED, WORKSPACE]));
    await expect(firstTenantWorkspace(TOKEN)).resolves.toMatchObject({ id: WORKSPACE.id });
  });

  it("should fall back to the first joined workspace for a caller who owns none", async () => {
    requestMock.mockReset();
    requestMock.mockResolvedValue(pageWith([JOINED]));
    await expect(firstTenantWorkspace(TOKEN)).resolves.toMatchObject({ id: JOINED.id });
  });

  it("a tenant with no workspaces resolves null (the create-first empty state)", async () => {
    requestMock.mockReset();
    requestMock.mockResolvedValue(pageWith([]));
    await expect(firstTenantWorkspace(TOKEN)).resolves.toBeNull();
  });
});
