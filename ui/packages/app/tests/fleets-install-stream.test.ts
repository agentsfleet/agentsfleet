import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetCommonMocks } from "./helpers/dashboard-mocks";
import { INSTALL_STEP } from "@/lib/streaming/install-steps";

// The install step ladder after create: InstallStreamSteps reads the fleet's
// Server-Sent Events (SSE) stream, and reconciles a missed ready frame from the
// fleet list. Split from fleets-install-states.test.ts at the length cap; the
// boundaries mocked are the same.
const { listFleetsActionMock, useFleetEventStreamMock } = vi.hoisted(() => ({
  listFleetsActionMock: vi.fn(),
  useFleetEventStreamMock: vi.fn(),
}));

vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({
  installFleetAction: vi.fn(),
  listFleetsAction: listFleetsActionMock,
}));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: vi.fn() }));
vi.mock("@/components/domain/useFleetEventStream", () => ({
  useFleetEventStream: useFleetEventStreamMock,
}));

import { InstallStreamSteps } from "../app/(dashboard)/w/[workspaceId]/fleets/new/InstallStreamSteps";

function stubStream(installStep: string | null) {
  useFleetEventStreamMock.mockReturnValue({
    events: [],
    connectionStatus: "live",
    isRunning: false,
    installStep,
    appendOptimistic: vi.fn(),
    reconcileOptimistic: vi.fn(),
    discardOptimistic: vi.fn(),
    convertEvent: vi.fn(),
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks();
  stubStream(null);
  listFleetsActionMock.mockReturnValue(new Promise(() => {}));
});
afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe("test_install_status_stream — InstallStreamSteps consumes the SSE stream", () => {
  function renderSteps(onOpen = vi.fn()) {
    return render(
      React.createElement(InstallStreamSteps, {
        workspaceId: "ws_1",
        fleetId: "zom_1",
        fleetName: "pr-reviewer",
        onOpen,
      }),
    );
  }

  it("renders the creating step before any install frame, no Open fleet yet", () => {
    stubStream(null);
    renderSteps();
    expect(screen.getByText(/creating fleet/i)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /open fleet/i })).toBeNull();
  });

  it("advances to provisioning when the stream reports it", () => {
    stubStream(INSTALL_STEP.PROVISIONING);
    renderSteps();
    expect(screen.getByText(/provisioning/i)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /open fleet/i })).toBeNull();
  });

  it("on install:ready surfaces Open fleet, which routes to the steer/chat", async () => {
    stubStream(INSTALL_STEP.READY);
    const onOpen = vi.fn();
    const user = userEvent.setup({ delay: null });
    renderSteps(onOpen);
    expect(screen.getByText(/is ready/i)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /open fleet/i }));
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("reconciles a missed ready frame from the fleet's active server status", async () => {
    stubStream(null);
    listFleetsActionMock.mockResolvedValueOnce({
      ok: true,
      data: { items: [{ id: "zom_1", status: "active" }] },
    });
    renderSteps();
    await waitFor(() => expect(screen.getByRole("button", { name: /open fleet/i })).toBeTruthy());
    expect(listFleetsActionMock).toHaveBeenCalledWith("ws_1", { limit: 100 });
  });

  it("stops bounded reconciliation with an error when durable status never becomes active", async () => {
    vi.useFakeTimers();
    stubStream(null);
    listFleetsActionMock
      .mockResolvedValueOnce({ ok: false, error: "temporarily unavailable", status: 503 })
      .mockResolvedValue({ ok: true, data: { items: [] } });
    renderSteps();

    await act(async () => {
      await vi.runAllTimersAsync();
    });

    expect(screen.getByText(/install failed/i)).toBeTruthy();
    expect(listFleetsActionMock).toHaveBeenCalledTimes(12);
  });

  it("drops an in-flight reconciliation result after unmount", async () => {
    vi.useFakeTimers();
    stubStream(null);
    let resolveList: (value: unknown) => void = () => {};
    listFleetsActionMock.mockReturnValueOnce(new Promise((resolve) => (resolveList = resolve)));
    const view = renderSteps();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
    });
    view.unmount();
    await act(async () => {
      resolveList({ ok: true, data: { items: [{ id: "zom_1", status: "active" }] } });
      await Promise.resolve();
    });

    expect(screen.queryByRole("button", { name: /open fleet/i })).toBeNull();
  });

  it("an error step renders the failure line (spinner never hangs)", () => {
    stubStream(INSTALL_STEP.ERROR);
    renderSteps();
    expect(screen.getByText(/install failed/i)).toBeTruthy();
  });
});

// ── 9.6: install done routes into the fleet (the steer/chat) ─────────────────
