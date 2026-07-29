import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { RunnerDetail } from "@/lib/api/runners";

const refresh = vi.fn();
const push = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh, push }),
}));

const updateRunnerAdminStateActionMock = vi.fn();
const deleteRunnerActionMock = vi.fn();
vi.mock("../../actions", () => ({
  updateRunnerAdminStateAction: (...args: unknown[]) => updateRunnerAdminStateActionMock(...args),
  deleteRunnerAction: (...args: unknown[]) => deleteRunnerActionMock(...args),
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
    render(<RunnerHeader runner={detail()} grafanaHref={null} />);
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
    render(<RunnerHeader runner={detail()} grafanaHref={null} />);
    expect(screen.getByText("Linux · Landlock (full)")).toBeTruthy();
    expect(screen.getByText("gpu")).toBeTruthy();
    expect(screen.getByText("prod")).toBeTruthy();
    // The raw runner id never renders as visible text; it is reachable only
    // through the copy control.
    expect(screen.queryByText("01J2WQ8F3K7VZ9XB4N6MTYD5AR")).toBeNull();
    expect(screen.getByRole("button", { name: /copy runner id/i })).toBeTruthy();
  });

  it("test_grafana_action_hidden_without_configured_base", () => {
    render(<RunnerHeader runner={detail()} grafanaHref={null} />);
    expect(screen.queryByText("Grafana")).toBeNull();
    cleanup();
    render(
      <RunnerHeader
        runner={detail()}
        grafanaHref="https://grafana.example/d/runners?var-runner_id=01J2WQ8F3K7VZ9XB4N6MTYD5AR"
      />,
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
    render(<RunnerHeader runner={detail()} grafanaHref={null} />);
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
    render(<RunnerHeader runner={detail()} grafanaHref={null} />);
    fireEvent.click(screen.getByRole("button", { name: /copy runner id/i }));
    // The copy control announces the failure in its own accessible name and
    // never shows a success state (the design system's documented behaviour).
    expect(await screen.findByRole("button", { name: /copy failed/i })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /copied/i })).toBeNull();
  });
});
