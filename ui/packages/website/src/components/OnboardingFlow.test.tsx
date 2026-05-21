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

  it("renders three arrows — one between each consecutive pair of cards", () => {
    render(<OnboardingFlow />);
    expect(screen.getAllByTestId("onboarding-flow-arrow")).toHaveLength(3);
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
    expect(steer.getByText(/zombiectl steer/)).toBeInTheDocument();
  });

  it("anchors the outer section at #onboarding-flow (deep-link target)", () => {
    render(<OnboardingFlow />);
    const section = screen.getByTestId("onboarding-flow");
    expect(section.getAttribute("id")).toBe("onboarding-flow");
  });

  it("renders a tertiary link to the docs quickstart", () => {
    render(<OnboardingFlow />);
    const link = screen.getByTestId("onboarding-flow-quickstart");
    expect(link.getAttribute("href")).toBe("https://docs.usezombie.com/quickstart");
  });
});
