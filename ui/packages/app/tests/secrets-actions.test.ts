import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── Shared mocks ───────────────────────────────────────────────────────────
// These secret server actions are thin forwarders: each wraps
// withToken((t) => apiFn(args, t)). We mock the token wrapper and the API
// client so the only behaviour under test is the forwarding shape — that the
// resolved token is threaded into the position the source uses and the wrapped
// {ok, data} result is returned verbatim.

// vi.mock is hoisted above the static actions import, so the mock fns must be
// created via vi.hoisted() to exist when the factories run (see
// runners-actions.test.ts).
const { withTokenMock, createSecretMock, deleteSecretMock } = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  createSecretMock: vi.fn(),
  deleteSecretMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/secrets", () => ({
  createSecret: createSecretMock,
  deleteSecret: deleteSecretMock,
}));

import {
  createSecretAction,
  deleteSecretAction,
} from "@/app/(dashboard)/w/[workspaceId]/secrets/actions";

beforeEach(() => {
  vi.clearAllMocks();
  // withToken just forwards a resolved token to its callback for the happy path.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => ({
    ok: true,
    data: await fn("tok"),
  }));
});
afterEach(() => vi.resetAllMocks());

describe("secret server actions — thin withToken forwarders", () => {
  it("createSecretAction threads the token last and returns the wrapped result", async () => {
    createSecretMock.mockResolvedValueOnce({ name: "openai" });
    const body = { name: "openai", data: { api_key: "sk-xxx" } };
    const r = await createSecretAction("ws-1", body);
    expect(r).toEqual({ ok: true, data: { name: "openai" } });
    expect(createSecretMock).toHaveBeenCalledWith("ws-1", body, "tok");
    expect(deleteSecretMock).not.toHaveBeenCalled();
  });

  it("deleteSecretAction threads the token last and returns the wrapped result", async () => {
    deleteSecretMock.mockResolvedValueOnce(undefined);
    const r = await deleteSecretAction("ws-2", "stripe");
    expect(r).toEqual({ ok: true, data: undefined });
    expect(deleteSecretMock).toHaveBeenCalledWith("ws-2", "stripe", "tok");
    expect(createSecretMock).not.toHaveBeenCalled();
  });

  it("createSecretAction propagates a failure result from withToken untouched", async () => {
    withTokenMock.mockResolvedValueOnce({
      ok: false,
      error: "Unauthorized",
      status: 401,
      errorCode: "UZ-AUTH-001",
    });
    const r = await createSecretAction("ws-1", { name: "n", data: {} });
    expect(r).toEqual({ ok: false, error: "Unauthorized", status: 401, errorCode: "UZ-AUTH-001" });
    expect(createSecretMock).not.toHaveBeenCalled();
  });
});
