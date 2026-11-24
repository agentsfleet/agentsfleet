import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { afterEach, expect, it } from "vitest";
import { SCHEDULE_RUN_STATE } from "@/lib/api/draft-vocabulary";
import { SCHEDULE } from "@/tests/helpers/draft-fixtures";
import { ScheduledSetupSummary } from "./ScheduledSetupSummary";

afterEach(cleanup);

it("loads the saved Summary for a paused wake-up", async () => {
  const paused = {...SCHEDULE, status: "paused" as const, run_state: SCHEDULE_RUN_STATE.paused, next_run_at: null};
  render(<ScheduledSetupSummary ready schedules={[paused]} />, {wrapper: TooltipProvider});
  expect(await screen.findByText("Paused")).toBeTruthy();
  expect(screen.getByText("Next wake-up")).toBeTruthy();
});
