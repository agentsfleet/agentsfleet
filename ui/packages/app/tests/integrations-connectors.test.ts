import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { EVENTS } from "../lib/analytics/events";

const { startGithubConnectActionMock, startSlackConnectActionMock, captureProductEventMock } =
  vi.hoisted(() => ({
    startGithubConnectActionMock: vi.fn(),
    startSlackConnectActionMock: vi.fn(),
    captureProductEventMock: vi.fn(),
  }));
vi.mock("@/app/(dashboard)/integrations/connector-actions", () => ({
  startGithubConnectAction: startGithubConnectActionMock,
  startSlackConnectAction: startSlackConnectActionMock,
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
  };
});

import IntegrationsConnectors from "@/app/(dashboard)/integrations/components/IntegrationsConnectors";
import { CONNECTOR_STATUS } from "@/lib/api/connectors";

const WS = "ws_test";
const PLANNED_INTEGRATIONS = ["zoho"] as const;

afterEach(() => {
  cleanup();
  startGithubConnectActionMock.mockReset();
  startSlackConnectActionMock.mockReset();
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
    startGithubConnectActionMock.mockResolvedValue({ ok: false, error: "not wired yet" });
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /connect github/i }));
    await waitFor(() => expect(startGithubConnectActionMock).toHaveBeenCalledWith(WS));
  });

  it("redirects the browser to the install URL when connect succeeds", async () => {
    const install_url = "https://github.com/apps/agentsfleet/installations/new?state=signed";
    startGithubConnectActionMock.mockResolvedValue({ ok: true, data: { install_url } });
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

  it("renders Zoho as a planned connector with a Request access button (no email)", () => {
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
      }),
    );
    for (const name of PLANNED_INTEGRATIONS) {
      const row = screen.getByTestId(`integration-${name}`);
      expect(row.textContent).toContain("Planned");
    }
    // Request access is a PostHog-only signal now — a plain button, never a
    // mailto link. No <a> should carry an email href.
    const requestButtons = screen.getAllByRole("button", { name: "Request access" });
    expect(requestButtons).toHaveLength(PLANNED_INTEGRATIONS.length);
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
    startSlackConnectActionMock.mockResolvedValue({ ok: false, error: "not wired yet" });
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
        slackStatus: CONNECTOR_STATUS.notConnected,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /connect slack/i }));
    await waitFor(() => expect(startSlackConnectActionMock).toHaveBeenCalledWith(WS));
  });

  it("redirects the browser to the Slack authorize URL when connect succeeds", async () => {
    const install_url = "https://slack.com/oauth/v2/authorize?state=signed";
    startSlackConnectActionMock.mockResolvedValue({ ok: true, data: { install_url } });
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
