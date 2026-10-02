import { describe, expect, it, vi } from "vitest";

const requestMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/api/client", () => ({
  request: requestMock,
  requestWithEtag: vi.fn(),
}));

import { WORKSPACE_LIST_PAGE_LIMIT, firstTenantWorkspace } from "@/lib/api/workspaces";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";

const TOKEN = "tok";
const CURSOR = "cur_1";
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
  it("test_entry_redirect_single_page: stops at the first page holding an owned workspace", async () => {
    requestMock.mockReset();
    // A next_cursor is present; the owned row on this page ends the walk anyway.
    requestMock.mockResolvedValue({ ...pageWith([WORKSPACE]), next_cursor: CURSOR });

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

  it("should walk past a full page of joined workspaces to the caller's own", async () => {
    requestMock.mockReset();
    requestMock
      .mockResolvedValueOnce({ ...pageWith([JOINED]), next_cursor: CURSOR })
      .mockResolvedValueOnce(pageWith([WORKSPACE]));

    await expect(firstTenantWorkspace(TOKEN)).resolves.toMatchObject({ id: WORKSPACE.id });
    expect(requestMock).toHaveBeenCalledTimes(2);
    expect(requestMock).toHaveBeenLastCalledWith(
      `/v1/tenants/me/workspaces?limit=${WORKSPACE_LIST_PAGE_LIMIT}&starting_after=${CURSOR}`,
      { method: "GET" },
      TOKEN,
    );
  });

  it("should fall back to the first page's first row when no page holds an owned workspace", async () => {
    requestMock.mockReset();
    const LATER_JOINED = { ...JOINED, id: "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f33" };
    requestMock
      .mockResolvedValueOnce({ ...pageWith([JOINED]), next_cursor: CURSOR })
      .mockResolvedValueOnce(pageWith([LATER_JOINED]));

    await expect(firstTenantWorkspace(TOKEN)).resolves.toMatchObject({ id: JOINED.id });
    expect(requestMock).toHaveBeenCalledTimes(2);
  });

  it("should refuse a cursor the walk has already followed", async () => {
    requestMock.mockReset();
    requestMock.mockResolvedValue({ ...pageWith([JOINED]), next_cursor: CURSOR });

    await expect(firstTenantWorkspace(TOKEN)).rejects.toThrow("workspace pagination repeated a cursor");
    expect(requestMock).toHaveBeenCalledTimes(2);
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
