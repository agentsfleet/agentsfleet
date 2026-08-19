import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { formatTimeAbsolute, TooltipProvider } from "@agentsfleet/design-system";
import {
  RUNNER_LAST_SEEN_NEVER,
  RUNNER_LIFECYCLE_EVENT_TYPES,
  type RunnerEventItem,
} from "@/lib/api/runners";

// The wrapper stands in for a real ancestor, not a missing one: the app mounts
// exactly one TooltipProvider in `app/layout.tsx`, above every route group, and
// the relative `Time` cell here reads it from there. That the root mounts it at
// all is guarded once, in `app/layout.test.tsx` — not re-litigated per island.

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
      />, { wrapper: TooltipProvider });
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
      />, { wrapper: TooltipProvider });
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
      />, { wrapper: TooltipProvider });
    expect(screen.getByText("registered")).toBeTruthy();
    expect(screen.getByText("runner-prod-ams-01.internal · Landlock")).toBeTruthy();
  });

  it("test_activity_uses_data_table", () => {
    render(
      <ActivityTable
        initial={{ items: [item({})], total: 1, next_cursor: null }}
        pageSize={25}
      />, { wrapper: TooltipProvider });
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
      />, { wrapper: TooltipProvider });
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
      />, { wrapper: TooltipProvider });
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

  // ── Detail for online/offline records ────────────────────────────────────
  // These two event types are the bulk of any runner's history and rendered a
  // BLANK detail cell, because the reader only understood string metadata and
  // both writers store `last_seen_at` as a JSON number.

  it("states the real last-contact instant on a went-offline record", () => {
    // The record's own timestamp is when the sweeper NOTICED — three lease
    // TTLs after the runner actually went quiet. The detail carries the honest
    // instant, so the row cannot be read as "it died at 10:01:30".
    const WENT_DARK_MS = Date.UTC(2026, 6, 24, 10, 0, 0);
    const SWEEPER_NOTICED_MS = WENT_DARK_MS + 90_000;
    render(
      <ActivityTable
        initial={{
          items: [
            item({
              id: "offline-1",
              event_type: "runner_offline",
              occurred_at: SWEEPER_NOTICED_MS,
              metadata: { last_seen_at: WENT_DARK_MS },
            }),
          ],
          total: 1,
          next_cursor: null,
        }}
        pageSize={25}
      />, { wrapper: TooltipProvider });
    expect(screen.getByText("went offline")).toBeTruthy();
    expect(
      screen.getByText(`last contact ${formatTimeAbsolute(new Date(WENT_DARK_MS))}`),
    ).toBeTruthy();
  });

  it("states when a recovered runner was last heard from before it came back", () => {
    const LAST_HEARD_MS = Date.UTC(2026, 6, 24, 10, 0, 0);
    render(
      <ActivityTable
        initial={{
          items: [
            item({
              id: "online-1",
              event_type: "runner_online",
              occurred_at: LAST_HEARD_MS + 3_600_000,
              metadata: { last_seen_at: LAST_HEARD_MS },
            }),
          ],
          total: 1,
          next_cursor: null,
        }}
        pageSize={25}
      />, { wrapper: TooltipProvider });
    expect(
      screen.getByText(`last contact ${formatTimeAbsolute(new Date(LAST_HEARD_MS))}`),
    ).toBeTruthy();
  });

  it("names the never-contacted sentinel instead of rendering the epoch", () => {
    // A runner minted but never heard from carries last_seen_at = 0. Formatting
    // that as an instant would print 1 Jan 1970 — a lie dressed as precision.
    render(
      <ActivityTable
        initial={{
          items: [
            item({
              id: "first-1",
              event_type: "runner_online",
              metadata: { last_seen_at: RUNNER_LAST_SEEN_NEVER },
            }),
          ],
          total: 1,
          next_cursor: null,
        }}
        pageSize={25}
      />, { wrapper: TooltipProvider });
    expect(screen.getByText("first contact")).toBeTruthy();
    expect(screen.queryByText(/1970/)).toBeNull();
  });

  it("leaves the detail empty when a record carries no metadata it understands", () => {
    // The default arm still has to be silent rather than inventive: a future
    // event type with unrecognised metadata renders nothing, not a guess.
    render(
      <ActivityTable
        initial={{
          items: [
            item({
              id: "bare-1",
              event_type: "runner_drained",
              metadata: { something_else: "value" },
            }),
          ],
          total: 1,
          next_cursor: null,
        }}
        pageSize={25}
      />, { wrapper: TooltipProvider });
    expect(screen.getByText("drained")).toBeTruthy();
    expect(screen.queryByText(/last contact/)).toBeNull();
    expect(screen.queryByText(/first contact/)).toBeNull();
  });
});
