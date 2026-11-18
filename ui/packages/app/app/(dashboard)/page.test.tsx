import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/lib/api/errors";
import { ERROR_CODE } from "@/lib/errors";

// The dashboard entry decides, from one refused request, whether a new
// account is still opening: Clerk has the person, but the daemon's
// `user.created` handler has not yet written the tenant back, so the token
// carries none. Only that case may wait on `AccountSetup`; every other refusal
// must reach the error boundary, where "try again" is the honest answer.
const mocks = vi.hoisted(() => ({
  first: vi.fn(),
  tenantMissing: vi.fn(),
  redirect: vi.fn((to: string) => {
    throw new Error(`redirect:${to}`);
  }),
}));
vi.mock("@/lib/auth/credential", () => ({
  requireCredential: () => Promise.resolve("session-token"),
  tenantClaimMissing: () => mocks.tenantMissing(),
}));
vi.mock("@/lib/api/workspaces", () => ({
  firstTenantWorkspace: (token: string) => mocks.first(token),
}));
vi.mock("next/navigation", () => ({ redirect: (to: string) => mocks.redirect(to) }));
vi.mock("@/components/layout/NoWorkspaceEmptyState", () => ({ default: () => null }));
vi.mock("./AccountSetup", () => ({ AccountSetup: () => null }));

import DashboardIndexPage from "./page";
import { AccountSetup } from "./AccountSetup";
import NoWorkspaceEmptyState from "@/components/layout/NoWorkspaceEmptyState";

const WORKSPACE = "01900000-0000-7000-8000-0000000000a1";
const refused = (code: string, status = 401) => new ApiError("refused", status, code as ApiError["code"]);

beforeEach(() => {
  mocks.first.mockReset();
  mocks.tenantMissing.mockReset();
  mocks.redirect.mockClear();
});

describe("dashboard entry — an account still opening", () => {
  it.each([
    ["a token refused at authentication", ERROR_CODE.AUTH_UNAUTHORIZED, 401],
    ["a token refused at the scope gate", ERROR_CODE.INSUFFICIENT_SCOPE, 403],
  ])("waits on account setup for %s that carries no tenant yet", async (_case, code, status) => {
    mocks.first.mockRejectedValueOnce(refused(code, status));
    mocks.tenantMissing.mockResolvedValueOnce(true);
    const page = await DashboardIndexPage();
    expect(page.type).toBe(AccountSetup);
  });

  it("sends a refused token that already carries a tenant to the error page: waiting cannot help it", async () => {
    const failure = refused(ERROR_CODE.AUTH_UNAUTHORIZED);
    mocks.first.mockRejectedValueOnce(failure);
    mocks.tenantMissing.mockResolvedValueOnce(false);
    await expect(DashboardIndexPage()).rejects.toBe(failure);
  });

  it("never mistakes an outage for an opening account, even before the tenant lands", async () => {
    const failure = new ApiError("unavailable", 503, "UZ-INTERNAL-003");
    mocks.first.mockRejectedValueOnce(failure);
    mocks.tenantMissing.mockResolvedValue(true);
    await expect(DashboardIndexPage()).rejects.toBe(failure);
    expect(mocks.tenantMissing).not.toHaveBeenCalled();
  });

  it("sends a failure that is not an API answer to the error page", async () => {
    const failure = new TypeError("fetch failed");
    mocks.first.mockRejectedValueOnce(failure);
    await expect(DashboardIndexPage()).rejects.toBe(failure);
  });
});

describe("dashboard entry — an opened account", () => {
  it("redirects once to the first workspace's new fleet page", async () => {
    mocks.first.mockResolvedValueOnce({ id: WORKSPACE });
    await expect(DashboardIndexPage()).rejects.toThrow(`redirect:/w/${WORKSPACE}/fleets/new`);
    expect(mocks.first).toHaveBeenCalledWith("session-token");
  });

  it("shows the create-workspace empty state only for a successful empty list", async () => {
    mocks.first.mockResolvedValueOnce(undefined);
    const page = await DashboardIndexPage();
    expect(page.type).toBe(NoWorkspaceEmptyState);
  });
});
