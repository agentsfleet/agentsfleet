import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// connector-actions are thin server-action wrappers: they re-validate the provider
// id's SHAPE (untrusted wire argument), then forward through withToken to the
// connectors API client. No token or secret passes through them; the real
// connect/probe security boundary is the backend. The provider is NOT checked
// against an allowlist — that would duplicate the registry — so a newly registered
// provider connects with no change here. Mock both module boundaries.
const { withTokenMock, startConnectMock, submitApiKeyConnectMock } = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  startConnectMock: vi.fn(),
  submitApiKeyConnectMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/connectors", () => ({
  startConnect: startConnectMock,
  submitApiKeyConnect: submitApiKeyConnectMock,
}));

import { startConnectAction, submitApiKeyConnectAction } from "@/app/(dashboard)/integrations/connector-actions";

beforeEach(() => {
  vi.clearAllMocks();
  // Faithful to the real withToken: forward a resolved token; normalise a thrown
  // error into { ok: false } rather than letting it escape the action.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => {
    try {
      return { ok: true, data: await fn("tok") };
    } catch (e) {
      return { ok: false, error: e instanceof Error ? e.message : String(e) };
    }
  });
});
afterEach(() => vi.resetAllMocks());

describe("startConnectAction (OAuth connect)", () => {
  it("forwards the provider + workspace through withToken to startConnect", async () => {
    const install = { install_url: "https://github.com/apps/agentsfleet/installations/new?state=signed" };
    startConnectMock.mockResolvedValue(install);

    const result = await startConnectAction("github", "ws_1");

    expect(result).toEqual({ ok: true, data: install });
    expect(withTokenMock).toHaveBeenCalledTimes(1);
    // The token is injected by withToken, not the caller.
    expect(startConnectMock).toHaveBeenCalledWith("github", "ws_1", "tok");
  });

  it("forwards a provider that is NOT github/slack — registry-driven, no allowlist", async () => {
    const authorize = { install_url: "https://accounts.zoho.com/oauth/v2/auth?state=signed" };
    startConnectMock.mockResolvedValue(authorize);

    const result = await startConnectAction("zoho", "ws_1");

    expect(result).toEqual({ ok: true, data: authorize });
    expect(startConnectMock).toHaveBeenCalledWith("zoho", "ws_1", "tok");
  });

  it("surfaces a connectors-client failure as { ok: false } (degraded closed, no throw)", async () => {
    startConnectMock.mockRejectedValue(new Error("UZ-CONN-001"));

    const result = await startConnectAction("zoho", "ws_1");

    expect(result).toEqual({ ok: false, error: "UZ-CONN-001" });
  });

  it("rejects a path-tampered provider before any token or API work", async () => {
    const result = await startConnectAction("evil/../provider", "ws_1");

    expect(result).toEqual({ ok: false, error: "Unknown connector provider" });
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(startConnectMock).not.toHaveBeenCalled();
  });

  it("rejects a malformed provider id (uppercase / spaces are not a valid slug)", async () => {
    for (const bad of ["Datadog", "data dog", "", "9lives"]) {
      const result = await startConnectAction(bad, "ws_1");
      expect(result).toEqual({ ok: false, error: "Unknown connector provider" });
    }
    expect(startConnectMock).not.toHaveBeenCalled();
  });
});

describe("submitApiKeyConnectAction (api_key connect)", () => {
  it("forwards the provider + fields through withToken to submitApiKeyConnect", async () => {
    submitApiKeyConnectMock.mockResolvedValue({ status: "connected" });
    const fields = { api_key: "dd-key", app_key: "dd-app", site: "datadoghq.com" };

    const result = await submitApiKeyConnectAction("datadog", "ws_1", fields);

    expect(result).toEqual({ ok: true, data: { status: "connected" } });
    expect(submitApiKeyConnectMock).toHaveBeenCalledWith("datadog", "ws_1", fields, "tok");
  });

  it("surfaces a probe rejection as { ok: false } with the error message", async () => {
    submitApiKeyConnectMock.mockRejectedValue(new Error("Connector probe rejected the supplied credentials"));

    const result = await submitApiKeyConnectAction("datadog", "ws_1", { api_key: "bad", app_key: "bad", site: "x" });

    expect(result).toEqual({ ok: false, error: "Connector probe rejected the supplied credentials" });
  });

  it("rejects a path-tampered provider before any token or probe", async () => {
    const result = await submitApiKeyConnectAction("evil/../x", "ws_1", { org_token: "t" });

    expect(result).toEqual({ ok: false, error: "Unknown connector provider" });
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(submitApiKeyConnectMock).not.toHaveBeenCalled();
  });
});
