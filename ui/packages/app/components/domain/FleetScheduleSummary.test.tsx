import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL } from "@/tests/helpers/draft-fixtures";
import { FleetScheduleSummary } from "./FleetScheduleSummary";

afterEach(cleanup);
const SCHEDULE = {schedule_id: "scheduled", fleet_id: FLEET_DETAIL.id, timezone: "Asia/Kolkata", status: "active", sync: "synced"};

it("shows no schedule without building a trigger dashboard", () => {
  render(<FleetScheduleSummary fleet={FleetDetailSchema.parse(FLEET_DETAIL)} />);
  expect(screen.getByText("No schedule set.")).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.queryByText(/delivery|cron/i)).toBeNull();
});

it.each([
  {status: "active", sync: "synced", message: "Schedule set. Next run time unavailable."},
  {status: "paused", sync: "synced", message: "Schedule paused."},
  {status: "deleting", sync: "syncing", message: "Schedule is being removed."},
  {status: "active", sync: "failed", message: "Schedule saved, but activation is not confirmed."},
  {status: "active", sync: "syncing", message: "Schedule saved, but activation is not confirmed."},
  {status: "unknown", sync: "synced", message: "Schedule status unavailable."},
])("uses recorded schedule state without inventing the next run: $status $sync", ({status, sync, message}) => {
  const fleet = FleetDetailSchema.parse({...FLEET_DETAIL, schedules: [{...SCHEDULE, status, sync}]});
  render(<FleetScheduleSummary fleet={fleet} />);
  expect(screen.getByText(message, {exact: false})).toBeTruthy();
  expect(screen.getByText(`Timezone: ${SCHEDULE.timezone}.`)).toBeTruthy();
  expect(screen.queryByText("No schedule set.")).toBeNull();
});

it("does not claim a configured schedule is active when registration is absent", () => {
  const fleet = FleetDetailSchema.parse({...FLEET_DETAIL, definition: {...FLEET_DETAIL.definition,
    config: {"x-agentsfleet": {triggers: [{type: "cron", schedule: "0 9 * * *"}, {type: "webhook", source: "github"}]}}}});
  render(<FleetScheduleSummary fleet={fleet} />);
  expect(screen.getByText("Schedule saved; activation is not confirmed.")).toBeTruthy();
  expect(screen.getByText(/When github sends an event/)).toBeTruthy();
});
