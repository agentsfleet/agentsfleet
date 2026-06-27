import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { EVENTS } from "../lib/analytics/events";

const { startGithubConnectActionMock, captureProductEventMock } = vi.hoisted(() => ({
  startGithubConnectActionMock: vi.fn(),
  captureProductEventMock: vi.fn(),
}));
vi.mock("@/app/(dashboard)/credentials/connector-actions", () => ({
  startGithubConnectAction: startGithubConnectActionMock,
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

import IntegrationsConnectors from "@/app/(dashboard)/credentials/components/IntegrationsConnectors";
import { CONNECTOR_STATUS } from "@/lib/api/connectors";

const WS = "ws_test";
const PLANNED_INTEGRATIONS = ["zoho", "slack"] as const;

afterEach(() => {
  cleanup();
  startGithubConnectActionMock.mockReset();
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

  it("renders Zoho and Slack as planned connectors that request access via email", () => {
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
    const requestLinks = screen.getAllByRole("link", { name: "Request access" });
    expect(requestLinks).toHaveLength(PLANNED_INTEGRATIONS.length);
    for (const link of requestLinks) {
      expect((link.getAttribute("href") ?? "").startsWith("mailto:agentsfleet@agentmail.to")).toBe(true);
    }
  });

  it("captures interest and marks Requested when a planned connector is requested", () => {
    render(
      React.createElement(IntegrationsConnectors, {
        workspaceId: WS,
        githubStatus: CONNECTOR_STATUS.notConnected,
      }),
    );
    fireEvent.click(screen.getAllByRole("link", { name: "Request access" })[0]!);
    expect(captureProductEventMock).toHaveBeenCalledWith(
      EVENTS.integration_requested,
      { integration_id: "zoho", integration_name: "Zoho" },
      { setPersonProperties: { last_integration_requested: "zoho" } },
    );
    expect(screen.getByTestId("integration-zoho").querySelector("button[disabled]")).toBeTruthy();
  });
});
