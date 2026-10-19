import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL } from "@/tests/helpers/draft-fixtures";
import { FleetScheduleSummary } from "./FleetScheduleSummary";

afterEach(cleanup);

it("shows no schedule without building a trigger dashboard", () => {
  render(<FleetScheduleSummary fleet={FleetDetailSchema.parse(FLEET_DETAIL)} />);
  expect(screen.getByText("No schedule set.")).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.queryByText(/delivery|cron/i)).toBeNull();
});

it("does not claim a configured schedule is active when registration is absent", () => {
  const fleet = FleetDetailSchema.parse({...FLEET_DETAIL, definition: {...FLEET_DETAIL.definition,
    config: {"x-agentsfleet": {triggers: [{type: "cron", schedule: "0 9 * * *"}, {type: "webhook", source: "github"}]}}}});
  render(<FleetScheduleSummary fleet={fleet} />);
  expect(screen.getByText("Schedule saved; activation is not confirmed.")).toBeTruthy();
  expect(screen.getByText(/When github sends an event/)).toBeTruthy();
});
