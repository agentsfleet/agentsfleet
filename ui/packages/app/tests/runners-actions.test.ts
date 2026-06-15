import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── Shared mocks ───────────────────────────────────────────────────────────
// The actions module is the dashboard's defence-in-depth gate: it must fail
// closed on the platform_admin claim BEFORE any token round-trip. We mock the
// claim, the token wrapper, and the API client so the gate's branch is the only
// thing under test (the real security boundary is the backend, proven by the
// backend integration suite).

// vi.mock is hoisted above the static actions import, so the mock fns must be
// created via vi.hoisted() to exist when the factories run (see runners.test.ts).
const {
  readPlatformAdminClaimMock,
  withTokenMock,
  listRunnersMock,
  createRunnerMock,
  updateRunnerAdminStateMock,
  listRunnerEventsMock,
} = vi.hoisted(() => ({
  readPlatformAdminClaimMock: vi.fn(),
  withTokenMock: vi.fn(),
  listRunnersMock: vi.fn(),
  createRunnerMock: vi.fn(),
  updateRunnerAdminStateMock: vi.fn(),
  listRunnerEventsMock: vi.fn(),
}));

vi.mock("@/lib/auth/platform", () => ({ readPlatformAdminClaim: readPlatformAdminClaimMock }));
vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/runners", () => ({
  listRunners: listRunnersMock,
  createRunner: createRunnerMock,
  updateRunnerAdminState: updateRunnerAdminStateMock,
  listRunnerEvents: listRunnerEventsMock,
}));

import {
  listRunnersAction,
  createRunnerAction,
  updateRunnerAdminStateAction,
  listRunnerEventsAction,
} from "@/app/(dashboard)/admin/runners/actions";

beforeEach(() => {
  vi.clearAllMocks();
  // withToken just forwards a resolved token to its callback for the happy path.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => ({
    ok: true,
    data: await fn("tok"),
  }));
});
afterEach(() => vi.resetAllMocks());

describe("runner server actions — platform-admin gate (defence-in-depth)", () => {
  it("listRunnersAction fails closed with 403 UZ-AUTH-021 for a non-admin, before any round-trip", async () => {
    readPlatformAdminClaimMock.mockResolvedValueOnce(false);
    const r = await listRunnersAction({ page: 1 });
    expect(r).toEqual({
      ok: false,
      error: "Platform-admin access required",
      status: 403,
      errorCode: "UZ-AUTH-021",
    });
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(listRunnersMock).not.toHaveBeenCalled();
  });

  it("createRunnerAction fails closed with 403 UZ-AUTH-021 for a non-admin, before any round-trip", async () => {
    readPlatformAdminClaimMock.mockResolvedValueOnce(false);
    const body = { host_id: "web-prod-1", sandbox_tier: "landlock_full" as const, labels: ["gpu"] };
    const r = await createRunnerAction(body);
    expect(r).toEqual({
      ok: false,
      error: "Platform-admin access required",
      status: 403,
      errorCode: "UZ-AUTH-021",
    });
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(createRunnerMock).not.toHaveBeenCalled();
  });

  it("updateRunnerAdminStateAction fails closed with 403 UZ-AUTH-021 for a non-admin, before any round-trip", async () => {
    readPlatformAdminClaimMock.mockResolvedValueOnce(false);
    const r = await updateRunnerAdminStateAction("runner-1", "cordon");
    expect(r).toEqual({
      ok: false,
      error: "Platform-admin access required",
      status: 403,
      errorCode: "UZ-AUTH-021",
    });
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(updateRunnerAdminStateMock).not.toHaveBeenCalled();
  });

  it("listRunnerEventsAction fails closed with 403 UZ-AUTH-021 for a non-admin, before any round-trip", async () => {
    readPlatformAdminClaimMock.mockResolvedValueOnce(false);
    const r = await listRunnerEventsAction("runner-1", { page: 1 });
    expect(r).toEqual({
      ok: false,
      error: "Platform-admin access required",
      status: 403,
      errorCode: "UZ-AUTH-021",
    });
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(listRunnerEventsMock).not.toHaveBeenCalled();
  });

  it("listRunnersAction forwards params through withToken to the client when admin", async () => {
    readPlatformAdminClaimMock.mockResolvedValueOnce(true);
    listRunnersMock.mockResolvedValueOnce({ items: [], total: 0, page: 1, page_size: 25 });
    const params = { page: 2, page_size: 25, sort: "host_id" as const };
    const r = await listRunnersAction(params);
    expect(r).toEqual({ ok: true, data: { items: [], total: 0, page: 1, page_size: 25 } });
    expect(listRunnersMock).toHaveBeenCalledWith("tok", params);
  });

  it("createRunnerAction forwards the mint body through withToken to the client when admin", async () => {
    readPlatformAdminClaimMock.mockResolvedValueOnce(true);
    createRunnerMock.mockResolvedValueOnce({ runner_id: "r1", runner_token: "agt_rabc" });
    const body = { host_id: "web-prod-1", sandbox_tier: "container_nested" as const, labels: [] };
    const r = await createRunnerAction(body);
    expect(r).toEqual({ ok: true, data: { runner_id: "r1", runner_token: "agt_rabc" } });
    expect(createRunnerMock).toHaveBeenCalledWith("tok", body);
  });

  it("updateRunnerAdminStateAction forwards the runner state change through withToken when admin", async () => {
    readPlatformAdminClaimMock.mockResolvedValueOnce(true);
    updateRunnerAdminStateMock.mockResolvedValueOnce({ id: "runner-1", admin_state: "cordoned" });
    const r = await updateRunnerAdminStateAction("runner-1", "cordon");
    expect(r).toEqual({ ok: true, data: { id: "runner-1", admin_state: "cordoned" } });
    expect(updateRunnerAdminStateMock).toHaveBeenCalledWith("tok", "runner-1", "cordon");
  });

  it("listRunnerEventsAction forwards activity-history paging through withToken when admin", async () => {
    readPlatformAdminClaimMock.mockResolvedValueOnce(true);
    listRunnerEventsMock.mockResolvedValueOnce({ items: [], total: 0, page: 1, page_size: 25 });
    const params = { page: 2, page_size: 25 };
    const r = await listRunnerEventsAction("runner-1", params);
    expect(r).toEqual({ ok: true, data: { items: [], total: 0, page: 1, page_size: 25 } });
    expect(listRunnerEventsMock).toHaveBeenCalledWith("tok", "runner-1", params);
  });
});
