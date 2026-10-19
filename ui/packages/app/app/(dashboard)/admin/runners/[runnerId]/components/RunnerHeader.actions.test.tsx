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
  const { EDIT_POLICY_LABEL } = await import("../../components/RunnerListCells");
  return {
    default: ({ onSaved }: { onSaved: () => void }) => (
      <Button onClick={onSaved}>{EDIT_POLICY_LABEL}</Button>
    ),
  };
});

const updateRunnerAdminStateActionMock = vi.fn();
const updateRunnerActionMock = vi.fn();
const deleteRunnerActionMock = vi.fn();
const requestRunnerSelftestActionMock = vi.fn();
vi.mock("../../actions", () => ({
  updateRunnerAdminStateAction: (...args: unknown[]) => updateRunnerAdminStateActionMock(...args),
  updateRunnerAction: (...args: unknown[]) => updateRunnerActionMock(...args),
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
    status: "active",
    liveness: "busy",
    labels: ["gpu", "prod"],
    last_seen_at: new Date().toISOString(),
    created_at: new Date().toISOString(),
    assigned_policy: {sandbox_tier: "landlock_full", network_policy: "allow_all", registry_allowlist: [], worker_count: 1, extra_binds: []},
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
  it("retains the revoke dialog when its request throws and reconciles the server state", async () => {
    updateRunnerAdminStateActionMock.mockRejectedValueOnce(new Error("private-transport-error"));
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", {name: "Revoke"}));
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", {name: "Revoke"}));
    await waitFor(() => expect(within(screen.getByRole("alertdialog")).getByRole("alert").textContent).toContain("Try again"));
    expect(screen.getByRole("alertdialog").textContent).not.toContain("private-transport-error");
    expect(refresh).toHaveBeenCalledOnce();
  });
  it("should close the confirm and refresh when an admin action succeeds", async () => {
    // Revoke carries the case: it is the one confirm-backed action still
    // operable (cordon and drain render disabled until their verbs land).
    updateRunnerAdminStateActionMock.mockResolvedValueOnce({
      ok: true,
      data: { status: "revoked" },
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

    // Painted before the daemon answers: the badge reads revoked. The one
    // destructive control does not follow the paint — Delete waits for the
    // server to confirm the revoke.
    await waitFor(() => expect(screen.getByText(/^revoked/)).toBeTruthy());
    expect(screen.queryByText(/^active/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Delete" })).toBeNull();

    // While the PATCH is out the dialog is held: both buttons disabled, the
    // confirm reads Working…, and a second click sends no second PATCH.
    const working = within(dialog).getByRole("button", { name: "Working…" });
    expect(working.hasAttribute("disabled")).toBe(true);
    expect(within(dialog).getByRole("button", { name: "Cancel" }).hasAttribute("disabled")).toBe(true);
    fireEvent.click(working);
    expect(updateRunnerAdminStateActionMock).toHaveBeenCalledTimes(1);

    settle({ ok: false, status: 409, errorCode: "UZ-RUN-015", error: "state changed under you" });

    // A conflict ends the transition: the server-rendered state is back, the
    // error is shown, and the header re-reads so the real state follows.
    await waitFor(() => expect(screen.getByText(/^active/)).toBeTruthy());
    expect(screen.queryByText(/^revoked/)).toBeNull();
    await waitFor(() => expect(refresh).toHaveBeenCalled());
    expect(screen.getByRole("alertdialog").textContent).toMatch(/state changed under you/);
  });
});
