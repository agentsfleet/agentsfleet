import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { RunnerDetail } from "@/lib/api/runners";

// The self-test control is the header's one action with no confirm dialog, so
// its request/refusal wiring is a concern of its own — split from
// RunnerHeader.test.tsx (which owns identity, degraded state, and the two
// dialog-backed actions) to keep both files inside the length cap.

const refresh = vi.fn();
const push = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh, push }),
}));

vi.mock("@/components/domain/island-dynamic/EditPolicyDialogDynamic", async () => {
  const { Button } = await import("@agentsfleet/design-system");
  return {
    default: ({ onSaved }: { onSaved: () => void }) => <Button onClick={onSaved}>Edit policy</Button>,
  };
});

const requestRunnerSelftestActionMock = vi.fn();
vi.mock("../../actions", () => ({
  updateRunnerAdminStateAction: vi.fn(),
  updateRunnerPolicyAction: vi.fn(),
  deleteRunnerAction: vi.fn(),
  requestRunnerSelftestAction: (...args: unknown[]) => requestRunnerSelftestActionMock(...args),
  listRunnerLeasesAction: vi.fn(),
  listRunnersAction: vi.fn(),
  createRunnerAction: vi.fn(),
}));

import { RunnerHeader } from "./RunnerHeader";

const RUNNER_ID = "01J2WQ8F3K7VZ9XB4N6MTYD5AR";
const SELFTEST_LABEL = "Run self-test";
const SELFTEST_PENDING_LABEL = "Self-test requested";

afterEach(() => cleanup());
beforeEach(() => {
  refresh.mockReset();
  push.mockReset();
  requestRunnerSelftestActionMock.mockReset();
});

function detail(overrides: Partial<RunnerDetail> = {}): RunnerDetail {
  return {
    id: RUNNER_ID,
    host_id: "runner-prod-ams-01.internal",
    sandbox_tier: "landlock_full",
    admin_state: "active",
    liveness: "busy",
    labels: [],
    last_seen_at: Date.now(),
    created_at: Date.now(),
    assigned_policy: null,
    achievable: null,
    degraded: false,
    degraded_reason: null,
    selftest_requested_at: null,
    selftest_completed_at: null,
    selftest: null,
    active_lease_count: 0,
    active_fleet_count: 0,
    leases_acquired: 0,
    leases_succeeded: 0,
    leases_failed: 0,
    leases_expired: 0,
    ...overrides,
  };
}

describe("RunnerHeader self-test control", () => {
  it("records the request against this runner and re-reads the header for the pending state", async () => {
    requestRunnerSelftestActionMock.mockResolvedValueOnce({
      ok: true,
      data: { id: RUNNER_ID, admin_state: "active", selftest_requested_at: Date.now() },
    });
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: SELFTEST_LABEL }));
    await waitFor(() => {
      expect(requestRunnerSelftestActionMock).toHaveBeenCalledWith(RUNNER_ID);
      // No verdict is awaited — the refresh is what makes the pending state
      // appear, so it must fire on the success path too.
      expect(refresh).toHaveBeenCalled();
    });
    // A successful request leaves nothing to read: no dialog, no alert.
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("surfaces a refused self-test beside the control rather than swallowing it", async () => {
    requestRunnerSelftestActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-RUN-018",
      error: "Revoked runner cannot self-test",
    });
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: SELFTEST_LABEL }));
    // The control opens no dialog, so the refusal has to read on the header
    // itself — the daemon's message behind the operator-facing lead.
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBe("Couldn't run a self-test on this runner — Revoked runner cannot self-test.");
    expect(refresh).toHaveBeenCalled();
  });

  it("clears a previous refusal when the operator asks again", async () => {
    requestRunnerSelftestActionMock
      .mockResolvedValueOnce({ ok: false, errorCode: "UZ-RUN-018", error: "Runner is offline" })
      .mockResolvedValueOnce({
        ok: true,
        data: { id: RUNNER_ID, admin_state: "active", selftest_requested_at: Date.now() },
      });
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: SELFTEST_LABEL }));
    expect(await screen.findByRole("alert")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: SELFTEST_LABEL }));
    await waitFor(() => {
      expect(requestRunnerSelftestActionMock).toHaveBeenCalledTimes(2);
      // A stale refusal beside a request that has just succeeded would read as
      // the new request failing.
      expect(screen.queryByRole("alert")).toBeNull();
    });
  });

  it("disables the control and names the outstanding ask while a request is unanswered", () => {
    render(<RunnerHeader runner={detail({ selftest_requested_at: Date.now() })} grafanaHref={null} canWrite />);
    const button = screen.getByRole("button", { name: SELFTEST_PENDING_LABEL });
    expect(button.hasAttribute("disabled")).toBe(true);
    expect(screen.queryByRole("button", { name: SELFTEST_LABEL })).toBeNull();
    fireEvent.click(button);
    expect(requestRunnerSelftestActionMock).not.toHaveBeenCalled();
  });

  it("offers no self-test on a revoked runner — it will never heartbeat again to answer", () => {
    render(
      <RunnerHeader
        runner={detail({ admin_state: "revoked", liveness: "offline" })}
        grafanaHref={null}
        canWrite
      />,
    );
    expect(screen.queryByRole("button", { name: SELFTEST_LABEL })).toBeNull();
    expect(screen.queryByRole("button", { name: SELFTEST_PENDING_LABEL })).toBeNull();
  });

  it("offers no self-test to a read-only operator", () => {
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite={false} />);
    expect(screen.queryByRole("button", { name: SELFTEST_LABEL })).toBeNull();
  });
});
