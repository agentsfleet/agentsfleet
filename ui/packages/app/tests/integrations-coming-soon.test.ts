import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { EVENTS } from "../lib/analytics/events";

const { captureProductEventMock } = vi.hoisted(() => ({
  captureProductEventMock: vi.fn(),
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

import IntegrationsComingSoon from "@/app/(dashboard)/credentials/components/IntegrationsComingSoon";

afterEach(() => {
  cleanup();
  captureProductEventMock.mockReset();
});

const PLANNED_INTEGRATIONS = ["zoho", "slack"] as const;

describe("IntegrationsComingSoon (test_integrations_coming_soon)", () => {
  it("renders GitHub as native and not connected until the vault key exists", () => {
    render(React.createElement(IntegrationsComingSoon));

    const github = screen.getByTestId("integration-github");
    expect(github.textContent).toContain("Not connected");
    expect(github.textContent).toContain("GITHUB_TOKEN");
    expect(screen.getByRole("link", { name: /connect github/i }).getAttribute("href")).toBe(
      "#add-custom-secret",
    );
  });

  it("renders GitHub as connected once GITHUB_TOKEN exists", () => {
    render(React.createElement(IntegrationsComingSoon, { credentialNames: ["GITHUB_TOKEN"] }));

    const github = screen.getByTestId("integration-github");
    expect(github.textContent).toContain("Connected");
    expect(screen.queryByRole("link", { name: /connect github/i })).toBeNull();
  });

  it("renders Zoho and Slack as planned connectors that request access via email", () => {
    render(React.createElement(IntegrationsComingSoon));

    for (const name of PLANNED_INTEGRATIONS) {
      const row = screen.getByTestId(`integration-${name}`);
      expect(row).toBeTruthy();
      expect(row.textContent).toContain("Planned");
      expect(row.textContent).toContain("Request access");
    }
    // The request control is a mailto link to the team inbox.
    const requestLinks = screen.getAllByRole("link", { name: "Request access" });
    expect(requestLinks).toHaveLength(PLANNED_INTEGRATIONS.length);
    for (const link of requestLinks) {
      const href = link.getAttribute("href") ?? "";
      expect(href.startsWith("mailto:agentsfleet@agentmail.to")).toBe(true);
      expect(href).toContain("subject=");
    }
    expect(screen.queryByText(/^connect$/i)).toBeNull();
  });

  it("shows a planned connector as token stored when its secret exists", () => {
    render(React.createElement(IntegrationsComingSoon, { credentialNames: ["ZOHO_TOKEN"] }));

    const zoho = screen.getByTestId("integration-zoho");
    expect(zoho.textContent).toContain("Token stored");
    expect(zoho.textContent).toContain("Request access");
  });

  it("captures interest and marks Requested when a planned connector is requested", () => {
    render(React.createElement(IntegrationsComingSoon));
    fireEvent.click(screen.getAllByRole("link", { name: "Request access" })[0]!);

    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.integration_requested, {
      integration_id: "zoho",
    });
    const zoho = screen.getByTestId("integration-zoho");
    expect(zoho.textContent).toContain("Requested");
    // After requesting, the control is a disabled button (no second submit).
    expect(zoho.querySelector("button[disabled]")).toBeTruthy();
    expect(JSON.stringify(captureProductEventMock.mock.calls)).not.toContain("ZOHO_TOKEN");
  });

  it("surfaces the integrations helper hint", () => {
    render(React.createElement(IntegrationsComingSoon));
    const hint = screen.getByTestId("integrations-coming-soon");
    expect(hint.textContent).toMatch(/github connects now\. request zoho or slack if needed\./i);
    expect(screen.getAllByText("GITHUB_TOKEN")).toHaveLength(1);
  });
});
