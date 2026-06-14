import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import OnboardingFlow from "./OnboardingFlow";

describe("OnboardingFlow", () => {
  it("renders four numbered cards in declared order", () => {
    render(<OnboardingFlow />);
    const ids = ["install", "skill", "wire", "steer"] as const;
    ids.forEach((id) => {
      expect(screen.getByTestId(`onboarding-step-${id}`)).toBeInTheDocument();
    });
    expect(screen.getByTestId("onboarding-step-number-install").textContent).toMatch(
      /step 01/i,
    );
    expect(screen.getByTestId("onboarding-step-number-skill").textContent).toMatch(
      /step 02/i,
    );
    expect(screen.getByTestId("onboarding-step-number-wire").textContent).toMatch(
      /step 03/i,
    );
    expect(screen.getByTestId("onboarding-step-number-steer").textContent).toMatch(
      /step 04/i,
    );
  });

  it("renders the steps in a responsive grid with no arrow connectors", () => {
    const { container } = render(<OnboardingFlow />);
    // Steps live in a grid (1 col → 2×2), not a single flex row that
    // overflows the page; the inline arrow connectors are gone.
    expect(screen.queryAllByTestId("onboarding-flow-arrow")).toHaveLength(0);
    const grid = container.querySelector(".grid");
    expect(grid).not.toBeNull();
    ["install", "skill", "wire", "steer"].forEach((id) => {
      expect(
        grid!.querySelector(`[data-testid="onboarding-step-${id}"]`),
      ).not.toBeNull();
    });
  });

  it("renders each spec'd snippet inside its card's Terminal", () => {
    render(<OnboardingFlow />);
    const install = within(screen.getByTestId("onboarding-step-install"));
    expect(install.getByText(/curl -fsSL https:\/\/usezombie\.sh \| bash/)).toBeInTheDocument();

    const skill = within(screen.getByTestId("onboarding-step-skill"));
    expect(skill.getByText(/\/usezombie-install-platform-ops/)).toBeInTheDocument();

    const wire = within(screen.getByTestId("onboarding-step-wire"));
    expect(wire.getByText(/gh api -X POST repos/)).toBeInTheDocument();

    const steer = within(screen.getByTestId("onboarding-step-steer"));
    expect(steer.getByText(/agentsfleet steer/)).toBeInTheDocument();
  });

  it("anchors the outer section at #onboarding-flow (deep-link target)", () => {
    render(<OnboardingFlow />);
    const section = screen.getByTestId("onboarding-flow");
    expect(section.getAttribute("id")).toBe("onboarding-flow");
  });

  it("renders a tertiary link to the docs quickstart", () => {
    render(<OnboardingFlow />);
    const link = screen.getByTestId("onboarding-flow-quickstart");
    expect(link.getAttribute("href")).toBe("https://docs.agentsfleet.net/quickstart");
  });
});
