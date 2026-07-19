import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { OnboardingInputs } from "@/lib/onboarding";

vi.mock("next/link", () => ({
  default: ({ href, children }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href }, children),
}));

const getProgress = vi.fn();
const putPreference = vi.fn();
vi.mock("@/lib/actions/preferences", () => ({
  getOnboardingProgressAction: (...a: unknown[]) => getProgress(...a),
  putPreferenceAction: (...a: unknown[]) => putPreference(...a),
}));
const capture = vi.fn();
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: (...a: unknown[]) => capture(...a) }));

import GettingStartedWidget from "./GettingStartedWidget";
import { PREFERENCE_KEY } from "@/lib/api/preferences";
import { requestOnboardingRefresh } from "@/lib/onboarding-refresh";

const COMPLETE: OnboardingInputs = {
  modelConfigured: true,
  fleetTotal: 1,
  secretCount: 1,
  hasProcessedEvent: true,
  hasSteerEvent: true,
  cliTicked: false,
};
const INCOMPLETE: OnboardingInputs = { ...COMPLETE, hasSteerEvent: false };

function progressOk(inputs: OnboardingInputs, over: Partial<{ dismissed: boolean; collapsed: boolean }> = {}) {
  return { ok: true as const, data: { inputs, dismissed: false, collapsed: false, ...over } };
}

function renderWidget(workspaceId = "ws_1", pollingMode: "desktop" | "mounted" = "mounted") {
  return render(
    React.createElement(
      TooltipProvider,
      null,
      React.createElement(GettingStartedWidget, { workspaceId, pollingMode }),
    ),
  );
}

beforeEach(() => {
  getProgress.mockReset();
  putPreference.mockReset().mockResolvedValue({ ok: true, data: {} });
  capture.mockReset();
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  cleanup();
});

describe("GettingStartedWidget — strikethrough + collapse (4.1)", () => {
  it("renders done steps struck-through and collapses/expands the step list", async () => {
    getProgress.mockResolvedValue(progressOk(INCOMPLETE));
    const user = userEvent.setup();
    const { getByText, queryByText, getByLabelText } = renderWidget();

    await waitFor(() => expect(getByText("Model configured")).toBeTruthy());
    expect(getByText("Model configured").className).toContain("line-through");

    // Collapse hides the rail rows; the header count stays.
    await user.click(getByLabelText("Collapse getting started"));
    await waitFor(() => expect(queryByText("Model configured")).toBeNull());
    expect(putPreference).toHaveBeenCalledWith("ws_1", PREFERENCE_KEY.COLLAPSED, true);
  });
});

describe("GettingStartedWidget — dismiss persists (4.2)", () => {
  it("dismiss is offered only when complete, writes the pref, and hides the widget", async () => {
    getProgress.mockResolvedValue(progressOk(COMPLETE));
    const user = userEvent.setup();
    const { getByText, queryByText, getByLabelText } = renderWidget();

    await waitFor(() => expect(getByText("Getting started")).toBeTruthy());
    await user.click(getByLabelText("Dismiss getting started"));

    await waitFor(() => expect(queryByText("Getting started")).toBeNull());
    expect(putPreference).toHaveBeenCalledWith("ws_1", PREFERENCE_KEY.DISMISSED, true);
    expect(capture).toHaveBeenCalled();
  });

  it("an incomplete checklist offers no dismiss control", async () => {
    getProgress.mockResolvedValue(progressOk(INCOMPLETE));
    const { getByText, queryByLabelText } = renderWidget();
    await waitFor(() => expect(getByText("Getting started")).toBeTruthy());
    expect(queryByLabelText("Dismiss getting started")).toBeNull();
  });

  it("a widget loaded with the dismissed pref set renders nothing (persistence)", async () => {
    getProgress.mockResolvedValue(progressOk(COMPLETE, { dismissed: true }));
    const { queryByText } = renderWidget();
    // Give the effect a tick; the dismissed progress must keep it hidden.
    await new Promise((r) => setTimeout(r, 0));
    expect(queryByText("Getting started")).toBeNull();
  });
});

describe("GettingStartedWidget — read failure never hides onboarding (FM §5)", () => {
  it("shows fail-open onboarding when the initial progress read fails", async () => {
    getProgress.mockResolvedValue({ ok: false, error: "down" });
    const { getByText } = renderWidget();
    await waitFor(() => expect(getByText("0/5")).toBeTruthy());
    expect(putPreference).not.toHaveBeenCalled();
  });

  it("preserves the last good progress when a later refresh fails", async () => {
    getProgress
      .mockResolvedValueOnce(progressOk(INCOMPLETE))
      .mockResolvedValueOnce({ ok: false, error: "down" });
    const rendered = renderWidget();

    await waitFor(() => expect(rendered.getByText("4/5")).toBeTruthy());
    act(() => requestOnboardingRefresh("ws_1"));
    await waitFor(() => expect(getProgress).toHaveBeenCalledTimes(2));
    expect(rendered.getByText("4/5")).toBeTruthy();
  });

  it("ignores a progress that resolves after unmount (no state update on a dead component)", async () => {
    // Control the resolution timing: unmount before it settles so the effect's
    // `if (!live) return` guard fires. A settle-after-unmount must not throw.
    let resolve!: (v: unknown) => void;
    getProgress.mockReturnValue(new Promise((r) => { resolve = r; }));
    const { unmount, queryByText } = renderWidget();
    unmount();
    resolve(progressOk(COMPLETE));
    await new Promise((r) => setTimeout(r, 0));
    expect(queryByText("Getting started")).toBeNull();
  });
});
