import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { RunnerDetail, SelftestReport } from "@/lib/api/runners";
import type { AssignedPolicy } from "@/lib/api/runners-types";
import { RunnerChecksBadge } from "./RunnerChecksBadge";

afterEach(() => cleanup());

const renderBadge = (ui: ReactElement) => render(ui, { wrapper: TooltipProvider });

const ASSIGNED: AssignedPolicy = {
  sandbox_tier: "landlock_full",
  network_policy: "deny_all_egress",
  registry_allowlist: [],
  worker_count: 2,
  extra_binds: [],
};

const PASSING: SelftestReport = {
  checks: [
    { name: "resolver file resolves inside the sandbox", ok: true, detail: "no fault detected" },
    { name: "a hostname resolves inside the sandbox", ok: true, detail: "no fault detected" },
  ],
  all_ok: true,
  sandbox_tier: ASSIGNED.sandbox_tier,
  network_policy: ASSIGNED.network_policy,
};

const FAILING: SelftestReport = {
  ...PASSING,
  all_ok: false,
  checks: [
    { name: "resolver file resolves inside the sandbox", ok: false, detail: "the stub is not bound" },
    { name: "a hostname resolves inside the sandbox", ok: false, detail: "the resolver did not answer" },
    { name: "the inference endpoint is reachable", ok: true, detail: "no fault detected" },
  ],
};

function detail(overrides: Partial<RunnerDetail> = {}): RunnerDetail {
  return {
    id: "runner-1",
    host_id: "runner-prod-ams-01.internal",
    sandbox_tier: "landlock_full",
    admin_state: "active",
    liveness: "online",
    labels: [],
    last_seen_at: 1_760_000_000_000,
    created_at: 1_759_000_000_000,
    assigned_policy: ASSIGNED,
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

const trigger = () => screen.getByRole("button", { name: /checks/i });
const DAY_MS = 86_400_000;

describe("RunnerChecksBadge", () => {
  it("names the passing verdict on a green icon without a timestamp", () => {
    renderBadge(<RunnerChecksBadge runner={detail({ selftest: PASSING, selftest_completed_at: Date.now() - 5 * 3_600_000 })} />);
    expect(trigger().getAttribute("aria-label")).toMatch(/checks passed/i);
    expect(trigger().className).toContain("text-success");
    expect(trigger().parentElement?.querySelector("time")).toBeNull();
    expect(trigger().textContent).toBe("");
  });

  it("paints a failure red and counts it, so it is seen without a click", () => {
    renderBadge(<RunnerChecksBadge runner={detail({ selftest: FAILING, selftest_completed_at: 1 })} />);
    expect(trigger().getAttribute("aria-label")).toMatch(/2 checks failed/i);
    expect(trigger().className).toContain("text-destructive");
  });

  it("makes the last-check age and exact date available on keyboard focus", async () => {
    const completedAt = Date.now() - 6 * DAY_MS;
    renderBadge(<RunnerChecksBadge runner={detail({ selftest: PASSING, selftest_completed_at: completedAt })} />);
    fireEvent.focus(trigger());
    const tooltip = await screen.findByRole("tooltip");
    expect(tooltip.textContent).toContain("Last checked 6 days ago");
    const times = tooltip.querySelectorAll("time");
    expect(times).toHaveLength(2);
    expect(times[1]?.getAttribute("datetime")).toBe(new Date(completedAt).toISOString());
    expect(times[1]?.textContent).not.toContain("ago");
  });

  it("does not invent a last-check time before any check completes", async () => {
    renderBadge(<RunnerChecksBadge runner={detail()} />);
    fireEvent.focus(trigger());
    const tooltip = await screen.findByRole("tooltip");
    expect(tooltip.textContent).toContain("checks never run");
    expect(tooltip.textContent).not.toContain("Last checked");
    expect(tooltip.querySelector("time")).toBeNull();
  });

  it("says never rather than showing an empty verdict", () => {
    renderBadge(<RunnerChecksBadge runner={detail()} />);
    expect(trigger().getAttribute("aria-label")).toMatch(/checks never run/i);
    expect(trigger().className).toContain("text-muted-foreground");
    expect(trigger().parentElement?.querySelector("time")).toBeNull();
  });

  it("names an outstanding request so a blank verdict does not read as a healthy one", () => {
    renderBadge(<RunnerChecksBadge runner={detail({ selftest_requested_at: 1_760_000_000_000 })} />);
    expect(trigger().getAttribute("aria-label")).toMatch(/checks pending/i);
  });


  it("marks a verdict recorded against an assignment the runner no longer carries", () => {
    renderBadge(
      <RunnerChecksBadge
        runner={detail({
          selftest: { ...PASSING, network_policy: "allow_all" },
          selftest_completed_at: 1_760_000_000_000,
        })}
      />,
    );
    expect(trigger().getAttribute("aria-label")).toMatch(/checks stale/i);
    expect(trigger().className).toContain("text-warning");
  });

  it("opens the full report — every check by name and the mounts — on click", () => {
    renderBadge(<RunnerChecksBadge runner={detail({ selftest: PASSING, selftest_completed_at: 1_760_000_000_000 })} />);
    expect(screen.queryByRole("dialog")).toBeNull();
    const button = trigger();
    expect(button.getAttribute("aria-haspopup")).toBe("dialog");
    fireEvent.click(button);
    const dialog = screen.getByRole("dialog", { name: "runner-prod-ams-01.internal" });
    expect(dialog.querySelectorAll("h2, h3")[0]?.textContent).toBe("runner-prod-ams-01.internal");
    expect(dialog.textContent).toContain("resolver file resolves inside the sandbox");
    expect(dialog.textContent).toContain("all checks passed");
    expect(within(dialog).getByText("No additional mounts.")).toBeTruthy();
    expect(dialog.querySelector("time")).toBeNull();
    // The modal hides the page behind it from assistive tech, the trigger
    // included; the element itself still reports the open state.
    expect(button.getAttribute("aria-expanded")).toBe("true");
  });
});
