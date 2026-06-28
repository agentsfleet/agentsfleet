import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// These provider server actions are thin forwarders: each export wraps
// withToken((t) => apiFn(args, t)). We mock the token wrapper and the API
// client so the only thing under test is the forwarding shape — argument
// order and the wrapped {ok,data} envelope (the real round-trip lives in the
// api client suite + the backend integration suite).

// vi.mock is hoisted above the static actions import, so the mock fns must be
// created via vi.hoisted() to exist when the factories run.
const { withTokenMock, setTenantProviderSelfManagedMock, resetTenantProviderMock, rotateCredentialMock } = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  setTenantProviderSelfManagedMock: vi.fn(),
  resetTenantProviderMock: vi.fn(),
  rotateCredentialMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/tenant_provider", () => ({
  setTenantProviderSelfManaged: setTenantProviderSelfManagedMock,
  resetTenantProvider: resetTenantProviderMock,
}));
vi.mock("@/lib/api/credentials", () => ({ rotateCredential: rotateCredentialMock }));

import {
  setProviderSelfManagedAction,
  resetProviderAction,
  rotateCredentialAction,
} from "@/app/(dashboard)/settings/models/actions";

beforeEach(() => {
  vi.clearAllMocks();
  // withToken forwards a resolved token to its callback for the happy path.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => ({
    ok: true,
    data: await fn("tok"),
  }));
});
afterEach(() => vi.resetAllMocks());

describe("provider server actions — thin forwarders", () => {
  it("setProviderSelfManagedAction forwards the body then token through withToken to the client", async () => {
    const provider = { mode: "self_managed", model: "claude-opus" };
    setTenantProviderSelfManagedMock.mockResolvedValueOnce(provider);
    const body = { credential_ref: "vault://anthropic", model: "claude-opus" };
    const r = await setProviderSelfManagedAction(body);
    expect(r).toEqual({ ok: true, data: provider });
    expect(setTenantProviderSelfManagedMock).toHaveBeenCalledWith(body, "tok");
    expect(resetTenantProviderMock).not.toHaveBeenCalled();
  });

  it("setProviderSelfManagedAction forwards a body with only credential_ref (model omitted)", async () => {
    setTenantProviderSelfManagedMock.mockResolvedValueOnce({ mode: "self_managed" });
    const body = { credential_ref: "vault://openai" };
    const r = await setProviderSelfManagedAction(body);
    expect(r).toEqual({ ok: true, data: { mode: "self_managed" } });
    expect(setTenantProviderSelfManagedMock).toHaveBeenCalledWith(body, "tok");
  });

  it("resetProviderAction forwards only the token through withToken to the client", async () => {
    const provider = { mode: "managed" };
    resetTenantProviderMock.mockResolvedValueOnce(provider);
    const r = await resetProviderAction();
    expect(r).toEqual({ ok: true, data: provider });
    expect(resetTenantProviderMock).toHaveBeenCalledWith("tok");
    expect(setTenantProviderSelfManagedMock).not.toHaveBeenCalled();
  });

  it("both actions route through withToken exactly once", async () => {
    setTenantProviderSelfManagedMock.mockResolvedValueOnce({});
    resetTenantProviderMock.mockResolvedValueOnce({});
    await setProviderSelfManagedAction({ credential_ref: "vault://x" });
    await resetProviderAction();
    expect(withTokenMock).toHaveBeenCalledTimes(2);
  });

  it("rotateCredentialAction forwards (workspaceId, name, apiKey) then token to the client", async () => {
    rotateCredentialMock.mockResolvedValueOnce({ name: "anthropic-prod" });
    const r = await rotateCredentialAction("ws_1", "anthropic-prod", "sk-ant-rotated");
    expect(r).toEqual({ ok: true, data: { name: "anthropic-prod" } });
    expect(rotateCredentialMock).toHaveBeenCalledWith("ws_1", "anthropic-prod", "sk-ant-rotated", "tok");
    expect(setTenantProviderSelfManagedMock).not.toHaveBeenCalled();
    expect(resetTenantProviderMock).not.toHaveBeenCalled();
  });
});
