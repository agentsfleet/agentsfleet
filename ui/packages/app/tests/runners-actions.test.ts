import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SCOPE } from "@/lib/auth/scopes";

// ── Shared mocks ───────────────────────────────────────────────────────────
// The actions module is the dashboard's defence-in-depth gate: each action must
// fail closed on its specific operator scope BEFORE any token round-trip. We
// mock the scope check, the token wrapper, and the API client so the gate's
// branch is the only thing under test (the real security boundary is the
// backend requireScope, proven by the backend integration suite).

// vi.mock is hoisted above the static actions import, so the mock fns must be
// created via vi.hoisted() to exist when the factories run (see runners.test.ts).
const {
  hasScopeMock,
  withTokenMock,
  listRunnersMock,
  createRunnerMock,
  updateRunnerAdminStateMock,
  deleteRunnerMock,
  listRunnerLeasesMock,
} = vi.hoisted(() => ({
  hasScopeMock: vi.fn(),
  withTokenMock: vi.fn(),
  listRunnersMock: vi.fn(),
  createRunnerMock: vi.fn(),
  updateRunnerAdminStateMock: vi.fn(),
  deleteRunnerMock: vi.fn(),
  listRunnerLeasesMock: vi.fn(),
}));

vi.mock("@/lib/auth/platform", () => ({ hasScope: hasScopeMock }));
vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/runners", () => ({
  listRunners: listRunnersMock,
  createRunner: createRunnerMock,
  updateRunnerAdminState: updateRunnerAdminStateMock,
  deleteRunner: deleteRunnerMock,
  listRunnerLeases: listRunnerLeasesMock,
  // Enrollment defaults the action folds into assigned_policy — mirror the
  // real module's values so the forwarded body assertion stays truthful.
  DEFAULT_ASSIGNED_NETWORK_POLICY: "allow_all",
  DEFAULT_WORKER_COUNT: 1,
}));

import {
  listRunnersAction,
  createRunnerAction,
  updateRunnerAdminStateAction,
  deleteRunnerAction,
  listRunnerLeasesAction,
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

// test_runner_ui_gates_on_runner_scopes (Dimension 4.1)
describe("runner server actions — per-scope gate (defence-in-depth)", () => {
  it("listRunnersAction gates on runner:read and fails closed 403 UZ-AUTH-022 without it", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const r = await listRunnersAction({});
    expect(r).toEqual({
      ok: false,
      error: "Operator scope required: runner:read",
      status: 403,
      errorCode: "UZ-AUTH-022",
    });
    expect(hasScopeMock).toHaveBeenCalledWith(SCOPE.RUNNER_READ);
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(listRunnersMock).not.toHaveBeenCalled();
  });

  it("createRunnerAction gates on runner:enroll and fails closed 403 UZ-AUTH-022 without it", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const body = { host_id: "web-prod-1", sandbox_tier: "landlock_full" as const, labels: ["gpu"] };
    const r = await createRunnerAction(body);
    expect(r).toEqual({
      ok: false,
      error: "Operator scope required: runner:enroll",
      status: 403,
      errorCode: "UZ-AUTH-022",
    });
    expect(hasScopeMock).toHaveBeenCalledWith(SCOPE.RUNNER_ENROLL);
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(createRunnerMock).not.toHaveBeenCalled();
  });

  it("updateRunnerAdminStateAction gates on runner:write and fails closed 403 UZ-AUTH-022 without it", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const r = await updateRunnerAdminStateAction("runner-1", "cordon");
    expect(r).toEqual({
      ok: false,
      error: "Operator scope required: runner:write",
      status: 403,
      errorCode: "UZ-AUTH-022",
    });
    expect(hasScopeMock).toHaveBeenCalledWith(SCOPE.RUNNER_WRITE);
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(updateRunnerAdminStateMock).not.toHaveBeenCalled();
  });

  it("deleteRunnerAction gates on runner:write — the same scope as revoke, deliberately — and fails closed without it", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const r = await deleteRunnerAction("runner-1");
    expect(r).toEqual({
      ok: false,
      error: "Operator scope required: runner:write",
      status: 403,
      errorCode: "UZ-AUTH-022",
    });
    expect(hasScopeMock).toHaveBeenCalledWith(SCOPE.RUNNER_WRITE);
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(deleteRunnerMock).not.toHaveBeenCalled();
  });

  it("deleteRunnerAction forwards the token round-trip when the scope holds", async () => {
    hasScopeMock.mockResolvedValueOnce(true);
    deleteRunnerMock.mockResolvedValueOnce(undefined);
    const r = await deleteRunnerAction("runner-1");
    expect(r).toEqual({ ok: true, data: undefined });
    expect(deleteRunnerMock).toHaveBeenCalledWith("tok", "runner-1");
  });

  it("listRunnerLeasesAction gates on runner:read and fails closed 403 UZ-AUTH-022 without it", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const r = await listRunnerLeasesAction("runner-1", {});
    expect(r).toEqual({
      ok: false,
      error: "Operator scope required: runner:read",
      status: 403,
      errorCode: "UZ-AUTH-022",
    });
    expect(hasScopeMock).toHaveBeenCalledWith(SCOPE.RUNNER_READ);
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(listRunnerLeasesMock).not.toHaveBeenCalled();
  });

  it("listRunnersAction forwards keyset params through withToken to the client when scoped", async () => {
    hasScopeMock.mockResolvedValueOnce(true);
    listRunnersMock.mockResolvedValueOnce({ items: [], total: 0, next_cursor: null });
    const params = { starting_after: "cursor-1", limit: 50 };
    const r = await listRunnersAction(params);
    expect(r).toEqual({ ok: true, data: { items: [], total: 0, next_cursor: null } });
    expect(listRunnersMock).toHaveBeenCalledWith("tok", params);
  });

  it("createRunnerAction forwards the mint body through withToken to the client when scoped", async () => {
    hasScopeMock.mockResolvedValueOnce(true);
    createRunnerMock.mockResolvedValueOnce({ runner_id: "r1", runner_token: "agt_rabc" });
    const body = { host_id: "web-prod-1", sandbox_tier: "container_nested" as const, labels: [] };
    const r = await createRunnerAction(body);
    expect(r).toEqual({ ok: true, data: { runner_id: "r1", runner_token: "agt_rabc" } });
    // The action wraps the dialog's tier into the full assignment envelope with
    // the documented enrollment defaults.
    expect(createRunnerMock).toHaveBeenCalledWith("tok", {
      host_id: "web-prod-1",
      assigned_policy: {
        sandbox_tier: "container_nested",
        network_policy: "allow_all",
        registry_allowlist: [],
        worker_count: 1,
      },
      labels: [],
    });
  });

  it("updateRunnerAdminStateAction forwards the runner state change through withToken when scoped", async () => {
    hasScopeMock.mockResolvedValueOnce(true);
    updateRunnerAdminStateMock.mockResolvedValueOnce({ id: "runner-1", admin_state: "cordoned" });
    const r = await updateRunnerAdminStateAction("runner-1", "cordon");
    expect(r).toEqual({ ok: true, data: { id: "runner-1", admin_state: "cordoned" } });
    expect(updateRunnerAdminStateMock).toHaveBeenCalledWith("tok", "runner-1", "cordon");
  });

  it("listRunnerLeasesAction forwards keyset paging through withToken when scoped", async () => {
    hasScopeMock.mockResolvedValueOnce(true);
    listRunnerLeasesMock.mockResolvedValueOnce({ items: [], total: 0, next_cursor: null });
    const params = { starting_after: "lease-9", limit: 25 };
    const r = await listRunnerLeasesAction("runner-1", params);
    expect(r).toEqual({ ok: true, data: { items: [], total: 0, next_cursor: null } });
    expect(listRunnerLeasesMock).toHaveBeenCalledWith("tok", "runner-1", params);
  });
});
