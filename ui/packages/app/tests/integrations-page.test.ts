import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// Server-component test for the standalone /integrations page. The data layer
// and the interactive connectors client are mocked at module boundaries; this
// asserts composition (title + connectors wiring) and the fail-closed connector
// degradation that used to live on the Credentials page.

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const auth = vi.fn();
vi.mock("next/navigation", () => ({ redirect }));
vi.mock("@clerk/nextjs/server", () => ({ auth }));
vi.mock("@/lib/workspace", () => ({
  withWorkspaceScope: vi.fn(),
  orFallback:
    <T,>(fallback: T) =>
    (): T =>
      fallback,
}));
vi.mock("@/lib/api/credentials", () => ({ listCredentials: vi.fn() }));
vi.mock("@/lib/api/connectors", () => ({
  getGithubConnector: vi.fn(),
  CONNECTOR_STATUS: {
    connected: "connected",
    reconnectRequired: "reconnect_required",
    notConnected: "not_connected",
  },
}));
vi.mock("@/app/(dashboard)/integrations/components/IntegrationsConnectors", () => ({
  default: ({
    workspaceId,
    githubStatus,
    credentialNames,
  }: {
    workspaceId: string;
    githubStatus: string;
    credentialNames: readonly string[];
  }) =>
    React.createElement(
      "div",
      { "data-integrations-connectors": workspaceId, "data-github-status": githubStatus },
      credentialNames.join(","),
    ),
}));
vi.mock("lucide-react", () => ({
  LinkIcon: (p: Record<string, unknown>) => React.createElement("svg", { ...p, "data-icon": "LinkIcon" }),
}));

import { withWorkspaceScope } from "@/lib/workspace";
import { listCredentials } from "@/lib/api/credentials";
import { getGithubConnector, CONNECTOR_STATUS } from "@/lib/api/connectors";

beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_123") });
  vi.mocked(withWorkspaceScope).mockImplementation(
    async (_token: string, fn: (workspaceId: string) => Promise<unknown>) => fn("ws_1"),
  );
});
afterEach(() => vi.clearAllMocks());

describe("Integrations page", () => {
  it("renders the connectors wired with the github status and stored-secret names", async () => {
    vi.mocked(listCredentials).mockResolvedValue({
      credentials: [{ name: "SLACK_BOT_TOKEN", created_at: 0 }],
    });
    vi.mocked(getGithubConnector).mockResolvedValue({ status: CONNECTOR_STATUS.connected });

    const { default: Page } = await import("../app/(dashboard)/integrations/page");
    const markup = renderToStaticMarkup(await Page());

    expect(markup).toContain(">Integrations<");
    expect(markup).toContain('data-integrations-connectors="ws_1"');
    expect(markup).toContain('data-github-status="connected"');
    expect(markup).toContain("SLACK_BOT_TOKEN");
  });

  it("degrades the GitHub connector to not-connected when the status read errors", async () => {
    vi.mocked(listCredentials).mockResolvedValue({ credentials: [] });
    vi.mocked(getGithubConnector).mockRejectedValue(new Error("connector endpoint down"));

    const { default: Page } = await import("../app/(dashboard)/integrations/page");
    const markup = renderToStaticMarkup(await Page());

    // Never fabricate a connected pill: a failed status read reads as not-connected.
    expect(markup).toContain('data-github-status="not_connected"');
  });

  it("renders the no-workspace empty state under the Integrations title", async () => {
    vi.mocked(withWorkspaceScope).mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/integrations/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain(">Integrations<");
    expect(markup).toContain("No workspace yet");
  });

  it("redirects to /sign-in when unauthenticated", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/integrations/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });
});
