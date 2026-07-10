import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

// The connectors card grid is registry-driven: it renders whatever the catalog
// returns (`ConnectorCatalogEntry[]`), so these tests feed a catalog and assert
// the cards mirror it — no provider list is baked into the component. All
// connectors are OAuth/app_install (api-key providers are custom secrets, not
// connectors). The connect action is mocked; the real security boundary is the
// backend, proven by its own suite.
const { startConnectActionMock } = vi.hoisted(() => ({ startConnectActionMock: vi.fn() }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/integrations/connector-actions", () => ({
  startConnectAction: startConnectActionMock,
}));
vi.mock("lucide-react", () => {
  const make = (name: string) => (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": name });
  return {
    BriefcaseIcon: make("BriefcaseIcon"),
    GitPullRequestIcon: make("GitPullRequestIcon"),
    Grid2x2Icon: make("Grid2x2Icon"),
    HashIcon: make("HashIcon"),
    PlugIcon: make("PlugIcon"),
    TicketIcon: make("TicketIcon"),
  };
});

import IntegrationsConnectors from "@/app/(dashboard)/w/[workspaceId]/integrations/components/IntegrationsConnectors";
import {
  CONNECTOR_STATUS,
  CONNECTOR_NOT_CONFIGURED_DOCS_URI,
  type ConnectorCatalogEntry,
  type ConnectorStatus,
} from "@/lib/api/connectors";

const WS = "ws_test";

function entry(
  over: Partial<ConnectorCatalogEntry> & Pick<ConnectorCatalogEntry, "id" | "archetype">,
): ConnectorCatalogEntry {
  return { display_name: over.id, configured: true, connected: false, ...over };
}

const GITHUB = entry({ id: "github", archetype: "app_install", display_name: "GitHub" });
const SLACK = entry({ id: "slack", archetype: "oauth2", display_name: "Slack" });
const ZOHO = entry({ id: "zoho", archetype: "oauth2", display_name: "Zoho Desk" });

function renderConnectors(
  catalog: ConnectorCatalogEntry[],
  props: Partial<{
    githubStatus: ConnectorStatus;
    slackStatus: ConnectorStatus;
    slackTeam: string | null;
    catalogError: { code: string; status: number | null } | null;
  }> = {},
) {
  return render(
    React.createElement(IntegrationsConnectors, {
      workspaceId: WS,
      catalog,
      // Passed through as-is: omitting it (undefined) exercises the
      // component's own `catalogError = null` default.
      catalogError: props.catalogError,
      githubStatus: props.githubStatus ?? CONNECTOR_STATUS.notConnected,
      slackStatus: props.slackStatus,
      slackTeam: props.slackTeam,
    }),
  );
}

afterEach(() => {
  cleanup();
  startConnectActionMock.mockReset();
});

describe("IntegrationsConnectors (test_ui_connectors_cards_from_catalog)", () => {
  it("renders one card per catalog entry — registry-driven, no hard-coded list", () => {
    renderConnectors([GITHUB, SLACK, ZOHO]);
    for (const e of [GITHUB, SLACK, ZOHO]) {
      expect(screen.getByTestId(`integration-${e.id}`).textContent).toContain(e.display_name);
    }
    expect(screen.queryByTestId("integration-jira")).toBeNull();
  });

  it("shows a load-failure empty state when the catalog is empty (degraded closed)", () => {
    // The catalog is registry-driven and never legitimately empty — an empty
    // list always means the fetch failed and degraded via the page's
    // catch-all, so the copy must say so plainly (not an ambiguous "no
    // connectors" framing a real empty state would use).
    renderConnectors([]);
    expect(screen.getByTestId("connectors-empty")).toBeTruthy();
    expect(screen.getByText(/couldn't load connectors/i)).toBeTruthy();
    expect(screen.queryByTestId("integration-github")).toBeNull();
  });

  it("surfaces the fetch error code + status in the empty state when captured (test_connector_fetch_error_logged)", () => {
    // The page captures the ApiError instead of swallowing it; the code/status
    // is what makes the failure diagnosable (console logging is lint-banned).
    renderConnectors([], { catalogError: { code: "UZ-INTERNAL-003", status: 500 } });
    const empty = screen.getByTestId("connectors-empty");
    expect(empty.textContent).toContain("UZ-INTERNAL-003");
    expect(empty.textContent).toContain("500");
  });

  it("omits the status segment when the failure carried no HTTP status (non-ApiError)", () => {
    // A thrown non-ApiError has no HTTP status; the detail must render the
    // code alone — "(UZ-UNKNOWN)" — never a fabricated "· 0".
    renderConnectors([], { catalogError: { code: "UZ-UNKNOWN", status: null } });
    const empty = screen.getByTestId("connectors-empty");
    expect(empty.textContent).toContain("(UZ-UNKNOWN)");
    expect(empty.textContent).not.toContain("·");
  });

  it("renders a card for a provider with no bespoke icon (generic plug fallback)", () => {
    renderConnectors([entry({ id: "webhooks_custom", archetype: "oauth2", display_name: "Custom" })]);
    expect(screen.getByTestId("integration-webhooks_custom").textContent).toContain("Custom");
  });

  it("renders an OAuth connector not-connected with a Connect button and no paste", () => {
    renderConnectors([GITHUB], { githubStatus: CONNECTOR_STATUS.notConnected });
    const github = screen.getByTestId("integration-github");
    expect(github.textContent).toContain("Not connected");
    expect(github.textContent).not.toContain("GITHUB_TOKEN");
    expect(screen.getByRole("button", { name: /connect github/i })).toBeTruthy();
    // Not-connected is a neutral fact, not a fault (FINDING-007) — amber stays
    // reserved for reconnect-required, which genuinely needs attention.
    expect(within(github).getByText("Not connected").getAttribute("data-variant")).toBe("neutral");
  });

  it("uses the bespoke GitHub status override (connected → no Connect button)", () => {
    renderConnectors([GITHUB], { githubStatus: CONNECTOR_STATUS.connected });
    expect(screen.getByTestId("integration-github").textContent).toContain("Connected");
    expect(screen.queryByRole("button", { name: /connect github/i })).toBeNull();
  });

  it("offers Reconnect when the install was revoked (reconnect_required)", () => {
    renderConnectors([GITHUB], { githubStatus: CONNECTOR_STATUS.reconnectRequired });
    expect(screen.getByRole("button", { name: /reconnect github/i })).toBeTruthy();
  });

  it("derives status from the catalog `connected` flag when there's no override (Zoho)", () => {
    renderConnectors([entry({ id: "zoho", archetype: "oauth2", display_name: "Zoho Desk", connected: true })]);
    expect(screen.getByTestId("integration-zoho").textContent).toContain("Connected");
    expect(screen.queryByRole("button", { name: /connect zoho/i })).toBeNull();
  });

  it("shows the Slack team identity from the override when connected", () => {
    renderConnectors([SLACK], { slackStatus: CONNECTOR_STATUS.connected, slackTeam: "Acme Corp" });
    expect(screen.getByTestId("integration-slack").textContent).toContain("Connected: Acme Corp");
  });

  it("calls the connect action with the catalog id and redirects on success", async () => {
    const install_url = "https://github.com/apps/agentsfleet/installations/new?state=signed";
    startConnectActionMock.mockResolvedValue({ ok: true, data: { install_url } });
    const original = window.location;
    let assigned = "";
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { ...original, set href(v: string) { assigned = v; }, get href() { return assigned; } },
    });
    try {
      renderConnectors([GITHUB]);
      fireEvent.click(screen.getByRole("button", { name: /connect github/i }));
      await waitFor(() => expect(startConnectActionMock).toHaveBeenCalledWith("github", WS));
      await waitFor(() => expect(assigned).toBe(install_url));
    } finally {
      Object.defineProperty(window, "location", { configurable: true, value: original });
    }
  });

  it("surfaces a connect failure as an inline error, no redirect", async () => {
    startConnectActionMock.mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-CONN-001" });
    renderConnectors([GITHUB]);
    fireEvent.click(screen.getByRole("button", { name: /connect github/i }));
    await waitFor(() =>
      expect(within(screen.getByTestId("integration-github")).getByRole("alert")).toBeTruthy(),
    );
  });

  it("renders an unconfigured OAuth connector with the UZ-CONN-001 docs link, no Connect", () => {
    renderConnectors([entry({ id: "jira", archetype: "oauth2", display_name: "Jira", configured: false })]);
    const jira = screen.getByTestId("integration-jira");
    expect(jira.textContent).toContain("Admin setup required");
    expect(jira.textContent).toContain("A platform admin needs to enable this connector.");
    expect(screen.queryByRole("button", { name: /connect jira/i })).toBeNull();
    const link = within(jira).getByRole("link", { name: /setup steps/i });
    expect(link.getAttribute("href")).toBe(CONNECTOR_NOT_CONFIGURED_DOCS_URI);
  });
});
