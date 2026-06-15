import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── Shared mocks ───────────────────────────────────────────────────────────
// These actions are thin forwarders: each wraps withToken((t) => apiFn(t, ...)).
// We mock the token wrapper and the api_keys client so the only thing under test
// is the forwarding — token threaded into the first position, args passed
// through, and the wrapped {ok,data} envelope returned. The real auth/secrecy
// boundary is the backend, proven by the integration suite.

// vi.mock is hoisted above the static actions import, so the mock fns must be
// created via vi.hoisted() to exist when the factories run (see runners.test.ts).
const { withTokenMock, listApiKeysMock, createApiKeyMock, revokeApiKeyMock, deleteApiKeyMock } = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  listApiKeysMock: vi.fn(),
  createApiKeyMock: vi.fn(),
  revokeApiKeyMock: vi.fn(),
  deleteApiKeyMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/api_keys", () => ({
  listApiKeys: listApiKeysMock,
  createApiKey: createApiKeyMock,
  revokeApiKey: revokeApiKeyMock,
  deleteApiKey: deleteApiKeyMock,
}));

import {
  listApiKeysAction,
  createApiKeyAction,
  revokeApiKeyAction,
  deleteApiKeyAction,
} from "@/app/(dashboard)/settings/api-keys/actions";

beforeEach(() => {
  vi.clearAllMocks();
  // withToken forwards a resolved token to its callback for the happy path.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => ({
    ok: true,
    data: await fn("tok"),
  }));
});
afterEach(() => vi.resetAllMocks());

describe("api-keys server actions — thin forwarders", () => {
  it("listApiKeysAction threads the token first and forwards params, returning the envelope", async () => {
    const data = { items: [], total: 0, page: 2, page_size: 25 };
    listApiKeysMock.mockResolvedValueOnce(data);
    const params = { page: 2, page_size: 25, sort: "-created_at" as const };
    const r = await listApiKeysAction(params);
    expect(listApiKeysMock).toHaveBeenCalledWith("tok", params);
    expect(r).toEqual({ ok: true, data });
  });

  it("createApiKeyAction threads the token first and forwards the body, returning the envelope", async () => {
    const data = { id: "k1", key_name: "ci", key: "agt_tsecret", created_at: 1700000000000 };
    createApiKeyMock.mockResolvedValueOnce(data);
    const body = { key_name: "ci", description: "build bot" };
    const r = await createApiKeyAction(body);
    expect(createApiKeyMock).toHaveBeenCalledWith("tok", body);
    expect(r).toEqual({ ok: true, data });
  });

  it("revokeApiKeyAction threads the token first and forwards the id, returning the envelope", async () => {
    const data = { id: "k1", active: false, revoked_at: 1700000000000 };
    revokeApiKeyMock.mockResolvedValueOnce(data);
    const r = await revokeApiKeyAction("k1");
    expect(revokeApiKeyMock).toHaveBeenCalledWith("tok", "k1");
    expect(r).toEqual({ ok: true, data });
  });

  it("deleteApiKeyAction threads the token first and forwards the id, returning the envelope", async () => {
    deleteApiKeyMock.mockResolvedValueOnce(undefined);
    const r = await deleteApiKeyAction("k1");
    expect(deleteApiKeyMock).toHaveBeenCalledWith("tok", "k1");
    expect(r).toEqual({ ok: true, data: undefined });
  });
});
