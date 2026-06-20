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
    // A shipped Fleet invites you to "Try it"; a coming-soon one does not.
    expect(auto).toHaveTextContent(/try it/i);
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

  it("shows a coming-soon tile and no longer renders the product pillars", () => {
    renderFleets();
    expect(screen.getByTestId("fleet-card-coming-soon")).toHaveTextContent(/coming soon/i);
    // The Isolated / Compounding / Proactive pillars moved to Core Capabilities
    // (rendered on Home as capability-pillar-*). They must not appear here.
    expect(screen.queryByTestId("fleet-pillar-sandbox")).toBeNull();
    expect(screen.queryByTestId("fleet-pillar-learns")).toBeNull();
    expect(screen.queryByTestId("fleet-pillar-proactive")).toBeNull();
  });
});
