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
  it("a done step renders an explicit check and a struck-through label", () => {
    // Model configured → done; its label must be struck through.
    const { container, getByText } = renderRail({ ...ZERO, modelConfigured: true });
    expect(container.querySelector('[data-step-state="done"]')).not.toBeNull();
    expect(container.querySelector(".lucide-check")).not.toBeNull();
    const label = getByText("Model configured");
    expect(label.className).toContain("line-through");
  });

  it("the next incomplete step renders the centre-dot marker, no strikethrough", () => {
    // Nothing done → model_configured is the next step.
    const { container, getByText } = renderRail(ZERO);
    const nextMarker = container.querySelector('[data-step-state="next step"]');
    expect(nextMarker).not.toBeNull();
    // The next marker is the small static centre dot, NOT a check (that's the
    // done state) and NOT the wake-pulse animation (reserved for live entities).
    expect(nextMarker?.querySelector('[data-current-step="true"]')).not.toBeNull();
    expect(nextMarker?.querySelector(".lucide-check")).toBeNull();
    expect(getByText("Model configured").className).not.toContain("line-through");
  });

  it("a future step renders a plain hollow marker", () => {
    // Model done makes install_fleet the next; connect_credential is future.
    const { container } = renderRail({ ...ZERO, modelConfigured: true });
    expect(container.querySelector('[data-step-state="pending"]')).not.toBeNull();
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

describe("OnboardingRail — a row that goes somewhere says so", () => {
  it("draws a chevron on an incomplete step that links, and none on a done one", () => {
    // Model done → struck through, no chevron: nothing left to go and do.
    // Install a fleet → next, links → chevron.
    const { container } = renderRail({ ...ZERO, modelConfigured: true });
    const rows = Array.from(container.querySelectorAll("li"));
    const done = rows.find((li) => li.textContent?.includes("Model configured"));
    const next = rows.find((li) => li.textContent?.includes("Install a fleet"));
    expect(done?.querySelector(".lucide-chevron-right")).toBeNull();
    expect(next?.querySelector(".lucide-chevron-right")).not.toBeNull();
  });

  it("draws no chevron on a step with no destination", () => {
    // "Watch it wake" completes by activity, not by navigation, so a chevron
    // would point at nothing.
    const { container } = renderRail({ ...ZERO, modelConfigured: true, fleetTotal: 1, secretCount: 1 });
    const rows = Array.from(container.querySelectorAll("li"));
    const wake = rows.find((li) => li.textContent?.includes("Watch it wake"));
    expect(wake?.querySelector("a")).toBeNull();
    expect(wake?.querySelector(".lucide-chevron-right")).toBeNull();
  });

  it("draws no chevron in the compact widget", () => {
    const { container } = render(
      React.createElement(OnboardingRail, {
        workspaceId: "ws_1",
        steps: deriveSteps({ ...ZERO, modelConfigured: true }),
        compact: true,
      }),
    );
    expect(container.querySelector(".lucide-chevron-right")).toBeNull();
  });

  it("sends Install a fleet straight to the recommended card", () => {
    const { container } = renderRail({ ...ZERO, modelConfigured: true });
    const rows = Array.from(container.querySelectorAll("li"));
    const next = rows.find((li) => li.textContent?.includes("Install a fleet"));
    expect(next?.querySelector("a")?.getAttribute("href")).toBe(
      "/w/ws_1/fleets/new?library_id=github-pr-reviewer&library_visibility=public",
    );
  });
});

describe("OnboardingRail — the next step's words beckon with the button", () => {
  it("marks only the next step's label, never a done or future one", () => {
    const { getByText } = renderRail({ ...ZERO, modelConfigured: true });
    expect(getByText("Install a fleet").getAttribute("data-beckon-text")).toBe("true");
    expect(getByText("Model configured").getAttribute("data-beckon-text")).toBeNull();
    expect(getByText("Connect its credential").getAttribute("data-beckon-text")).toBeNull();
  });

  it("does not beckon in the compact widget", () => {
    const { getByText } = render(
      React.createElement(OnboardingRail, {
        workspaceId: "ws_1",
        steps: deriveSteps({ ...ZERO, modelConfigured: true }),
        compact: true,
      }),
    );
    expect(getByText("Install a fleet").getAttribute("data-beckon-text")).toBeNull();
  });
});
