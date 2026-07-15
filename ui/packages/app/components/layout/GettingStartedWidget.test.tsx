import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { OnboardingInputs } from "@/lib/onboarding";

vi.mock("next/link", () => ({
  default: ({ href, children }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href }, children),
}));

const getSnapshot = vi.fn();
const putPreference = vi.fn();
vi.mock("@/lib/actions/preferences", () => ({
  getOnboardingSnapshotAction: (...a: unknown[]) => getSnapshot(...a),
  putPreferenceAction: (...a: unknown[]) => putPreference(...a),
}));
const capture = vi.fn();
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: (...a: unknown[]) => capture(...a) }));

import GettingStartedWidget from "./GettingStartedWidget";
import { PREFERENCE_KEY } from "@/lib/api/preferences";

const COMPLETE: OnboardingInputs = {
  modelConfigured: true,
  fleetTotal: 1,
  secretCount: 1,
  hasProcessedEvent: true,
  hasSteerEvent: true,
  cliTicked: false,
};
const INCOMPLETE: OnboardingInputs = { ...COMPLETE, hasSteerEvent: false };

function snapshotOk(inputs: OnboardingInputs, over: Partial<{ dismissed: boolean; collapsed: boolean }> = {}) {
  return { ok: true as const, data: { inputs, dismissed: false, collapsed: false, ...over } };
}

function renderWidget() {
  return render(
    React.createElement(TooltipProvider, null, React.createElement(GettingStartedWidget, { workspaceId: "ws_1" })),
  );
}

beforeEach(() => {
  getSnapshot.mockReset();
  putPreference.mockReset().mockResolvedValue({ ok: true, data: {} });
  capture.mockReset();
});
afterEach(() => cleanup());

describe("GettingStartedWidget — strikethrough + collapse (4.1)", () => {
  it("renders done steps struck-through and collapses/expands the step list", async () => {
    getSnapshot.mockResolvedValue(snapshotOk(INCOMPLETE));
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
    getSnapshot.mockResolvedValue(snapshotOk(COMPLETE));
    const user = userEvent.setup();
    const { getByText, queryByText, getByLabelText } = renderWidget();

    await waitFor(() => expect(getByText("Getting started")).toBeTruthy());
    await user.click(getByLabelText("Dismiss getting started"));

    await waitFor(() => expect(queryByText("Getting started")).toBeNull());
    expect(putPreference).toHaveBeenCalledWith("ws_1", PREFERENCE_KEY.DISMISSED, true);
    expect(capture).toHaveBeenCalled();
  });

  it("an incomplete checklist offers no dismiss control", async () => {
    getSnapshot.mockResolvedValue(snapshotOk(INCOMPLETE));
    const { getByText, queryByLabelText } = renderWidget();
    await waitFor(() => expect(getByText("Getting started")).toBeTruthy());
    expect(queryByLabelText("Dismiss getting started")).toBeNull();
  });

  it("a widget loaded with the dismissed pref set renders nothing (persistence)", async () => {
    getSnapshot.mockResolvedValue(snapshotOk(COMPLETE, { dismissed: true }));
    const { queryByText } = renderWidget();
    // Give the effect a tick; the dismissed snapshot must keep it hidden.
    await new Promise((r) => setTimeout(r, 0));
    expect(queryByText("Getting started")).toBeNull();
  });
});

describe("GettingStartedWidget — read failure never hides onboarding (FM §5)", () => {
  it("stays hidden (no false checklist) when the snapshot read fails", async () => {
    // A failed action leaves the widget in its null-snapshot state — it renders
    // nothing rather than a zeroed checklist, and crucially never marks
    // onboarding dismissed off a read failure.
    getSnapshot.mockResolvedValue({ ok: false, error: "down" });
    const { queryByText } = renderWidget();
    await new Promise((r) => setTimeout(r, 0));
    expect(queryByText("Getting started")).toBeNull();
    expect(putPreference).not.toHaveBeenCalled();
  });
});
