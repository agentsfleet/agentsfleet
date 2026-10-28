import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// connector-actions are thin server-action wrappers: they re-validate the provider
// id's SHAPE (untrusted wire argument), then forward through withToken to the
// connectors API client. No token or secret passes through them; the real
// connect/probe security boundary is the backend. The provider is NOT checked
// against an allowlist — that would duplicate the registry — so a newly registered
// provider connects with no change here. Mock both module boundaries.
const { disconnectConnectorMock, withTokenMock, startConnectMock, cookieSet } = vi.hoisted(() => ({
  disconnectConnectorMock: vi.fn(),
  withTokenMock: vi.fn(),
  startConnectMock: vi.fn(),
  cookieSet: vi.fn(),
}));

vi.mock("next/headers", () => ({ cookies: async () => ({ set: cookieSet }) }));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/connectors", () => ({
  disconnectConnector: disconnectConnectorMock,
  startConnect: startConnectMock,
}));

import {
  disconnectConnectorAction,
  startConnectAction,
} from "@/app/(dashboard)/w/[workspaceId]/integrations/connector-actions";

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

describe("disconnectConnectorAction", () => {
  it("forwards the provider and workspace through withToken", async () => {
    disconnectConnectorMock.mockResolvedValue(undefined);

    const result = await disconnectConnectorAction("github", "ws_1");

    expect(result).toEqual({ ok: true, data: undefined });
    expect(disconnectConnectorMock).toHaveBeenCalledWith("github", "ws_1", "tok");
  });

  it("rejects a path-tampered provider before token or API work", async () => {
    const result = await disconnectConnectorAction("evil/../provider", "ws_1");

    expect(result).toEqual({ ok: false, error: "Unknown connector provider" });
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(disconnectConnectorMock).not.toHaveBeenCalled();
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
    expect(cookieSet).toHaveBeenCalledWith("connector_return_github", "/w/ws_1/integrations",
      expect.objectContaining({ httpOnly: true, sameSite: "lax", path: "/", maxAge: 600 }));
  });

  it("forwards a provider that is NOT github/slack — registry-driven, no allowlist", async () => {
    const authorize = { install_url: "https://accounts.zoho.com/oauth/v2/auth?state=signed" };
    startConnectMock.mockResolvedValue(authorize);

    const result = await startConnectAction("zoho", "ws_1");

    expect(result).toEqual({ ok: true, data: authorize });
    expect(startConnectMock).toHaveBeenCalledWith("zoho", "ws_1", "tok");
  });

  it("never stores an unsafe workspace return hint", async () => {
    const install = { install_url: "https://jira.example/authorize" };
    startConnectMock.mockResolvedValue(install);

    expect(await startConnectAction("jira", "ws_1?code=private")).toEqual({ ok: true, data: install });
    expect(cookieSet).not.toHaveBeenCalled();
  });

  it("returns a retryable start failure before saving a return hint", async () => {
    startConnectMock.mockRejectedValue(new Error("Connection service unavailable"));

    expect(await startConnectAction("jira", "ws_1"))
      .toEqual({ ok: false, error: "Connection service unavailable" });
    expect(startConnectMock).toHaveBeenCalledOnce();
    expect(cookieSet).not.toHaveBeenCalled();
  });

  it("never reports a successful start when saving the return hint fails", async () => {
    startConnectMock.mockResolvedValue({ install_url: "https://jira.example/authorize" });
    cookieSet.mockImplementationOnce(() => { throw new Error("Return hint unavailable"); });

    expect(await startConnectAction("jira", "ws_1"))
      .toEqual({ ok: false, error: "Return hint unavailable" });
    expect(startConnectMock).toHaveBeenCalledOnce();
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
