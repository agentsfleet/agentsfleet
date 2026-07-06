import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// Server-component test for the /integrations page. The data layer and the
// interactive connectors client are mocked at module boundaries; this asserts
// composition (title + catalog/status wiring) and the fail-closed degradation:
// a failed catalog read → empty grid, a failed status read → not-connected —
// never a fabricated connected state.

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const auth = vi.fn();
vi.mock("next/navigation", () => ({ redirect }));
vi.mock("@clerk/nextjs/server", () => ({ auth }));
vi.mock("@/lib/api/connectors", () => ({
  getConnector: vi.fn(),
  getConnectorCatalog: vi.fn(),
  CONNECTOR_STATUS: {
    connected: "connected",
    reconnectRequired: "reconnect_required",
    notConnected: "not_connected",
  },
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/integrations/components/IntegrationsConnectors", () => ({
  default: ({
    workspaceId,
    catalog,
    githubStatus,
    slackStatus,
    slackTeam,
  }: {
    workspaceId: string;
    catalog: ReadonlyArray<{ id: string }>;
    githubStatus: string;
    slackStatus: string;
    slackTeam: string | null;
  }) =>
    React.createElement("div", {
      "data-integrations-connectors": workspaceId,
      "data-github-status": githubStatus,
      "data-slack-status": slackStatus,
      "data-slack-team": slackTeam ?? "",
      "data-catalog": catalog.map((e) => e.id).join(","),
    }),
}));
vi.mock("lucide-react", () => ({
  LinkIcon: (p: Record<string, unknown>) => React.createElement("svg", { ...p, "data-icon": "LinkIcon" }),
}));

import { getConnector, getConnectorCatalog, CONNECTOR_STATUS } from "@/lib/api/connectors";

// The workspace id now comes from the route param; every page invocation passes
// it explicitly and the page forwards it to its data clients.
const WORKSPACE_ID = "ws_1";
function renderPage(Page: (args: { params: Promise<{ workspaceId: string }> }) => Promise<React.ReactElement>) {
  return Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID }) });
}

// The page reads both bespoke-status connectors through one getConnector(provider,
// …); dispatch the stub on the provider argument so each row's status/team (and
// its fail-closed rejection path) is set independently.
type StubResult = { status: string; team?: string | null } | Error;
function stubConnectors(github: StubResult, slack: StubResult) {
  vi.mocked(getConnector).mockImplementation(((provider: string) => {
    const r = provider === "github" ? github : slack;
    return r instanceof Error ? Promise.reject(r) : Promise.resolve(r);
  }) as unknown as typeof getConnector);
}

beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_123") });
  // Defaults: a two-entry catalog, both bespoke connectors not connected.
  vi.mocked(getConnectorCatalog).mockResolvedValue([
    { id: "github", archetype: "app_install", display_name: "GitHub", configured: true, connected: false },
    { id: "slack", archetype: "oauth2", display_name: "Slack", configured: true, connected: false },
  ]);
  stubConnectors(
    { status: CONNECTOR_STATUS.notConnected },
    { status: CONNECTOR_STATUS.notConnected, team: null },
  );
});
afterEach(() => vi.clearAllMocks());

describe("Integrations page", () => {
  it("wires the registry-driven catalog and the github status into the connectors", async () => {
    vi.mocked(getConnectorCatalog).mockResolvedValue([
      { id: "github", archetype: "app_install", display_name: "GitHub", configured: true, connected: true },
      { id: "zoho", archetype: "oauth2", display_name: "Zoho Desk", configured: true, connected: false },
    ]);
    stubConnectors(
      { status: CONNECTOR_STATUS.connected },
      { status: CONNECTOR_STATUS.notConnected, team: null },
    );

    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/integrations/page");
    const markup = renderToStaticMarkup(await renderPage(Page));

    expect(markup).toContain(">Integrations<");
    expect(markup).toContain('data-integrations-connectors="ws_1"');
    expect(markup).toContain('data-catalog="github,zoho"');
    expect(markup).toContain('data-github-status="connected"');
  });

  it("wires the Slack connector status + team through to the connectors component", async () => {
    stubConnectors(
      { status: CONNECTOR_STATUS.notConnected },
      { status: CONNECTOR_STATUS.connected, team: "Acme Corp" },
    );

    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/integrations/page");
    const markup = renderToStaticMarkup(await renderPage(Page));

    expect(markup).toContain('data-slack-status="connected"');
    expect(markup).toContain('data-slack-team="Acme Corp"');
  });

  it("degrades to an empty catalog when the catalog read errors (never fabricates cards)", async () => {
    vi.mocked(getConnectorCatalog).mockRejectedValue(new Error("catalog endpoint down"));

    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/integrations/page");
    const markup = renderToStaticMarkup(await renderPage(Page));

    expect(markup).toContain('data-catalog=""');
  });

  it("degrades the Slack connector to not-connected when the status read errors", async () => {
    stubConnectors(
      { status: CONNECTOR_STATUS.notConnected },
      new Error("connector endpoint down"),
    );

    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/integrations/page");
    const markup = renderToStaticMarkup(await renderPage(Page));

    expect(markup).toContain('data-slack-status="not_connected"');
  });

  it("degrades the GitHub connector to not-connected when the status read errors", async () => {
    stubConnectors(
      new Error("connector endpoint down"),
      { status: CONNECTOR_STATUS.notConnected, team: null },
    );

    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/integrations/page");
    const markup = renderToStaticMarkup(await renderPage(Page));

    // Never fabricate a connected pill: a failed status read reads as not-connected.
    expect(markup).toContain('data-github-status="not_connected"');
  });

  it("redirects to /sign-in when unauthenticated", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/integrations/page");
    await expect(renderPage(Page)).rejects.toThrow("redirect:/sign-in");
  });
});
