import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL } from "@/tests/helpers/draft-fixtures";
import { FleetScheduleSummary } from "./FleetScheduleSummary";

afterEach(cleanup);
const NEXT = "2026-10-20T03:30:00Z";
const SCHEDULE = {schedule_id: "scheduled", fleet_id: FLEET_DETAIL.id, timezone: "Asia/Kolkata",
  status: "active", sync: "synced", description: "At 9:00 AM (Asia/Kolkata)", run_state: "scheduled", next_run_at: NEXT};
const fleet = (schedule: unknown) => FleetDetailSchema.parse({...FLEET_DETAIL, schedules: [schedule]});

it("shows the server's schedule words and exact next instant through the shared date display", () => {
  const view = render(<FleetScheduleSummary fleet={fleet(SCHEDULE)} />, {wrapper: TooltipProvider});
  expect(screen.getByText(SCHEDULE.description)).toBeTruthy();
  expect(screen.getByText(/Next run:/)).toBeTruthy();
  expect(view.container.querySelector("time")?.dateTime).toBe("2026-10-20T03:30:00.000Z");
  expect(screen.queryByRole("button")).toBeNull();
});

it.each([
  ["paused", "Schedule paused."],
  ["activation_pending", "Schedule saved; activation is pending."],
  ["next_run_unavailable", "Next run time unavailable."],
  ["future_state", "Schedule status unavailable."],
])("uses recorded %s state without promising a stale next time", (run_state, message) => {
  const view = render(<FleetScheduleSummary fleet={fleet({...SCHEDULE, run_state})} />, {wrapper: TooltipProvider});
  expect(screen.getByText(SCHEDULE.description)).toBeTruthy();
  expect(screen.getByText(message)).toBeTruthy();
  expect(view.container.querySelector("time")).toBeNull();
});

it("keeps a missing next time explicit instead of calculating one", () => {
  render(<FleetScheduleSummary fleet={fleet({...SCHEDULE, next_run_at: null})} />);
  expect(screen.getByText("Next run time unavailable.")).toBeTruthy();
});

it.each([
  {description: undefined}, {description: ""}, {description: 3},
  {run_state: undefined}, {run_state: 3},
  {next_run_at: undefined}, {next_run_at: 3}, {next_run_at: "2026-13-01T00:00:00Z"},
])("refuses an incomplete or malformed saved schedule: %j", (change) => {
  expect(() => fleet({...SCHEDULE, ...change})).toThrow();
});
