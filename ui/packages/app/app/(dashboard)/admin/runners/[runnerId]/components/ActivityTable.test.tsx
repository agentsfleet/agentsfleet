import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { RunnerEventItem } from "@/lib/api/runners";
import { RUNNER_EVENT_TYPE } from "@/lib/api/runners-types";
import { ActivityTable } from "./ActivityTable";

vi.mock("@/lib/pagination/use-url-cursor-pages", () => ({
  useUrlCursorPages: () => ({page: 1, hasNext: false, isLoading: false, goToPage: vi.fn(), changePageSize: vi.fn()}),
}));
afterEach(cleanup);
const OCCURRED = "2026-10-17T06:32:49Z";
function item(id: string, overrides: Partial<RunnerEventItem> = {}): RunnerEventItem {
  return {id, runner_id: "runner-1", event_type: RUNNER_EVENT_TYPE.online, occurred_at: OCCURRED, metadata: {}, ...overrides};
}
function show(items: RunnerEventItem[]) {
  render(<ActivityTable initial={{items, total: null, next_cursor: null}} pageSize={25} />, {wrapper: TooltipProvider});
}

describe("runner activity history", () => {
  it("uses descriptive headings and puts the recorded time after the activity", () => {
    show([item("online")]);
    expect(screen.getAllByRole("columnheader").map((header) => header.textContent)).toEqual(["Activity", "Details", "Occurred"]);
    expect(screen.getByText("Runner connected")).toBeTruthy();
    expect(screen.getByText("Connection established.")).toBeTruthy();
    expect(screen.queryByText(/last contact/i)).toBeNull();
  });
  it("keeps work events in Leases and includes access-token changes in Activity", () => {
    show([item("acquired", {event_type: RUNNER_EVENT_TYPE.leaseAcquired}),
      item("released", {event_type: RUNNER_EVENT_TYPE.leaseReleased}),
      item("rotated", {event_type: RUNNER_EVENT_TYPE.tokenRotated})]);
    expect(screen.getAllByRole("row")).toHaveLength(2);
    expect(screen.getByText("Runner access token replaced")).toBeTruthy();
    expect(screen.queryByText("Runner connected")).toBeNull();
  });
  it("formats the server's saved state transition keys", () => {
    show([item("draining", {event_type: RUNNER_EVENT_TYPE.draining,
      metadata: {from_status: "active", to_status: "draining"}})]);
    expect(screen.getByText("Runner started draining")).toBeTruthy();
    expect(screen.getByText("Active → Draining")).toBeTruthy();
  });
  it("renders recorded registration and policy facts", () => {
    show([item("registered", {event_type: RUNNER_EVENT_TYPE.registered,
      metadata: {host_id: "runner-prod", sandbox_tier: "landlock_full"}}),
    item("policy", {event_type: RUNNER_EVENT_TYPE.policyAssigned,
      metadata: {sandbox_tier: "landlock_full", network_policy: "deny_all_egress", worker_count: 4}})]);
    expect(screen.getByText("Host: runner-prod · Isolation: Landlock")).toBeTruthy();
    expect(screen.getByText("Isolation: Landlock · Network: No egress · Workers: 4")).toBeTruthy();
  });
  it("explains missing metadata without fabricating details or hiding the event", () => {
    show([item("offline", {event_type: RUNNER_EVENT_TYPE.offline, metadata: null})]);
    expect(screen.getByText("Runner stopped responding")).toBeTruthy();
    expect(screen.getByText("No recent heartbeat received.")).toBeTruthy();
  });
  it("sorts whole-second and fractional instants by time", () => {
    show([item("first"), item("later", {event_type: RUNNER_EVENT_TYPE.offline, occurred_at: "2026-10-17T06:32:49.123Z"})]);
    const order = () => screen.getAllByRole("row").slice(1).map((row) => within(row).getAllByRole("cell")[0]?.textContent);
    fireEvent.click(screen.getByRole("button", {name: /Occurred/}));
    expect(order()).toEqual(["Runner connected", "Runner stopped responding"]);
    fireEvent.click(screen.getByRole("button", {name: /Occurred/}));
    expect(order()).toEqual(["Runner stopped responding", "Runner connected"]);
  });
  it("distinguishes an empty history from a missing read", () => {
    show([]);
    expect(screen.getByText("No lifecycle records yet")).toBeTruthy();
  });
});
