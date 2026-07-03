import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { EVENTS } from "../lib/analytics/events";

const { startConnectActionMock, captureProductEventMock } = vi.hoisted(() => ({
  startConnectActionMock: vi.fn(),
  captureProductEventMock: vi.fn(),
}));
vi.mock("@/app/(dashboard)/integrations/connector-actions", () => ({
  startConnectAction: startConnectActionMock,
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: captureProductEventMock,
}));
vi.mock("lucide-react", () => {
  const make = (name: string) => (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": name });
  return {
    GitPullRequestIcon: make("GitPullRequestIcon"),
    BriefcaseIcon: make("BriefcaseIcon"),
    HashIcon: make("HashIcon"),
    TicketIcon: make("TicketIcon"),
    SquareKanbanIcon: make("SquareKanbanIcon"),
    GaugeIcon: make("GaugeIcon"),
  };
});

import IntegrationsConnectors from "@/app/(dashboard)/integrations/components/IntegrationsConnectors";
import { CONNECTOR_STATUS } from "@/lib/api/connectors";

const WS = "ws_test";
// Connectors without one-click OAuth: Zoho (vault-secret bridge) plus the
// roadmap rows (Jira/Linear/Grafana). All render as "Not connected" with a
// Request-access button until they ship a native connector.
const COMING_SOON_INTEGRATIONS = ["zoho", "jira", "linear", "grafana"] as const;

afterEach(() => {
  cleanup();
  startConnectActionMock.mockReset();
  captureProductEventMock.mockReset();
});

describe("IntegrationsConnectors (test_github_states_and_planned)", () => {
  it("renders GitHub not-connected with a Connect button and no token to paste", () => {
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
      }),
    );
    const github = screen.getByTestId("integration-github");
    expect(github.textContent).toContain("Not connected");
    expect(github.textContent).not.toContain("GITHUB_TOKEN");
    expect(screen.getByRole("button", { name: /connect github/i })).toBeTruthy();
  });

  it("renders GitHub connected with no connect button", () => {
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.connected,
      }),
    );
    const github = screen.getByTestId("integration-github");
    expect(github.textContent).toContain("Connected");
    expect(screen.queryByRole("button", { name: /connect github/i })).toBeNull();
  });

  it("offers Reconnect when the install was revoked", () => {
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.reconnectRequired,
      }),
    );
    expect(screen.getByRole("button", { name: /reconnect github/i })).toBeTruthy();
  });

  it("calls the connect action with the workspace on click", async () => {
    startConnectActionMock.mockResolvedValue({ ok: false, error: "not wired yet" });
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /connect github/i }));
    await waitFor(() => expect(startConnectActionMock).toHaveBeenCalledWith("github", WS));
  });

  it("redirects the browser to the install URL when connect succeeds", async () => {
    const install_url = "https://github.com/apps/agentsfleet/installations/new?state=signed";
    startConnectActionMock.mockResolvedValue({ ok: true, data: { install_url } });
    // jsdom won't navigate; capture the assignment instead of letting it no-op.
    const original = window.location;
    let assigned = "";
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { ...original, set href(v: string) { assigned = v; }, get href() { return assigned; } },
    });
    try {
      render(
        React.createElement(IntegrationsConnectors, {
          workspaceId: WS,
          githubStatus: CONNECTOR_STATUS.notConnected,
        }),
      );
      fireEvent.click(screen.getByRole("button", { name: /connect github/i }));
      await waitFor(() => expect(assigned).toBe(install_url));
    } finally {
      Object.defineProperty(window, "location", { configurable: true, value: original });
    }
  });

  it("renders the coming-soon connectors as Not connected, each with a Request access button (no email)", () => {
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
      }),
    );
    for (const name of COMING_SOON_INTEGRATIONS) {
      const row = screen.getByTestId(`integration-${name}`);
      expect(row.textContent).toContain("Not connected");
    }
    // Description branch: the vault-secret bridge (Zoho) surfaces its secret name;
    // the pure roadmap rows (Jira) read "Coming soon".
    expect(screen.getByTestId("integration-zoho").textContent).toContain("ZOHO_TOKEN");
    expect(screen.getByTestId("integration-jira").textContent).toContain("Coming soon");
    // Request access is a PostHog-only signal now — a plain button, never a
    // mailto link. No <a> should carry an email href.
    const requestButtons = screen.getAllByRole("button", { name: "Request access" });
    expect(requestButtons).toHaveLength(COMING_SOON_INTEGRATIONS.length);
    expect(screen.queryByRole("link", { name: "Request access" })).toBeNull();
    expect(
      Array.from(document.querySelectorAll("a")).some((a) =>
        (a.getAttribute("href") ?? "").startsWith("mailto:"),
      ),
    ).toBe(false);
  });

  it("captures interest and marks Requested when a planned connector is requested", () => {
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
      }),
    );
    fireEvent.click(screen.getAllByRole("button", { name: "Request access" })[0]!);
    expect(captureProductEventMock).toHaveBeenCalledWith(
      EVENTS.integration_requested,
      { integration_id: "zoho", integration_name: "Zoho" },
      { setPersonProperties: { last_integration_requested: "zoho" } },
    );
    expect(screen.getByTestId("integration-zoho").querySelector("button[disabled]")).toBeTruthy();
  });

  it("shows Token stored for a planned connector whose required secret is already in the vault", () => {
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
        // ZOHO_TOKEN is the required secret for the Zoho connector; once stored,
        // the pill reads "Token stored" rather than "Planned".
        credentialNames: ["ZOHO_TOKEN"],
      }),
    );
    expect(screen.getByTestId("integration-zoho").textContent).toContain("Token stored");
  });
});

describe("IntegrationsConnectors — Slack OAuth (test_dashboard_slack_connect_flow)", () => {
  it("renders Slack not-connected with a Connect button and no token to paste", () => {
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
        slackStatus: CONNECTOR_STATUS.notConnected,
      }),
    );
    const slack = screen.getByTestId("integration-slack");
    expect(slack.textContent).toContain("Not connected");
    expect(slack.textContent).not.toContain("SLACK_BOT_TOKEN");
    expect(slack.textContent).not.toContain("Planned");
    expect(screen.getByRole("button", { name: /connect slack/i })).toBeTruthy();
  });

  it("renders Slack connected with the team name and no connect button", () => {
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
        slackStatus: CONNECTOR_STATUS.connected,
        slackTeam: "Acme Corp",
      }),
    );
    const slack = screen.getByTestId("integration-slack");
    expect(slack.textContent).toContain("Slack connected: Acme Corp");
    expect(screen.queryByRole("button", { name: /connect slack/i })).toBeNull();
  });

  it("offers Reconnect when the Slack install was revoked", () => {
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
        slackStatus: CONNECTOR_STATUS.reconnectRequired,
      }),
    );
    expect(screen.getByRole("button", { name: /reconnect slack/i })).toBeTruthy();
  });

  it("calls the Slack connect action with the workspace on click", async () => {
    startConnectActionMock.mockResolvedValue({ ok: false, error: "not wired yet" });
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
        slackStatus: CONNECTOR_STATUS.notConnected,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /connect slack/i }));
    await waitFor(() => expect(startConnectActionMock).toHaveBeenCalledWith("slack", WS));
  });

  it("redirects the browser to the Slack authorize URL when connect succeeds", async () => {
    const install_url = "https://slack.com/oauth/v2/authorize?state=signed";
    startConnectActionMock.mockResolvedValue({ ok: true, data: { install_url } });
    const original = window.location;
    let assigned = "";
    Object.defineProperty(window, "location", {
      configurable: true,
      value: {
        ...original,
        set href(v: string) {
          assigned = v;
        },
        get href() {
          return assigned;
        },
      },
    });
    try {
      render(
        React.createElement(IntegrationsConnectors, {
          workspaceId: WS,
          githubStatus: CONNECTOR_STATUS.notConnected,
          slackStatus: CONNECTOR_STATUS.notConnected,
        }),
      );
      fireEvent.click(screen.getByRole("button", { name: /connect slack/i }));
      await waitFor(() => expect(assigned).toBe(install_url));
    } finally {
      Object.defineProperty(window, "location", { configurable: true, value: original });
    }
  });
});
