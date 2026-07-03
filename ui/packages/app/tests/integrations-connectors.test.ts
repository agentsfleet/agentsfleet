import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

// The connectors card grid is registry-driven: it renders whatever the catalog
// returns (`ConnectorCatalogEntry[]`), so these tests feed a catalog and assert
// the cards/forms mirror it — no provider list is baked into the component. Both
// action-module boundaries are mocked; the real connect/probe security boundary is
// the backend, proven by its own suite.
const { startConnectActionMock, submitApiKeyConnectActionMock, refreshMock } = vi.hoisted(() => ({
  startConnectActionMock: vi.fn(),
  submitApiKeyConnectActionMock: vi.fn(),
  refreshMock: vi.fn(),
}));
vi.mock("@/app/(dashboard)/integrations/connector-actions", () => ({
  startConnectAction: startConnectActionMock,
  submitApiKeyConnectAction: submitApiKeyConnectActionMock,
}));
vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: refreshMock }) }));
vi.mock("lucide-react", () => {
  const make = (name: string) => (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": name });
  return {
    ActivityIcon: make("ActivityIcon"),
    BriefcaseIcon: make("BriefcaseIcon"),
    GitPullRequestIcon: make("GitPullRequestIcon"),
    Grid2x2Icon: make("Grid2x2Icon"),
    HashIcon: make("HashIcon"),
    LineChartIcon: make("LineChartIcon"),
    PlaneIcon: make("PlaneIcon"),
    PlugIcon: make("PlugIcon"),
    TicketIcon: make("TicketIcon"),
  };
});

import IntegrationsConnectors from "@/app/(dashboard)/integrations/components/IntegrationsConnectors";
import {
  CONNECTOR_STATUS,
  CONNECTOR_NOT_CONFIGURED_DOCS_URI,
  type ConnectorCatalogEntry,
  type ConnectorStatus,
} from "@/lib/api/connectors";

const WS = "ws_test";

// Catalog rows mirroring the backend wire contract; `fields` on api_key entries is
// exactly what the catalog serializes (name + secret).
const DATADOG_FIELDS = [
  { name: "api_key", secret: true },
  { name: "app_key", secret: true },
  { name: "site", secret: false },
] as const;

function entry(over: Partial<ConnectorCatalogEntry> & Pick<ConnectorCatalogEntry, "id" | "archetype">): ConnectorCatalogEntry {
  return {
    display_name: over.id,
    configured: true,
    connected: false,
    fields: [],
    ...over,
  };
}

const GITHUB = entry({ id: "github", archetype: "app_install", display_name: "GitHub" });
const SLACK = entry({ id: "slack", archetype: "oauth2", display_name: "Slack" });
const ZOHO = entry({ id: "zoho", archetype: "oauth2", display_name: "Zoho Desk" });
const DATADOG = entry({ id: "datadog", archetype: "api_key", display_name: "Datadog", fields: [...DATADOG_FIELDS] });

function renderConnectors(
  catalog: ConnectorCatalogEntry[],
  props: Partial<{ githubStatus: ConnectorStatus; slackStatus: ConnectorStatus; slackTeam: string | null }> = {},
) {
  return render(
    React.createElement(IntegrationsConnectors, {
      workspaceId: WS,
      catalog,
      githubStatus: props.githubStatus ?? CONNECTOR_STATUS.notConnected,
      slackStatus: props.slackStatus,
      slackTeam: props.slackTeam,
    }),
  );
}

afterEach(() => {
  cleanup();
  startConnectActionMock.mockReset();
  submitApiKeyConnectActionMock.mockReset();
  refreshMock.mockReset();
});

describe("IntegrationsConnectors (test_ui_connectors_cards_from_catalog)", () => {
  it("renders one card per catalog entry — registry-driven, no hard-coded list", () => {
    renderConnectors([GITHUB, SLACK, ZOHO, DATADOG]);
    for (const e of [GITHUB, SLACK, ZOHO, DATADOG]) {
      const card = screen.getByTestId(`integration-${e.id}`);
      expect(card.textContent).toContain(e.display_name);
    }
    // Nothing beyond the catalog: a provider absent from it has no card.
    expect(screen.queryByTestId("integration-jira")).toBeNull();
  });

  it("shows an empty state when the catalog is empty (degraded closed)", () => {
    renderConnectors([]);
    expect(screen.getByTestId("connectors-empty")).toBeTruthy();
    expect(screen.queryByTestId("integration-github")).toBeNull();
  });

  // ── OAuth / app_install cards ──────────────────────────────────────────────

  it("renders an OAuth connector not-connected with a Connect button and no paste", () => {
    renderConnectors([GITHUB], { githubStatus: CONNECTOR_STATUS.notConnected });
    const github = screen.getByTestId("integration-github");
    expect(github.textContent).toContain("Not connected");
    expect(github.textContent).not.toContain("GITHUB_TOKEN");
    expect(screen.getByRole("button", { name: /connect github/i })).toBeTruthy();
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

  it("derives status from the catalog `connected` flag for providers with no override (Zoho)", () => {
    renderConnectors([entry({ id: "zoho", archetype: "oauth2", display_name: "Zoho Desk", connected: true })]);
    const zoho = screen.getByTestId("integration-zoho");
    expect(zoho.textContent).toContain("Connected");
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
    expect(jira.textContent).toContain("Setup required");
    expect(screen.queryByRole("button", { name: /connect jira/i })).toBeNull();
    const link = within(jira).getByRole("link", { name: /setup guide/i });
    expect(link.getAttribute("href")).toBe(CONNECTOR_NOT_CONFIGURED_DOCS_URI);
  });

  // ── api_key cards + connect form ───────────────────────────────────────────

  it("reveals the api_key form with exactly the catalog-declared fields", () => {
    renderConnectors([DATADOG]);
    // No form until the operator opens it.
    expect(screen.queryByTestId("api-key-form-datadog")).toBeNull();
    fireEvent.click(within(screen.getByTestId("integration-datadog")).getByRole("button", { name: "Connect" }));
    const form = screen.getByTestId("api-key-form-datadog");
    // One input per declared field, secrets masked, plain coordinate as text.
    expect((within(form).getByLabelText("API key") as HTMLInputElement).type).toBe("password");
    expect((within(form).getByLabelText("App key") as HTMLInputElement).type).toBe("password");
    expect((within(form).getByLabelText("Site") as HTMLInputElement).type).toBe("text");
  });

  it("posts the declared fields and refreshes on a successful probe", async () => {
    submitApiKeyConnectActionMock.mockResolvedValue({ ok: true, data: { status: "connected" } });
    renderConnectors([DATADOG]);
    fireEvent.click(within(screen.getByTestId("integration-datadog")).getByRole("button", { name: "Connect" }));
    const form = screen.getByTestId("api-key-form-datadog");
    fireEvent.change(within(form).getByLabelText("API key"), { target: { value: "dd-key" } });
    fireEvent.change(within(form).getByLabelText("App key"), { target: { value: "dd-app" } });
    fireEvent.change(within(form).getByLabelText("Site"), { target: { value: "datadoghq.com" } });
    fireEvent.click(within(form).getByRole("button", { name: "Connect" }));
    await waitFor(() =>
      expect(submitApiKeyConnectActionMock).toHaveBeenCalledWith("datadog", WS, {
        api_key: "dd-key",
        app_key: "dd-app",
        site: "datadoghq.com",
      }),
    );
    await waitFor(() => expect(refreshMock).toHaveBeenCalled());
  });

  it("keeps the form open and shows the rejection when the probe fails (UZ-CONN-005)", async () => {
    submitApiKeyConnectActionMock.mockResolvedValue({
      ok: false,
      error: "Connector probe rejected the supplied credentials",
      errorCode: "UZ-CONN-005",
    });
    renderConnectors([DATADOG]);
    fireEvent.click(within(screen.getByTestId("integration-datadog")).getByRole("button", { name: "Connect" }));
    const form = screen.getByTestId("api-key-form-datadog");
    fireEvent.change(within(form).getByLabelText("API key"), { target: { value: "bad" } });
    fireEvent.change(within(form).getByLabelText("App key"), { target: { value: "bad" } });
    fireEvent.change(within(form).getByLabelText("Site"), { target: { value: "datadoghq.com" } });
    fireEvent.click(within(form).getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(within(form).getByRole("alert")).toBeTruthy());
    expect(refreshMock).not.toHaveBeenCalled();
    // Form stays open on failure so the operator can correct and retry.
    expect(screen.getByTestId("api-key-form-datadog")).toBeTruthy();
  });

  it("renders a connected api_key connector with no form and no Connect button", () => {
    renderConnectors([entry({ id: "fly", archetype: "api_key", display_name: "Fly.io", connected: true, fields: [{ name: "org_token", secret: true }] })]);
    const fly = screen.getByTestId("integration-fly");
    expect(fly.textContent).toContain("Connected");
    expect(within(fly).queryByRole("button", { name: "Connect" })).toBeNull();
  });

  it("renders a card for a provider with no bespoke icon (generic plug fallback)", () => {
    // An id absent from the icon map still renders — the icon falls back to a plug.
    renderConnectors([entry({ id: "webhooks_custom", archetype: "oauth2", display_name: "Custom" })]);
    expect(screen.getByTestId("integration-webhooks_custom").textContent).toContain("Custom");
  });

  it("closes the api_key form when the toggle is clicked again (Cancel)", () => {
    renderConnectors([DATADOG]);
    const row = screen.getByTestId("integration-datadog");
    fireEvent.click(within(row).getByRole("button", { name: "Connect" }));
    expect(screen.getByTestId("api-key-form-datadog")).toBeTruthy();
    // Open → the action button flips to Cancel; clicking it closes the form.
    fireEvent.click(within(row).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("api-key-form-datadog")).toBeNull();
  });

  it("Enter on an incomplete api_key form does nothing (submit guard)", () => {
    submitApiKeyConnectActionMock.mockResolvedValue({ ok: true, data: { status: "connected" } });
    renderConnectors([DATADOG]);
    fireEvent.click(within(screen.getByTestId("integration-datadog")).getByRole("button", { name: "Connect" }));
    const form = screen.getByTestId("api-key-form-datadog");
    // Fields empty → canSubmit false → Enter reaches submit's guard, which returns.
    fireEvent.keyDown(within(form).getByLabelText("API key"), { key: "Enter" });
    expect(submitApiKeyConnectActionMock).not.toHaveBeenCalled();
  });

  it("submits the api_key form via Enter when complete, and ignores other keys", async () => {
    submitApiKeyConnectActionMock.mockResolvedValue({ ok: true, data: { status: "connected" } });
    renderConnectors([DATADOG]);
    fireEvent.click(within(screen.getByTestId("integration-datadog")).getByRole("button", { name: "Connect" }));
    const form = screen.getByTestId("api-key-form-datadog");
    fireEvent.change(within(form).getByLabelText("API key"), { target: { value: "k" } });
    fireEvent.change(within(form).getByLabelText("App key"), { target: { value: "k" } });
    fireEvent.change(within(form).getByLabelText("Site"), { target: { value: "s" } });
    // A non-Enter key is ignored; Enter submits.
    fireEvent.keyDown(within(form).getByLabelText("Site"), { key: "a" });
    expect(submitApiKeyConnectActionMock).not.toHaveBeenCalled();
    fireEvent.keyDown(within(form).getByLabelText("Site"), { key: "Enter" });
    await waitFor(() =>
      expect(submitApiKeyConnectActionMock).toHaveBeenCalledWith("datadog", WS, {
        api_key: "k",
        app_key: "k",
        site: "s",
      }),
    );
  });
});
