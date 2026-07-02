import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// connector-actions is a thin server-action wrapper: it forwards the provider +
// workspace id through withToken to the connectors API client and returns the
// authorize/install URL the browser redirects to. No token or secret ever passes
// through the action — the real connect/callback security boundary is the backend,
// proven by its own suite. Mock both module boundaries so only the action's
// provider-parameterised delegation is tested.
const { withTokenMock, startConnectMock } = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  startConnectMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/connectors", () => ({
  startConnect: startConnectMock,
  CONNECTOR_PROVIDERS: { github: "github", slack: "slack" },
}));

import { startConnectAction } from "@/app/(dashboard)/integrations/connector-actions";

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

describe("connector connect server action", () => {
  it("forwards the GitHub provider + workspace through withToken to startConnect", async () => {
    const install = { install_url: "https://github.com/apps/agentsfleet/installations/new?state=signed" };
    startConnectMock.mockResolvedValue(install);

    const result = await startConnectAction("github", "ws_1");

    expect(result).toEqual({ ok: true, data: install });
    expect(withTokenMock).toHaveBeenCalledTimes(1);
    // The token is injected by withToken, not the caller — the action only knows
    // the provider + workspace id.
    expect(startConnectMock).toHaveBeenCalledWith("github", "ws_1", "tok");
  });

  it("surfaces a connectors-client failure as { ok: false } (degraded closed, no throw)", async () => {
    startConnectMock.mockRejectedValue(new Error("UZ-CONN-001"));

    const result = await startConnectAction("github", "ws_1");

    expect(result).toEqual({ ok: false, error: "UZ-CONN-001" });
  });

  it("forwards the Slack provider + workspace through withToken to startConnect", async () => {
    const install = { install_url: "https://slack.com/oauth/v2/authorize?state=signed" };
    startConnectMock.mockResolvedValue(install);

    const result = await startConnectAction("slack", "ws_1");

    expect(result).toEqual({ ok: true, data: install });
    expect(withTokenMock).toHaveBeenCalledTimes(1);
    expect(startConnectMock).toHaveBeenCalledWith("slack", "ws_1", "tok");
  });

  it("surfaces a Slack connect failure as { ok: false } (degraded closed, no throw)", async () => {
    startConnectMock.mockRejectedValue(new Error("UZ-SLK-021"));

    const result = await startConnectAction("slack", "ws_1");

    expect(result).toEqual({ ok: false, error: "UZ-SLK-021" });
  });

  it("rejects an unknown provider before any token or API work (untrusted wire argument)", async () => {
    const result = await startConnectAction(
      "evil/../provider" as unknown as Parameters<typeof startConnectAction>[0],
      "ws_1",
    );

    expect(result).toEqual({ ok: false, error: "Unknown connector provider" });
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(startConnectMock).not.toHaveBeenCalled();
  });
});
