import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { RunnerLease } from "@/lib/api/runners";

vi.mock("@/lib/pagination/use-url-cursor-pages", () => ({
  useUrlCursorPages: () => ({
    page: 1,
    hasNext: false,
    isLoading: false,
    goToPage: vi.fn(),
    changePageSize: vi.fn(),
  }),
}));

import { LeaseTable } from "./LeaseTable";

afterEach(() => cleanup());

function lease(overrides: Partial<RunnerLease>): RunnerLease {
  return {
    id: `lease-${Math.random().toString(36).slice(2)}`,
    fleet_id: "fleet-1",
    fleet_name: "Search Services",
    workspace_id: "ws-1",
    event_id: "evt-1",
    event_type: "index_build",
    actor: "system",
    outcome: "succeeded",
    failure_label: null,
    failure_detail: null,
    kind: "fresh",
    fencing_token: 1884,
    provider: "azure_openai",
    model: "gpt-4o-mini",
    posture: "metered",
    metered_input_tokens: 18204,
    metered_cached_tokens: 4096,
    metered_output_tokens: 2881,
    wall_ms: 242_000,
    lease_expires_at: Date.now(),
    created_at: Date.now(),
    ...overrides,
  };
}

describe("LeaseTable", () => {
  it("test_lease_table_orders_live_leases_first", () => {
    render(
      <LeaseTable
        initial={{
          items: [
            lease({ id: "settled-1", outcome: "succeeded" }),
            lease({ id: "running-1", outcome: "running", wall_ms: null }),
            lease({ id: "settled-2", outcome: "failed", failure_label: "oom_kill" }),
            lease({ id: "running-2", outcome: "running", wall_ms: null }),
          ],
          total: 4,
          next_cursor: null,
        }}
        pageSize={25}
      />,
      { wrapper: TooltipProvider },
    );
    const rows = screen.getAllByRole("row").slice(1); // drop the header row
    const runningIndexes = rows
      .map((row, index) => (row.textContent?.includes("RUNNING") ? index : -1))
      .filter((index) => index >= 0);
    const settledIndexes = rows
      .map((row, index) => (row.textContent?.includes("RUNNING") ? -1 : index))
      .filter((index) => index >= 0);
    expect(runningIndexes).toHaveLength(2);
    expect(Math.max(...runningIndexes)).toBeLessThan(Math.min(...settledIndexes));
  });

  it("test_lease_table_failed_row_renders_failure_sentence", () => {
    render(
      <LeaseTable
        initial={{
          items: [
            lease({
              id: "failed-1",
              outcome: "failed",
              failure_label: "oom_kill",
              failure_detail: "Container exceeded its 2 GiB memory limit and was terminated.",
            }),
          ],
          total: 1,
          next_cursor: null,
        }}
        pageSize={25}
      />,
      { wrapper: TooltipProvider },
    );
    // The shared plain-English sentence renders; the machine tag never does.
    expect(screen.getByText("Ran out of memory")).toBeTruthy();
    expect(screen.getByText("Container exceeded its 2 GiB memory limit and was terminated.")).toBeTruthy();
    expect(screen.queryByText(/oom_kill/)).toBeNull();
  });

  it("test_lease_table_expired_row_states_reclaim", () => {
    render(
      <LeaseTable
        initial={{
          items: [lease({ id: "expired-1", outcome: "expired" })],
          total: 1,
          next_cursor: null,
        }}
        pageSize={25}
      />,
      { wrapper: TooltipProvider },
    );
    expect(screen.getByText("Lease not renewed")).toBeTruthy();
    expect(
      screen.getByText("This runner stopped renewing; the work was re-leased to another runner."),
    ).toBeTruthy();
  });

  it("suppresses the outcome fabrication for a lease with no recorded event", () => {
    render(
      <LeaseTable
        initial={{
          items: [lease({ id: "unknown-1", outcome: "unknown" })],
          total: 1,
          next_cursor: null,
        }}
        pageSize={25}
      />,
      { wrapper: TooltipProvider },
    );
    expect(screen.getByText("Outcome not recorded")).toBeTruthy();
  });
});
