import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";

// next/link is a client dependency the pure rail pulls in; a shallow anchor mock
// keeps the render tree client-safe without a router.
vi.mock("next/link", () => ({
  default: ({ href, children }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href }, children),
}));

import OnboardingRail from "./OnboardingRail";
import { deriveSteps, type OnboardingInputs } from "@/lib/onboarding";

const ZERO: OnboardingInputs = {
  modelConfigured: false,
  fleetTotal: 0,
  secretCount: 0,
  hasProcessedEvent: false,
  hasSteerEvent: false,
  cliTicked: false,
};

afterEach(() => cleanup());

function renderRail(inputs: OnboardingInputs) {
  return render(
    React.createElement(OnboardingRail, {
      workspaceId: "ws_1",
      steps: deriveSteps(inputs),
    }),
  );
}

describe("OnboardingRail — tick marks + strikethrough (3.4)", () => {
  it("a done step renders a filled tick marker and a struck-through label", () => {
    // Model configured → done; its label must be struck through.
    const { container, getByText } = renderRail({ ...ZERO, modelConfigured: true });
    expect(container.querySelector('[aria-label="done"]')).not.toBeNull();
    const label = getByText("Model configured");
    expect(label.className).toContain("line-through");
  });

  it("the next incomplete step renders the static ring marker, no strikethrough", () => {
    // Nothing done → model_configured is the ringed next step.
    const { container, getByText } = renderRail(ZERO);
    const nextMarker = container.querySelector('[aria-label="next step"]');
    expect(nextMarker).not.toBeNull();
    // The ring is the static pulse-glow shadow, NOT the wake-pulse animation.
    expect(nextMarker?.className).toContain("shadow-[0_0_0_4px_var(--pulse-glow)]");
    expect(getByText("Model configured").className).not.toContain("line-through");
  });

  it("a future step renders a plain hollow marker", () => {
    // Model done makes install_fleet the next; connect_credential is future.
    const { container } = renderRail({ ...ZERO, modelConfigured: true });
    expect(container.querySelector('[aria-label="pending"]')).not.toBeNull();
  });

  it("renders the steps in fixed order, model first and CLI last", () => {
    const { container } = renderRail(ZERO);
    const labels = Array.from(container.querySelectorAll("li")).map(
      (li) => li.textContent ?? "",
    );
    expect(labels[0]).toContain("Model configured");
    expect(labels[labels.length - 1]).toContain("Install the CLI");
  });

  it("marks the optional step with an OPTIONAL eyebrow", () => {
    const { getByText } = renderRail(ZERO);
    // The eyebrow renders lowercase text uppercased by CSS; the DOM text is "optional".
    expect(getByText("optional")).not.toBeNull();
  });
});
