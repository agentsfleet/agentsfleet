import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { OnboardingInputs } from "@/lib/onboarding";
import { subscribeOnboardingRefresh } from "@/lib/onboarding-refresh";

vi.mock("next/link", () => ({
  default: ({ href, children }: React.PropsWithChildren<{ href: string }>) =>
    React.createElement("a", { href }, children),
}));
const refresh = vi.fn();
vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh }) }));
const putPreferenceAction = vi.fn();
vi.mock("@/lib/actions/preferences", () => ({ putPreferenceAction: (...a: unknown[]) => putPreferenceAction(...a) }));
const capture = vi.fn();
let unsubscribeRefresh: (() => void) | null = null;
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: (...a: unknown[]) => capture(...a) }));

import GettingStarted from "./GettingStarted";
import { EVENTS } from "@/lib/analytics/events";

const INPUTS: OnboardingInputs = {
  modelConfigured: true,
  fleetTotal: 0,
  secretCount: 0,
  hasProcessedEvent: false,
  hasSteerEvent: false,
  cliTicked: false,
};

function renderGS(inputs = INPUTS) {
  return render(React.createElement(GettingStarted, { workspaceId: "ws_1", inputs }));
}

afterEach(() => {
  unsubscribeRefresh?.();
  unsubscribeRefresh = null;
  cleanup();
  refresh.mockReset();
  putPreferenceAction.mockReset();
  capture.mockReset();
});

describe("GettingStarted — the checklist page (Wall empty state)", () => {
  it("renders the rail and fires the viewed funnel event once with the completed count", async () => {
    renderGS();
    await waitFor(() =>
      expect(capture).toHaveBeenCalledWith(EVENTS.onboarding_viewed, { workspace_id: "ws_1", completed_steps: 1 }),
    );
    expect(screen.getByText("Getting started")).toBeTruthy();
    expect(screen.getByText("1/5 done")).toBeTruthy();
    expect(screen.getByText("Install a fleet")).toBeTruthy();
  });

  it("ticks the CLI: persists, fires the event, refreshes", async () => {
    const onboardingRefresh = vi.fn();
    unsubscribeRefresh = subscribeOnboardingRefresh("ws_1", onboardingRefresh);
    putPreferenceAction.mockResolvedValue({ ok: true, data: {} });
    const user = userEvent.setup();
    renderGS();
    await user.click(screen.getByRole("button", { name: /I've installed the CLI/i }));
    await waitFor(() =>
      expect(putPreferenceAction).toHaveBeenCalledWith("ws_1", "getting_started_cli_ticked", true),
    );
    expect(capture).toHaveBeenCalledWith(EVENTS.onboarding_cli_ticked, { workspace_id: "ws_1" });
    expect(onboardingRefresh).toHaveBeenCalledTimes(1);
    expect(refresh).toHaveBeenCalled();
  });

  it("reverts and surfaces an error when the CLI tick write fails", async () => {
    const onboardingRefresh = vi.fn();
    unsubscribeRefresh = subscribeOnboardingRefresh("ws_1", onboardingRefresh);
    putPreferenceAction.mockResolvedValue({ ok: false, error: "down" });
    const user = userEvent.setup();
    renderGS();
    await user.click(screen.getByRole("button", { name: /I've installed the CLI/i }));
    await waitFor(() => expect(screen.getByText(/Couldn't save/)).toBeTruthy());
    // Reverted → the button is back (cliTicked flipped back to false). Wait for
    // the transition's `pending` to settle: on the error render it can still be
    // true, so the button momentarily reads "Saving…" before reverting to the
    // tick prompt — a synchronous assert races that window under load.
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /I've installed the CLI/i })).toBeTruthy(),
    );
    expect(onboardingRefresh).not.toHaveBeenCalled();
    expect(refresh).not.toHaveBeenCalled();
  });
});
