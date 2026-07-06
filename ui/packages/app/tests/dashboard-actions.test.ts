import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── Shared mocks ───────────────────────────────────────────────────────────
// (dashboard)/actions is now a thin server-action forwarder. Post-M118 it no
// longer writes the active-workspace cookie or revalidates — selection is a
// client navigation (router.push('/w/<newId>'), see CreateWorkspaceDialog).
// createWorkspaceAction just threads createTenantWorkspace through withToken and
// returns the {ok,data} envelope. We mock the token wrapper and the workspaces
// API client so the only behaviour under test is the forward + envelope
// passthrough.

// vi.mock factories are hoisted above the static actions import, so every fn a
// factory references must be created via vi.hoisted() to exist when the
// factory runs.
const { withTokenMock, createTenantWorkspaceMock } = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  createTenantWorkspaceMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/workspaces", () => ({ createTenantWorkspace: createTenantWorkspaceMock }));

import { createWorkspaceAction } from "@/app/(dashboard)/actions";

beforeEach(() => {
  vi.clearAllMocks();
  // withToken forwards a resolved token to its callback for the happy path,
  // returning the {ok:true,data} envelope the action threads back to callers.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => ({
    ok: true,
    data: await fn("tok"),
  }));
});
afterEach(() => vi.resetAllMocks());

describe("createWorkspaceAction — forwards create through withToken", () => {
  it("creates the workspace under the resolved token and returns the envelope", async () => {
    const data = { workspace_id: "ws9", name: "scrappy-otter", request_id: "req-1" };
    createTenantWorkspaceMock.mockResolvedValueOnce(data);

    const body = { name: "scrappy-otter" };
    const r = await createWorkspaceAction(body);

    // token threaded first, body second — the order the source uses.
    expect(createTenantWorkspaceMock).toHaveBeenCalledWith("tok", body);
    expect(r).toEqual({ ok: true, data });
  });

  it("forwards a withToken failure envelope untouched, without attempting the create", async () => {
    const failure = {
      ok: false as const,
      error: "Workspace limit reached",
      status: 409,
      errorCode: "UZ-WSP-009",
    };
    // withToken resolves to a failure envelope (e.g. missing token); the inner
    // client never runs, so the action just threads the failure straight back.
    withTokenMock.mockResolvedValueOnce(failure);

    const r = await createWorkspaceAction({ name: "doomed" });

    expect(r).toEqual(failure);
    expect(createTenantWorkspaceMock).not.toHaveBeenCalled();
  });
});
