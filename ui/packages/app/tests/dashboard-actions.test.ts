import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── Shared mocks ───────────────────────────────────────────────────────────
// The (dashboard)/actions module is a thin server-action layer, but NOT a pure
// forwarder: setActiveWorkspace and the create flow both write the
// active-workspace cookie and revalidate the dashboard. We mock the Next.js
// cookie store, revalidatePath, the token wrapper, and the workspaces API
// client so the only behaviour under test is the cookie write + the
// if (result.ok) switch. @/lib/workspace stays REAL so the cookie name
// (ACTIVE_WORKSPACE_COOKIE) is the production constant, not a fixture.

// vi.mock factories are hoisted above the static actions import, so every fn a
// factory references must be created via vi.hoisted() to exist when the
// factory runs (see runners-actions.test.ts).
const {
  cookiesMock,
  setSpy,
  revalidatePathMock,
  withTokenMock,
  createTenantWorkspaceMock,
  listTenantWorkspacesMock,
} = vi.hoisted(() => {
  const setSpy = vi.fn();
  return {
    setSpy,
    cookiesMock: vi.fn(async () => ({ set: setSpy })),
    revalidatePathMock: vi.fn(),
    withTokenMock: vi.fn(),
    createTenantWorkspaceMock: vi.fn(),
    listTenantWorkspacesMock: vi.fn(),
  };
});

vi.mock("next/headers", () => ({ cookies: cookiesMock }));
vi.mock("next/cache", () => ({ revalidatePath: revalidatePathMock }));
vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
// The source only uses createTenantWorkspace, but @/lib/workspace (kept real)
// value-imports listTenantWorkspaces at module-eval (cache(listTenantWorkspaces)),
// so the factory must export it too or the real workspace.ts import throws.
vi.mock("@/lib/api/workspaces", () => ({
  createTenantWorkspace: createTenantWorkspaceMock,
  listTenantWorkspaces: listTenantWorkspacesMock,
}));

import { setActiveWorkspace, createWorkspaceAction } from "@/app/(dashboard)/actions";
import { ACTIVE_WORKSPACE_COOKIE } from "@/lib/workspace";

// 60 * 60 * 24 * 365 — the cookie's one-year max-age, mirrored from the source
// so a drift in the constant fails this assertion loudly.
const ONE_YEAR_S = 31_536_000;

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

describe("setActiveWorkspace — server-side cookie write", () => {
  it("writes the active-workspace cookie with a year max-age and revalidates the layout", async () => {
    await setActiveWorkspace("ws42");

    expect(setSpy).toHaveBeenCalledTimes(1);
    expect(setSpy).toHaveBeenCalledWith({
      name: ACTIVE_WORKSPACE_COOKIE,
      value: "ws42",
      path: "/",
      sameSite: "lax",
      maxAge: ONE_YEAR_S,
    });
    expect(revalidatePathMock).toHaveBeenCalledWith("/", "layout");
  });
});

describe("createWorkspaceAction — create then switch (no half-switch on failure)", () => {
  it("ok-branch: creates the workspace, switches the cookie to the new id, returns the envelope", async () => {
    const data = { workspace_id: "ws9", name: "scrappy-otter", request_id: "req-1" };
    createTenantWorkspaceMock.mockResolvedValueOnce(data);

    const body = { name: "scrappy-otter" };
    const r = await createWorkspaceAction(body);

    // token threaded first, body second — the order the source uses.
    expect(createTenantWorkspaceMock).toHaveBeenCalledWith("tok", body);
    // the cookie is switched to the freshly-minted workspace id.
    expect(setSpy).toHaveBeenCalledTimes(1);
    expect(setSpy).toHaveBeenCalledWith({
      name: ACTIVE_WORKSPACE_COOKIE,
      value: "ws9",
      path: "/",
      sameSite: "lax",
      maxAge: ONE_YEAR_S,
    });
    expect(revalidatePathMock).toHaveBeenCalledWith("/", "layout");
    expect(r).toEqual({ ok: true, data });
  });

  it("failure-branch: leaves the cookie untouched and forwards the failure envelope", async () => {
    const failure = {
      ok: false as const,
      error: "Workspace limit reached",
      status: 409,
      errorCode: "UZ-WSP-009",
    };
    // withToken resolves to a failure envelope; the inner client never resolves
    // a workspace id, so the action must NOT write or revalidate.
    withTokenMock.mockResolvedValueOnce(failure);

    const r = await createWorkspaceAction({ name: "doomed" });

    expect(r).toEqual(failure);
    expect(setSpy).not.toHaveBeenCalled();
    expect(revalidatePathMock).not.toHaveBeenCalled();
  });
});
