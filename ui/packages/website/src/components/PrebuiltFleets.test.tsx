import { fireEvent, render, screen, within } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

const analytics = vi.hoisted(() => ({
  trackSignupStarted: vi.fn(),
}));

vi.mock("../analytics/posthog", async () => {
  const actual = await vi.importActual<typeof import("../analytics/posthog")>(
    "../analytics/posthog",
  );
  return { ...actual, trackSignupStarted: analytics.trackSignupStarted };
});

import PrebuiltFleets from "./PrebuiltFleets";
import { WAITLIST_URL } from "../config";
import {
  FLEETS_SECTION_HEADING,
  LOOP_ANCHOR_ID,
  PREBUILT_FLEETS,
} from "../lib/marketing-copy";

function renderFleets() {
  return render(
    <BrowserRouter>
      <PrebuiltFleets />
    </BrowserRouter>,
  );
}

describe("PrebuiltFleets", () => {
  beforeEach(() => {
    analytics.trackSignupStarted.mockReset();
  });

  it("renders the fleet section under the preserved loop anchor", () => {
    const { container } = renderFleets();
    const section = screen.getByTestId("prebuilt-fleets");
    expect(section).toBeInTheDocument();
    expect(screen.getByText(FLEETS_SECTION_HEADING)).toBeInTheDocument();
    // Anchor kept so the hero "Meet the fleet" link + footer + llms.txt resolve.
    expect(container.querySelector(`#${LOOP_ANCHOR_ID}`)).not.toBeNull();
  });

  it("renders every prebuilt Fleet with its name, category, and integration logos", () => {
    renderFleets();
    for (const fleet of PREBUILT_FLEETS) {
      const card = screen.getByTestId(`fleet-card-${fleet.id}`);
      expect(card).toHaveTextContent(fleet.name);
      expect(card).toHaveTextContent(fleet.category);
      expect(within(card).getAllByRole("term").map((term) => term.textContent)).toEqual([
        "Wakes on", "Delivers", "Your control",
      ]);
      const cta = within(card).getByRole("link", { name: /join the waitlist/i });
      expect(cta).toHaveAttribute("href", WAITLIST_URL);
      const icons = within(card).getByTestId(
        `fleet-integrations-${fleet.id}`,
      ).querySelectorAll("img");
      expect(icons).toHaveLength(fleet.integrations.length);
      for (const integration of fleet.integrations) {
        expect(card).toHaveTextContent(integration.label);
      }
    }
  });

  it("points each Fleet CTA at the waitlist and tracks the click", () => {
    renderFleets();
    const auto = screen.getByTestId("fleet-cta-auto-reviewer");
    expect(auto.tagName).toBe("A");
    expect(auto).toHaveAttribute("href", WAITLIST_URL);
    expect(auto).toHaveAttribute("target", "_blank");
    expect(auto).toHaveAttribute("rel", "noopener noreferrer");
    expect(auto).toHaveTextContent(/join the waitlist/i);
    fireEvent.click(auto);
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "fleet_auto-reviewer",
      surface: "fleets",
      mode: "humans",
    });
  });

  it("marks the roadmap-sourced Security Reviewer as coming soon with a waitlist CTA", () => {
    renderFleets();
    const card = screen.getByTestId("fleet-card-security-reviewer");
    expect(card).toHaveTextContent("Security Reviewer");
    expect(card).toHaveTextContent(/secret|vulnerab/i);
    expect(screen.getByTestId("fleet-coming-soon-security-reviewer")).toHaveTextContent(
      /coming soon/i,
    );
    const cta = screen.getByTestId("fleet-cta-security-reviewer");
    expect(cta).toHaveAttribute("href", WAITLIST_URL);
    expect(cta).toHaveTextContent(/join the waitlist/i);
  });

  it("does not imply immediate installation or guaranteed results", () => {
    renderFleets();
    const section = screen.getByTestId("prebuilt-fleets");
    expect(within(section).queryAllByRole("link", { name: /try it|install now/i })).toHaveLength(0);
    expect(section).not.toHaveTextContent(/prebuilt and proven|same day|every action gated|no setup/i);
    expect(screen.queryByTestId("fleet-card-coming-soon")).toBeNull();
  });

  it("explains configured review triggers without promising automatic merges", () => {
    renderFleets();
    const card = screen.getByTestId("fleet-card-auto-reviewer");
    expect(card).toHaveTextContent(/configured repositories/i);
    expect(card).toHaveTextContent(/review comments/i);
    expect(card).toHaveTextContent(/you review and merge/i);
    expect(card).not.toHaveTextContent(/every pull request|before a human/i);
  });

  it("keeps repository writes approval-gated and deployment human-owned", () => {
    renderFleets();
    const card = screen.getByTestId("fleet-card-diagnose");
    expect(card).toHaveTextContent(/draft pull request/i);
    expect(card).toHaveTextContent(/approve repository write access/i);
    expect(card).toHaveTextContent(/never merges or deploys/i);
  });

  it("does not market the Slack resident as unattended or a prebuilt install", () => {
    renderFleets();
    const card = screen.getByTestId("fleet-card-slack-teammate");
    expect(card).toHaveTextContent(/channel memory across threads/i);
    expect(card).toHaveTextContent(/mention-only/i);
    expect(card).toHaveTextContent(/read-only/i);
    expect(card).toHaveTextContent(/never acts unattended/i);
    expect(card).toHaveTextContent(/connect Slack/i);
    expect(within(card).queryByText(/coming soon/i)).toBeNull();
  });

  it("keeps the product pillars in Core Capabilities", () => {
    renderFleets();
    // The Isolated / Compounding / Proactive pillars moved to Core Capabilities
    // (rendered on Home as capability-pillar-*). They must not appear here.
    expect(screen.queryByTestId("fleet-pillar-sandbox")).toBeNull();
    expect(screen.queryByTestId("fleet-pillar-learns")).toBeNull();
    expect(screen.queryByTestId("fleet-pillar-proactive")).toBeNull();
  });
});
