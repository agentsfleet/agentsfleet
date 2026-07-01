import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// connector-actions is a thin server-action wrapper: it forwards the workspace
// id through withToken to the connectors API client and returns the install URL
// the browser redirects to. No token or secret ever passes through the action —
// the real connect/callback security boundary is the backend, proven by its own
// suite. Mock both module boundaries so only the action's delegation is tested.
const { withTokenMock, startGithubConnectMock, startSlackConnectMock } = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  startGithubConnectMock: vi.fn(),
  startSlackConnectMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/connectors", () => ({
  startGithubConnect: startGithubConnectMock,
  startSlackConnect: startSlackConnectMock,
}));

import {
  startGithubConnectAction,
  startSlackConnectAction,
} from "@/app/(dashboard)/integrations/connector-actions";

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

describe("credentials connector server actions", () => {
  it("startGithubConnectAction forwards the workspace id through withToken to startGithubConnect", async () => {
    const install = { install_url: "https://github.com/apps/agentsfleet/installations/new?state=signed" };
    startGithubConnectMock.mockResolvedValue(install);

    const result = await startGithubConnectAction("ws_1");

    expect(result).toEqual({ ok: true, data: install });
    expect(withTokenMock).toHaveBeenCalledTimes(1);
    // The token is injected by withToken, not the caller — the action only knows
    // the workspace id.
    expect(startGithubConnectMock).toHaveBeenCalledWith("ws_1", "tok");
  });

  it("surfaces a connectors-client failure as { ok: false } (degraded closed, no throw)", async () => {
    startGithubConnectMock.mockRejectedValue(new Error("UZ-CONN-001"));

    const result = await startGithubConnectAction("ws_1");

    expect(result).toEqual({ ok: false, error: "UZ-CONN-001" });
  });

  it("startSlackConnectAction forwards the workspace id through withToken to startSlackConnect", async () => {
    const install = { install_url: "https://slack.com/oauth/v2/authorize?state=signed" };
    startSlackConnectMock.mockResolvedValue(install);

    const result = await startSlackConnectAction("ws_1");

    expect(result).toEqual({ ok: true, data: install });
    expect(withTokenMock).toHaveBeenCalledTimes(1);
    expect(startSlackConnectMock).toHaveBeenCalledWith("ws_1", "tok");
  });

  it("surfaces a Slack connect failure as { ok: false } (degraded closed, no throw)", async () => {
    startSlackConnectMock.mockRejectedValue(new Error("UZ-SLK-021"));

    const result = await startSlackConnectAction("ws_1");

    expect(result).toEqual({ ok: false, error: "UZ-SLK-021" });
  });
});
