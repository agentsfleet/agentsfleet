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

import PrebuiltAgents from "./PrebuiltAgents";
import { WAITLIST_URL } from "../config";
import {
  AGENT_PILLARS,
  AGENTS_SECTION_HEADING,
  LOOP_ANCHOR_ID,
  PREBUILT_AGENTS,
} from "../lib/marketing-copy";

function renderAgents() {
  return render(
    <BrowserRouter>
      <PrebuiltAgents />
    </BrowserRouter>,
  );
}

describe("PrebuiltAgents", () => {
  beforeEach(() => {
    analytics.trackSignupStarted.mockReset();
  });

  it("renders the fleet section under the preserved loop anchor", () => {
    const { container } = renderAgents();
    const section = screen.getByTestId("prebuilt-agents");
    expect(section).toBeInTheDocument();
    expect(screen.getByText(AGENTS_SECTION_HEADING)).toBeInTheDocument();
    // Anchor kept so the hero "Meet the fleet" link + footer + llms.txt resolve.
    expect(container.querySelector(`#${LOOP_ANCHOR_ID}`)).not.toBeNull();
  });

  it("renders every prebuilt agent with its name, category, and integration logos", () => {
    renderAgents();
    for (const agent of PREBUILT_AGENTS) {
      const card = screen.getByTestId(`agent-card-${agent.id}`);
      expect(card).toHaveTextContent(agent.name);
      expect(card).toHaveTextContent(agent.category);
      const icons = within(card).getByTestId(
        `agent-integrations-${agent.id}`,
      ).querySelectorAll("img");
      expect(icons).toHaveLength(agent.integrations.length);
      for (const integration of agent.integrations) {
        expect(card).toHaveTextContent(integration.label);
      }
    }
  });

  it("points each agent CTA at the waitlist and tracks the click", () => {
    renderAgents();
    const auto = screen.getByTestId("agent-cta-auto-reviewer");
    expect(auto.tagName).toBe("A");
    expect(auto).toHaveAttribute("href", WAITLIST_URL);
    // A shipped agent invites you to "Try it"; a coming-soon one does not.
    expect(auto).toHaveTextContent(/try it/i);
    fireEvent.click(auto);
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "agent_auto-reviewer",
      surface: "agents",
      mode: "humans",
    });
  });

  it("marks the roadmap-sourced Security Reviewer as coming soon with a waitlist CTA", () => {
    renderAgents();
    const card = screen.getByTestId("agent-card-security-reviewer");
    expect(card).toHaveTextContent("Security Reviewer");
    expect(card).toHaveTextContent(/secret|vulnerab/i);
    expect(screen.getByTestId("agent-coming-soon-security-reviewer")).toHaveTextContent(
      /coming soon/i,
    );
    const cta = screen.getByTestId("agent-cta-security-reviewer");
    expect(cta).toHaveAttribute("href", WAITLIST_URL);
    expect(cta).toHaveTextContent(/join the waitlist/i);
  });

  it("shows a coming-soon tile and the three product pillars", () => {
    renderAgents();
    expect(screen.getByTestId("agent-card-coming-soon")).toHaveTextContent(/coming soon/i);
    for (const pillar of AGENT_PILLARS) {
      expect(screen.getByTestId(`agent-pillar-${pillar.id}`)).toHaveTextContent(pillar.title);
    }
  });
});
