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
describe("GettingStartedWidget — live completion refresh", () => {
  it("refreshes immediately when a workspace action invalidates onboarding", async () => {
    getProgress
      .mockResolvedValueOnce(progressOk(INCOMPLETE))
      .mockResolvedValueOnce(progressOk(COMPLETE));
    const { getByText } = renderWidget();

    await waitFor(() => expect(getByText("4/5")).toBeTruthy());
    act(() => requestOnboardingRefresh("ws_1"));

    await waitFor(() => expect(getByText("5/5")).toBeTruthy());
    expect(getProgress).toHaveBeenCalledTimes(2);
  });

  it("ignores an invalidation for another workspace", async () => {
    getProgress.mockResolvedValue(progressOk(INCOMPLETE));
    const { getByText } = renderWidget();

    await waitFor(() => expect(getByText("4/5")).toBeTruthy());
    act(() => requestOnboardingRefresh("ws_other"));
    await act(async () => Promise.resolve());

    expect(getProgress).toHaveBeenCalledTimes(1);
  });

  it("refreshes when the browser window regains focus", async () => {
    getProgress
      .mockResolvedValueOnce(progressOk(INCOMPLETE))
      .mockResolvedValueOnce(progressOk(COMPLETE));
    const { getByText } = renderWidget();

    await waitFor(() => expect(getByText("4/5")).toBeTruthy());
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
    });

    await waitFor(() => expect(getByText("5/5")).toBeTruthy());
    expect(getProgress).toHaveBeenCalledTimes(2);
  });

  it("honors a server dismissal after a stale local collapse preference", async () => {
    getProgress
      .mockResolvedValueOnce(progressOk(INCOMPLETE))
      .mockResolvedValueOnce(progressOk(INCOMPLETE, { dismissed: true }));
    const user = userEvent.setup();
    const rendered = renderWidget();

    await waitFor(() => expect(rendered.getByText("4/5")).toBeTruthy());
    await user.click(rendered.getByLabelText("Collapse getting started"));
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
    });

    await waitFor(() => expect(rendered.queryByText("Getting started")).toBeNull());
  });

  it("retries an incomplete progress every 30 seconds while the tab is visible", async () => {
    vi.useFakeTimers();
    getProgress
      .mockResolvedValueOnce({ ok: false, error: "down" })
      .mockResolvedValueOnce(progressOk(INCOMPLETE));
    const { getByText } = renderWidget();

    await act(async () => Promise.resolve());
    expect(getProgress).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });

    expect(getProgress).toHaveBeenCalledTimes(2);
    expect(getByText("4/5")).toBeTruthy();
  });

  it("does not poll while the browser tab is hidden", async () => {
    vi.useFakeTimers();
    vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    getProgress.mockResolvedValue(progressOk(INCOMPLETE));
    renderWidget();

    await act(async () => Promise.resolve());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });

    expect(getProgress).toHaveBeenCalledTimes(1);
  });

  it("does not poll from the CSS-hidden desktop sidebar on mobile", async () => {
    vi.useFakeTimers();
    vi.spyOn(window, "matchMedia").mockReturnValue({ matches: false } as MediaQueryList);
    getProgress.mockResolvedValue(progressOk(INCOMPLETE));
    renderWidget("ws_1", "desktop");

    await act(async () => Promise.resolve());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });

    expect(getProgress).toHaveBeenCalledTimes(1);
  });

  it("does not overlap refreshes for the same workspace", async () => {
    let resolveProgress!: (value: unknown) => void;
    getProgress.mockReturnValue(
      new Promise((resolve) => {
        resolveProgress = resolve;
      }),
    );
    renderWidget();
    await act(async () => Promise.resolve());

    act(() => {
      requestOnboardingRefresh("ws_1");
      window.dispatchEvent(new Event("focus"));
    });
    expect(getProgress).toHaveBeenCalledTimes(1);

    await act(async () => {
      resolveProgress(progressOk(INCOMPLETE));
      await Promise.resolve();
    });
    await waitFor(() => expect(getProgress).toHaveBeenCalledTimes(2));
  });

  it("allows a new workspace read while the previous workspace read settles", async () => {
    const resolvers = new Map<string, (value: unknown) => void>();
    getProgress.mockImplementation(
      (workspaceId: string) =>
        new Promise((resolve) => {
          resolvers.set(workspaceId, resolve);
        }),
    );
    const rendered = renderWidget();
    await act(async () => Promise.resolve());

    rendered.rerender(
      React.createElement(
        TooltipProvider,
        null,
        React.createElement(GettingStartedWidget, { workspaceId: "ws_2" }),
      ),
    );
    await act(async () => Promise.resolve());
    expect(getProgress).toHaveBeenNthCalledWith(2, "ws_2");

    await act(async () => {
      resolvers.get("ws_1")?.(progressOk(INCOMPLETE));
      await Promise.resolve();
    });
    await act(async () => {
      resolvers.get("ws_2")?.(progressOk(COMPLETE));
      await Promise.resolve();
    });
    expect(rendered.getByText("5/5")).toBeTruthy();
  });

  it("keeps the committed workspace active when a concurrent navigation is interrupted", async () => {
    getProgress
      .mockResolvedValueOnce(progressOk(INCOMPLETE))
      .mockResolvedValueOnce(progressOk(COMPLETE));
    const neverSettles = new Promise<never>(() => {});
    let setWorkspaceId!: React.Dispatch<React.SetStateAction<string>>;

    function SuspendNavigation({ workspaceId }: { workspaceId: string }) {
      if (workspaceId === "ws_2") throw neverSettles;
      return null;
    }
    function ConcurrentNavigationHarness() {
      const [workspaceId, setWorkspace] = React.useState("ws_1");
      setWorkspaceId = setWorkspace;
      return React.createElement(
        TooltipProvider,
        null,
        React.createElement(
          React.Suspense,
          { fallback: null },
          React.createElement(GettingStartedWidget, { workspaceId }),
          React.createElement(SuspendNavigation, { workspaceId }),
        ),
      );
    }

    const rendered = render(React.createElement(ConcurrentNavigationHarness));
    await waitFor(() => expect(rendered.getByText("4/5")).toBeTruthy());
    act(() => {
      React.startTransition(() => setWorkspaceId("ws_2"));
    });
    expect(rendered.getByText("4/5")).toBeTruthy();

    act(() => requestOnboardingRefresh("ws_1"));
    await waitFor(() => expect(rendered.getByText("5/5")).toBeTruthy());
  });

  it("coalesces ws1 → ws2 → ws1 reads and ignores the superseded ws1 response", async () => {
    const requests: Array<{
      workspaceId: string;
      resolve: (value: unknown) => void;
    }> = [];
    getProgress.mockImplementation(
      (workspaceId: string) =>
        new Promise((resolve) => {
          requests.push({ workspaceId, resolve });
        }),
    );
    const rendered = renderWidget();
    await act(async () => Promise.resolve());

    rendered.rerender(
      React.createElement(
        TooltipProvider,
        null,
        React.createElement(GettingStartedWidget, { workspaceId: "ws_2" }),
      ),
    );
    await act(async () => Promise.resolve());
    rendered.rerender(
      React.createElement(
        TooltipProvider,
        null,
        React.createElement(GettingStartedWidget, { workspaceId: "ws_1" }),
      ),
    );
    await act(async () => Promise.resolve());

    // Returning to ws_1 queues one fresh read instead of overlapping its
    // existing request. The existing response is now superseded and must not
    // paint its stale 4/5 state.
    expect(getProgress).toHaveBeenCalledTimes(2);
    await act(async () => {
      requests[0]?.resolve(progressOk(INCOMPLETE));
      await Promise.resolve();
    });
    expect(rendered.queryByText("4/5")).toBeNull();
    await waitFor(() => expect(getProgress).toHaveBeenCalledTimes(3));

    await act(async () => {
      requests[2]?.resolve(progressOk(COMPLETE));
      requests[1]?.resolve(progressOk(INCOMPLETE));
      await Promise.resolve();
    });
    expect(rendered.getByText("5/5")).toBeTruthy();
  });

  it("keeps polling complete progress because fleet and secret existence can regress", async () => {
    vi.useFakeTimers();
    getProgress.mockResolvedValue(progressOk(COMPLETE));
    const { getByText } = renderWidget();

    await act(async () => Promise.resolve());
    expect(getByText("5/5")).toBeTruthy();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(getProgress).toHaveBeenCalledTimes(3);
  });
});
