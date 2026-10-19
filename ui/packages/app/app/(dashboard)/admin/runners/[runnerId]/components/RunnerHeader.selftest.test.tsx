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
  const { EDIT_POLICY_LABEL } = await import("../../components/RunnerListCells");
  return {
    default: ({ onSaved }: { onSaved: () => void }) => <Button onClick={onSaved}>{EDIT_POLICY_LABEL}</Button>,
  };
});

const requestRunnerSelftestActionMock = vi.fn();
vi.mock("../../actions", () => ({
  updateRunnerAdminStateAction: vi.fn(),
  updateRunnerAction: vi.fn(),
  deleteRunnerAction: vi.fn(),
  requestRunnerSelftestAction: (...args: unknown[]) => requestRunnerSelftestActionMock(...args),
  listRunnerLeasesAction: vi.fn(),
  listRunnersAction: vi.fn(),
  createRunnerAction: vi.fn(),
}));

import { RunnerHeader } from "./RunnerHeader";
import { SELFTEST_ACTION_CONFIG } from "../../components/RunnerListCells";

const RUNNER_ID = "01J2WQ8F3K7VZ9XB4N6MTYD5AR";
const SELFTEST_LABEL = SELFTEST_ACTION_CONFIG.label;
const SELFTEST_PENDING_LABEL = "Checks requested";

afterEach(() => cleanup());
it("keeps a failed checks request recoverable without exposing transport details", async () => {
  requestRunnerSelftestActionMock.mockRejectedValueOnce(new Error("private-transport-error"));
  render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
  fireEvent.click(screen.getByRole("button", {name: SELFTEST_LABEL}));
  await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Try again"));
  expect(screen.getByRole("alert").textContent).not.toContain("private-transport-error");
  await waitFor(() => expect(screen.getByRole("button", {name: SELFTEST_LABEL})).toHaveProperty("disabled", false));
});
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
    status: "active",
    liveness: "busy",
    labels: [],
    last_seen_at: new Date().toISOString(),
    created_at: new Date().toISOString(),
    assigned_policy: {sandbox_tier: "landlock_full", network_policy: "allow_all", registry_allowlist: [], worker_count: 1, extra_binds: []},
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
  // Dimension 1.1 — the ask is recorded against this runner, and the re-read is
  // what makes the pending state appear; the pending face itself is asserted by
  // the sibling arm below.
  it("test_selftest_control_requests_and_reflects_pending", async () => {
    requestRunnerSelftestActionMock.mockResolvedValueOnce({
      ok: true,
      data: { id: RUNNER_ID, status: "active", selftest_requested_at: Date.now() },
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
    expect(alert.textContent).toBe("Couldn't run checks on this runner — Revoked runner cannot self-test.");
    expect(refresh).toHaveBeenCalled();
  });

  it("clears a previous refusal when the operator asks again", async () => {
    requestRunnerSelftestActionMock
      .mockResolvedValueOnce({ ok: false, errorCode: "UZ-RUN-018", error: "Runner is offline" })
      .mockResolvedValueOnce({
        ok: true,
        data: { id: RUNNER_ID, status: "active", selftest_requested_at: Date.now() },
      });
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: SELFTEST_LABEL }));
    expect(await screen.findByRole("alert")).toBeTruthy();

    await waitFor(() => expect(screen.getByRole("button", { name: SELFTEST_LABEL })).toHaveProperty("disabled", false));
    fireEvent.click(screen.getByRole("button", { name: SELFTEST_LABEL }));
    await waitFor(() => {
      expect(requestRunnerSelftestActionMock).toHaveBeenCalledTimes(2);
      // A stale refusal beside a request that has just succeeded would read as
      // the new request failing.
      expect(screen.queryByRole("alert")).toBeNull();
    });
  });

  // Dimension 1.1 (pending face) — an unanswered ask has to read as outstanding
  // and refuse a second click, or an operator re-asks into a queue of one.
  it("test_selftest_control_requests_and_reflects_pending (pending face)", () => {
    render(<RunnerHeader runner={detail({ selftest_requested_at: new Date().toISOString() })} grafanaHref={null} canWrite />);
    const button = screen.getByRole("button", { name: SELFTEST_PENDING_LABEL });
    expect(button.hasAttribute("disabled")).toBe(true);
    expect(screen.queryByRole("button", { name: SELFTEST_LABEL })).toBeNull();
    fireEvent.click(button);
    expect(requestRunnerSelftestActionMock).not.toHaveBeenCalled();
  });

  it("offers no self-test on a revoked runner — it will never heartbeat again to answer", () => {
    render(
      <RunnerHeader
        runner={detail({ status: "revoked", liveness: "offline" })}
        grafanaHref={null}
        canWrite
      />,
    );
    expect(screen.queryByRole("button", { name: SELFTEST_LABEL })).toBeNull();
    expect(screen.queryByRole("button", { name: SELFTEST_PENDING_LABEL })).toBeNull();
  });

  // Dimension 1.4 (control absent) — the refusal half is the route scope guard:
  // the self-test is a PATCH on the runner, which needs `runner:write`
  // (`RunnerOpsRoute::Patch` in rustd/crates/afd_http/src/route/runner_ops.rs),
  // proven daemon-side by
  // `runner_patch_is_mounted_behind_runner_write_and_rejects_shape_before_io`
  // in rustd/crates/afd_api/tests/operator_plane.rs.
  it("test_selftest_control_requires_write_scope (control absent)", () => {
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite={false} />);
    expect(screen.queryByRole("button", { name: SELFTEST_LABEL })).toBeNull();
  });
});
