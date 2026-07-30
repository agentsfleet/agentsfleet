import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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

// Two distinct instants whose order — not magnitude — the sort test pins.
const OLDER_INSTANT_MS = 1_000;
const NEWER_INSTANT_MS = 2_000;

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

  it("should degrade detail honestly — absent metadata, host-only registration, unknown tier tag", () => {
    render(
      <ActivityTable
        initial={{
          items: [
            item({ id: "bare", event_type: "runner_online", metadata: null }),
            item({ id: "host-only", event_type: "runner_registered", metadata: { host_id: "host-x" } }),
            item({
              id: "future-tier",
              event_type: "runner_registered",
              metadata: { host_id: "host-y", sandbox_tier: "quantum_jail" },
            }),
          ],
          total: 3,
          next_cursor: null,
        }}
        pageSize={25}
      />,
      { wrapper: TooltipProvider },
    );
    // Host-only registration renders the host with no dangling separator.
    expect(screen.getByText("host-x")).toBeTruthy();
    // A tier tag minted after this build renders its raw spelling, not nothing.
    expect(screen.getByText(/quantum_jail/)).toBeTruthy();
  });

  it("should sort by When through the standard header control, on an uncounted feed", () => {
    render(
      <ActivityTable
        initial={{
          items: [
            item({ id: "older", event_type: "runner_online", occurred_at: OLDER_INSTANT_MS }),
            item({ id: "newer", event_type: "runner_offline", occurred_at: NEWER_INSTANT_MS }),
          ],
          // total unknown (null) — the pager renders without a fabricated count.
          total: null,
          next_cursor: null,
        }}
        pageSize={25}
      />,
      { wrapper: TooltipProvider },
    );
    const orderOf = () =>
      screen
        .getAllByRole("row")
        .slice(1)
        .map((row) => (row.textContent?.includes("came online") ? "older" : "newer"));
    fireEvent.click(screen.getByRole("button", { name: /when/i }));
    const firstSort = orderOf();
    fireEvent.click(screen.getByRole("button", { name: /when/i }));
    // Two clicks walk both directions of the same comparator: the order must
    // invert, proving the column sorts on the record's real timestamp.
    expect(orderOf()).toEqual([...firstSort].reverse());
  });
});
