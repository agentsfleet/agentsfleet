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

describe("RunnerHeader", () => {
  it("test_runner_header_has_no_visible_second_title", () => {
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    // The host name appears once as visible text, inside the breadcrumb; the
    // page's own heading is present but screen-reader-only.
    const heading = screen.getByRole("heading", { level: 1 });
    expect(heading.className).toContain("sr-only");
    const visible = screen.getAllByText("runner-prod-ams-01.internal");
    const rendered = visible.filter((el) => !el.className.includes("sr-only"));
    expect(rendered).toHaveLength(1);
    expect(rendered[0]?.closest("nav")).not.toBeNull();
  });

  it("test_runner_header_identity_line", () => {
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    expect(screen.getByText("Landlock")).toBeTruthy();
    expect(screen.getByText("gpu")).toBeTruthy();
    expect(screen.getByText("prod")).toBeTruthy();
    // The raw runner id never renders as visible text; it is reachable only
    // through the copy control.
    expect(screen.queryByText("01J2WQ8F3K7VZ9XB4N6MTYD5AR")).toBeNull();
    expect(screen.getByRole("button", { name: /copy runner id/i })).toBeTruthy();
  });

  it("test_degraded_runner_row_states_the_reason (header face)", () => {
    // Dimensions 4.1/4.2 — a degraded runner is visually distinct (the error
    // badge) and names the missing mechanism beside what the host reported.
    render(
      <RunnerHeader
        runner={detail({
          degraded: true,
          degraded_reason: "landlock unavailable",
          achievable: {
            landlock: false,
            seccomp: true,
            cgroup_controllers: ["cpu", "memory", "pids"],
            bubblewrap: true,
            egress_enforcement: false,
          },
        })}
        grafanaHref={null} canWrite />,
    );
    expect(screen.getByText("degraded")).toBeTruthy();
    expect(screen.getByText(/assignment unmet: landlock unavailable/)).toBeTruthy();
    expect(screen.getByText(/host reports landlock ✗/)).toBeTruthy();
  });

  it("a healthy runner shows neither the degraded badge nor the mismatch line", () => {
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    expect(screen.queryByText("degraded")).toBeNull();
    expect(screen.queryByText(/assignment unmet/)).toBeNull();
  });

  it("a degraded runner with no report yet names the reason without a host-reports line", () => {
    render(
      <RunnerHeader
        runner={detail({ degraded: true, degraded_reason: "no assigned policy", achievable: null })}
        grafanaHref={null} canWrite />,
    );
    expect(screen.getByText(/assignment unmet: no assigned policy/)).toBeTruthy();
    expect(screen.queryByText(/host reports/)).toBeNull();
  });

  it("an empty controllers list renders as absent in the achievable line", () => {
    render(
      <RunnerHeader
        runner={detail({
          degraded: true,
          degraded_reason: "cgroup controllers not delegated",
          achievable: {
            landlock: true,
            seccomp: false,
            cgroup_controllers: [],
            bubblewrap: false,
            egress_enforcement: true,
          },
        })}
        grafanaHref={null} canWrite />,
    );
    expect(screen.getByText(/seccomp ✗/)).toBeTruthy();
    expect(screen.getByText(/cgroups ✗/)).toBeTruthy();
    expect(screen.getByText(/bubblewrap ✗/)).toBeTruthy();
    expect(screen.getByText(/egress ✓/)).toBeTruthy();
  });

  it("saving a policy re-assignment refreshes the header (the row must show the new truth)", async () => {
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    // The island stub fires onSaved directly — the wiring under test.
    fireEvent.click(screen.getByRole("button", { name: "Edit policy" }));
    await waitFor(() => expect(refresh).toHaveBeenCalled());
  });

  it("test_grafana_action_hidden_without_configured_base", () => {
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    expect(screen.queryByText("Grafana")).toBeNull();
    cleanup();
    render(
      <RunnerHeader
        runner={detail()}
        grafanaHref="https://grafana.example/d/runners?var-runner_id=01J2WQ8F3K7VZ9XB4N6MTYD5AR" canWrite />,
    );
    const grafana = screen.getByText("Grafana").closest("a");
    expect(grafana?.getAttribute("href")).toContain("var-runner_id=");
  });

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

  it("test_runner_header_copy_failure_is_reported", async () => {
    const writeText = vi.fn().mockRejectedValueOnce(new Error("denied"));
    // jsdom's navigator.clipboard is getter-only; defineProperty replaces it
    // (the design system's own CopyButton tests use this setup).
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: /copy runner id/i }));
    // The copy control announces the failure in its own accessible name and
    // never shows a success state (the design system's documented behaviour).
    expect(await screen.findByRole("button", { name: /copy failed/i })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /copied/i })).toBeNull();
  });

  it("should close the confirm and refresh when an admin action succeeds", async () => {
    updateRunnerAdminStateActionMock.mockResolvedValueOnce({
      ok: true,
      data: { admin_state: "cordoned" },
    });
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: "Cordon" }));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cordon" }));
    await waitFor(() => {
      expect(updateRunnerAdminStateActionMock).toHaveBeenCalledWith(detail().id, "cordon");
      expect(refresh).toHaveBeenCalled();
    });
    // Success closes the confirm — no error is left behind.
    await waitFor(() => {
      expect(screen.queryByRole("alertdialog")).toBeNull();
    });
  });

  it("should delete a revoked runner and route back to the wall", async () => {
    deleteRunnerActionMock.mockResolvedValueOnce({ ok: true, data: undefined });
    render(<RunnerHeader runner={detail({ admin_state: "revoked", liveness: "offline" })} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => {
      expect(deleteRunnerActionMock).toHaveBeenCalledWith(detail().id);
      expect(push).toHaveBeenCalledWith("/admin/runners");
    });
  });

  it("should surface a delete failure inside the confirm and refresh the header", async () => {
    deleteRunnerActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-RUN-016",
      error: "Runner must be revoked before deletion",
    });
    render(<RunnerHeader runner={detail({ admin_state: "revoked", liveness: "offline" })} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => {
      expect(deleteRunnerActionMock).toHaveBeenCalledTimes(1);
      expect(refresh).toHaveBeenCalled();
      expect(push).not.toHaveBeenCalled();
    });
    // The failure reads inside the still-open confirm, beside the state the
    // header just re-read — never a silent close.
    expect(within(screen.getByRole("alertdialog")).getByRole("alert")).toBeTruthy();
  });

  it("should not offer delete before the runner is revoked", () => {
    render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    expect(screen.queryByRole("button", { name: "Delete" })).toBeNull();
  });

  it("should walk away from either confirm without acting when the operator cancels", async () => {
    // Cancelling the admin-action confirm fires no state change. (An active
    // runner — a revoked one no longer offers the action buttons.)
    const { unmount } = render(<RunnerHeader runner={detail()} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: "Revoke" }));
    let dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => {
      expect(screen.queryByRole("alertdialog")).toBeNull();
    });
    expect(updateRunnerAdminStateActionMock).not.toHaveBeenCalled();
    unmount();

    // Cancelling the delete confirm deletes nothing and routes nowhere.
    render(<RunnerHeader runner={detail({ admin_state: "revoked", liveness: "offline" })} grafanaHref={null} canWrite />);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => {
      expect(screen.queryByRole("alertdialog")).toBeNull();
    });
    expect(deleteRunnerActionMock).not.toHaveBeenCalled();
    expect(push).not.toHaveBeenCalled();
  });
});
