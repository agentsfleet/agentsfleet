import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { RunnerDetail } from "@/lib/api/runners";

const refresh = vi.fn();
const push = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh, push }),
}));

// The edit-policy dialog is a dynamic island (it carries the react-hook-form +
// zod stack off the detail route's critical path). The header's own duty is
// wiring its onSaved to a route refresh, which this stub lets the test drive —
// the dialog's own behaviour is covered in EditPolicyDialog.test.tsx.
vi.mock("@/components/domain/island-dynamic/EditPolicyDialogDynamic", async () => {
  const { Button } = await import("@agentsfleet/design-system");
  return {
    default: ({ onSaved }: { onSaved: () => void }) => (
      <Button onClick={onSaved}>Edit policy</Button>
    ),
  };
});

const updateRunnerAdminStateActionMock = vi.fn();
const updateRunnerPolicyActionMock = vi.fn();
const deleteRunnerActionMock = vi.fn();
const requestRunnerSelftestActionMock = vi.fn();
vi.mock("../../actions", () => ({
  updateRunnerAdminStateAction: (...args: unknown[]) => updateRunnerAdminStateActionMock(...args),
  updateRunnerPolicyAction: (...args: unknown[]) => updateRunnerPolicyActionMock(...args),
  deleteRunnerAction: (...args: unknown[]) => deleteRunnerActionMock(...args),
  requestRunnerSelftestAction: (...args: unknown[]) => requestRunnerSelftestActionMock(...args),
  listRunnerLeasesAction: vi.fn(),
  listRunnersAction: vi.fn(),
  createRunnerAction: vi.fn(),
}));

import { RunnerHeader } from "./RunnerHeader";

afterEach(() => cleanup());
beforeEach(() => {
  refresh.mockReset();
  push.mockReset();
  updateRunnerAdminStateActionMock.mockReset();
  deleteRunnerActionMock.mockReset();
  requestRunnerSelftestActionMock.mockReset();
});

function detail(overrides: Partial<RunnerDetail> = {}): RunnerDetail {
  return {
    id: "01J2WQ8F3K7VZ9XB4N6MTYD5AR",
    host_id: "runner-prod-ams-01.internal",
    sandbox_tier: "landlock_full",
    admin_state: "active",
    liveness: "busy",
    labels: ["gpu", "prod"],
    last_seen_at: Date.now(),
    created_at: Date.now(),
    assigned_policy: null,
    achievable: null,
    degraded: false,
    degraded_reason: null,
    selftest_requested_at: null,
    selftest_completed_at: null,
    selftest: null,
    active_lease_count: 2,
    active_fleet_count: 2,
    leases_acquired: 4021,
    leases_succeeded: 3945,
    leases_failed: 42,
    leases_expired: 34,
    ...overrides,
  };
}

// The admin actions: confirm, PATCH, reconcile. Split from RunnerHeader.test.tsx
// by concern, the way the self-test suite already is, so each file stays under
// the length cap.

describe("RunnerHeader — admin actions", () => {
  it("test_runner_header_revoke_conflict_surfaces_state", async () => {
    updateRunnerAdminStateActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-RUN-016",
      error: "Active runner must be revoked before deletion",
    });
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: "Revoke" }));
    // Confirm inside the dialog — its confirm button shares the header
    // button's label, so the query scopes to the alertdialog.
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Revoke" }));
    await waitFor(() => {
      expect(updateRunnerAdminStateActionMock).toHaveBeenCalled();
      // The header re-reads the runner so the badge shows the returned
      // administrative state beside the error, never a stale one.
      expect(refresh).toHaveBeenCalled();
    });
  });

  it("should close the confirm and refresh when an admin action succeeds", async () => {
    // Revoke carries the case: it is the one confirm-backed action still
    // operable (cordon and drain render disabled until their verbs land).
    updateRunnerAdminStateActionMock.mockResolvedValueOnce({
      ok: true,
      data: { admin_state: "revoked" },
    });
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: "Revoke" }));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Revoke" }));
    await waitFor(() => {
      expect(updateRunnerAdminStateActionMock).toHaveBeenCalledWith(detail().id, "revoke");
      expect(refresh).toHaveBeenCalled();
    });
    // Success closes the confirm — no error is left behind.
    await waitFor(() => {
      expect(screen.queryByRole("alertdialog")).toBeNull();
    });
  });

  it("cordon and drain are disabled and inert", () => {
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    for (const name of ["Cordon", "Drain"]) {
      const button = screen.getByRole("button", { name });
      // Disabled; the reason rides the TooltipButton, not a mouse-only title.
      expect(button.hasAttribute("disabled")).toBe(true);
      expect(button.getAttribute("title")).toBeNull();
      // Clicking opens no confirm and PATCHes nothing.
      fireEvent.click(button);
    }
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(updateRunnerAdminStateActionMock).not.toHaveBeenCalled();
  });


  it("a runner badge paints the target state and rolls back on conflict", async () => {
    let settle: (result: { ok: false; status: number; errorCode: string; error: string }) => void = () => {};
    updateRunnerAdminStateActionMock.mockReturnValueOnce(
      new Promise((resolve) => {
        settle = resolve;
      }),
    );
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    expect(screen.getByText(/^active/)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Revoke" }));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Revoke" }));

    // Painted before the daemon answers: the badge reads revoked, and the
    // controls follow it — Delete is offered, Revoke is not.
    await waitFor(() => expect(screen.getByText(/^revoked/)).toBeTruthy());
    expect(screen.queryByText(/^active/)).toBeNull();

    settle({ ok: false, status: 409, errorCode: "UZ-RUN-015", error: "state changed under you" });

    // A conflict ends the transition: the server-rendered state is back, the
    // error is shown, and the header re-reads so the real state follows.
    await waitFor(() => expect(screen.getByText(/^active/)).toBeTruthy());
    expect(screen.queryByText(/^revoked/)).toBeNull();
    await waitFor(() => expect(refresh).toHaveBeenCalled());
    expect(screen.getByRole("alertdialog").textContent).toMatch(/state changed under you/);
  });
});
