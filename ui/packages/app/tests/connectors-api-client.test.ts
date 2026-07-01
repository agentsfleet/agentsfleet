import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Pure API-client tests for lib/api/connectors — exercise the real client against
// a stubbed fetch (no module mocks), so the provider path builder and both
// provider-parameterised calls are covered end to end. The action wrapper and the
// connect UI are proven in connector-actions / integrations-connectors; here we
// pin the wire: method, bearer, and the exact /v1/workspaces/{id}/connectors/{provider}
// route for each provider.
const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => {
  fetchMock.mockReset();
});

describe("lib/api/connectors", () => {
  it("getConnector('github', …) sends GET with bearer to the connector status path", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ status: "connected" }),
    });
    const mod = await import("../lib/api/connectors");
    const res = await mod.getConnector("github", "ws_1", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/connectors/github"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tkn" }),
      }),
    );
    expect(res.status).toBe(mod.CONNECTOR_STATUS.connected);
  });

  it("startConnect('github', …) POSTs to the /connect sub-path and returns the install URL", async () => {
    const install_url = "https://github.com/apps/agentsfleet/installations/new?state=signed";
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ install_url }),
    });
    const mod = await import("../lib/api/connectors");
    const res = await mod.startConnect("github", "ws_1", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/connectors/github/connect"),
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({ Authorization: "Bearer tkn" }),
      }),
    );
    expect(res.install_url).toBe(install_url);
  });

  it("getConnector('slack', …) sends GET with bearer and surfaces status + team", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ status: "connected", team: "Acme HQ" }),
    });
    const mod = await import("../lib/api/connectors");
    const res = await mod.getConnector("slack", "ws_1", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/connectors/slack"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tkn" }),
      }),
    );
    expect(res.status).toBe(mod.CONNECTOR_STATUS.connected);
    expect(res.team).toBe("Acme HQ");
  });

  it("startConnect('slack', …) POSTs to the /connect sub-path and returns the authorize URL", async () => {
    const install_url = "https://slack.com/oauth/v2/authorize?state=signed";
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ install_url }),
    });
    const mod = await import("../lib/api/connectors");
    const res = await mod.startConnect("slack", "ws_1", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/connectors/slack/connect"),
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({ Authorization: "Bearer tkn" }),
      }),
    );
    expect(res.install_url).toBe(install_url);
  });
});
