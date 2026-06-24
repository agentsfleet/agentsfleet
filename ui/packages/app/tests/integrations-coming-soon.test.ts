import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

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

afterEach(() => cleanup());

const PLANNED_INTEGRATIONS = ["github", "zoho", "slack"] as const;

describe("IntegrationsComingSoon (test_integrations_coming_soon)", () => {
  it("renders GitHub, Zoho, Slack as Planned with NO Connect control", () => {
    render(React.createElement(IntegrationsComingSoon));

    for (const name of PLANNED_INTEGRATIONS) {
      const row = screen.getByTestId(`integration-${name}`);
      expect(row).toBeTruthy();
      // Every integration reads "Planned" — no Connect control anywhere.
      expect(row.textContent).toContain("Planned");
      expect(row.querySelector("button")).toBeNull();
    }
    // No "Connect" affordance exists on the whole surface (the connector is a
    // later milestone).
    expect(screen.queryByRole("button", { name: /connect/i })).toBeNull();
    expect(screen.queryByText(/^connect$/i)).toBeNull();
  });

  it("surfaces the custom-secret bridge hint", () => {
    render(React.createElement(IntegrationsComingSoon));
    const hint = screen.getByTestId("integrations-coming-soon");
    // The bridge hint tells the user to store a custom secret in the meantime.
    expect(hint.textContent).toMatch(/custom secret/i);
    expect(screen.getByText("GITHUB_TOKEN")).toBeTruthy();
  });
});
