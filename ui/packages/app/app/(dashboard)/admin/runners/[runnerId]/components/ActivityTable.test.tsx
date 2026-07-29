import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { RUNNER_LIFECYCLE_EVENT_TYPES, type RunnerEventItem } from "@/lib/api/runners";

vi.mock("@/lib/pagination/use-url-cursor-pages", () => ({
  useUrlCursorPages: () => ({
    page: 1,
    hasNext: false,
    isLoading: false,
    goToPage: vi.fn(),
    changePageSize: vi.fn(),
  }),
}));

import { ActivityTable } from "./ActivityTable";

afterEach(() => cleanup());

function item(overrides: Partial<RunnerEventItem>): RunnerEventItem {
  return {
    id: `evt-${Math.random().toString(36).slice(2)}`,
    runner_id: "r-1",
    event_type: "runner_online",
    occurred_at: Date.now(),
    metadata: {},
    ...overrides,
  };
}

describe("ActivityTable", () => {
  it("test_activity_excludes_lease_work_events", () => {
    // The exported lifecycle set holds neither work tag…
    expect(RUNNER_LIFECYCLE_EVENT_TYPES).not.toContain("lease_acquired");
    expect(RUNNER_LIFECYCLE_EVENT_TYPES).not.toContain("lease_released");
    // …and a feed seeded with both renders neither.
    render(
      <ActivityTable
        initial={{
          items: [
            item({ id: "keep-1", event_type: "runner_online" }),
            item({ id: "drop-1", event_type: "lease_acquired" }),
            item({ id: "drop-2", event_type: "lease_released" }),
          ],
          total: 3,
          next_cursor: null,
        }}
        pageSize={25}
      />,
      { wrapper: TooltipProvider },
    );
    expect(screen.getByText("came online")).toBeTruthy();
    expect(screen.queryByText(/acquired a lease/)).toBeNull();
    expect(screen.queryByText(/released a lease/)).toBeNull();
  });

  it("test_activity_renders_admin_state_transition", () => {
    render(
      <ActivityTable
        initial={{
          items: [
            item({
              event_type: "runner_draining",
              metadata: { from_admin_state: "active", to_admin_state: "draining" },
            }),
          ],
          total: 1,
          next_cursor: null,
        }}
        pageSize={25}
      />,
      { wrapper: TooltipProvider },
    );
    expect(screen.getByText("draining")).toBeTruthy();
    expect(screen.getByText("active → draining")).toBeTruthy();
  });

  it("test_activity_renders_registration_record", () => {
    render(
      <ActivityTable
        initial={{
          items: [
            item({
              event_type: "runner_registered",
              metadata: { host_id: "runner-prod-ams-01.internal", sandbox_tier: "landlock_full" },
            }),
          ],
          total: 1,
          next_cursor: null,
        }}
        pageSize={25}
      />,
      { wrapper: TooltipProvider },
    );
    expect(screen.getByText("registered")).toBeTruthy();
    expect(screen.getByText("runner-prod-ams-01.internal · Linux · Landlock (full)")).toBeTruthy();
  });

  it("test_activity_uses_data_table", () => {
    render(
      <ActivityTable
        initial={{ items: [item({})], total: 1, next_cursor: null }}
        pageSize={25}
      />,
      { wrapper: TooltipProvider },
    );
    // The shared table structure, not bespoke markup: a real table with the
    // shared caption and column headers.
    expect(screen.getByRole("table")).toBeTruthy();
    for (const header of ["When", "What", "Detail"]) {
      expect(screen.getByRole("columnheader", { name: header })).toBeTruthy();
    }
  });
});
