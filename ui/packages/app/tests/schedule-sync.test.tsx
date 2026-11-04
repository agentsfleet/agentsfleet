import type { Schedule } from "@/lib/api/draft-types";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ScheduleSync } from "@/components/domain/chat/ScheduleSync";
import { FAILED_SCHEDULE, PENDING_SCHEDULE, SCHEDULE } from "./helpers/draft-fixtures";

afterEach(cleanup);

it("uses current schedule state and leaves retry under the caller's busy state", () => {
  const retry = vi.fn();
  const view = render(<ScheduleSync schedules={[SCHEDULE]} busy={false} onRetry={retry} />);
  expect(screen.getByText("At 9:00 AM (Asia/Kolkata) · Active")).toBeTruthy();
  expect(screen.getByText(SCHEDULE.message)).toBeTruthy();
  expect(view.container.querySelector("time")?.dateTime).toBe("2026-11-05T03:30:00.000Z");
  expect(screen.queryByRole("button")).toBeNull();
  view.rerender(<ScheduleSync schedules={[{...FAILED_SCHEDULE, last_error: "Scheduler refused."}]} busy onRetry={retry} />);
  expect(screen.getByText("Scheduler refused. Will retry on the next sync.")).toBeTruthy();
  expect(view.container.querySelector("time")).toBeNull();
  const button = screen.getByRole("button", {name: "Retry registration"});
  expect(button).toHaveProperty("disabled", true);
  fireEvent.click(button); expect(retry).not.toHaveBeenCalled();
  view.rerender(<ScheduleSync schedules={[PENDING_SCHEDULE]} busy={false} onRetry={retry} />);
  fireEvent.click(screen.getByRole("button", {name: "Retry registration"}));
  expect(retry).toHaveBeenCalledWith(PENDING_SCHEDULE);
  expect(screen.queryByRole("alert")).toBeNull();
});

it("keeps pause, removal, unknown and incomplete timing states non-predictive", () => {
  const retry = vi.fn();
  const cases: {schedule: Schedule; state: string; action: string | null}[] = [
    {schedule: {...SCHEDULE, status: "paused", run_state: "paused", next_run_at: null}, state: "Paused", action: null},
    {schedule: {...PENDING_SCHEDULE, status: "paused", run_state: "paused"}, state: "Pause requested; scheduler sync pending", action: "Retry sync"},
    {schedule: {...PENDING_SCHEDULE, status: "deleting", run_state: "next_run_unavailable"}, state: "Removal requested", action: "Retry sync"},
    {schedule: {...SCHEDULE, status: "future_state", run_state: "future_state", next_run_at: null}, state: "Status unavailable", action: null},
    // A partial timing response cannot invent the instant the native writer omitted.
    {schedule: {...SCHEDULE, next_run_at: null}, state: "Active", action: null},
  ];
  const view = render(<ScheduleSync schedules={[]} busy={false} onRetry={retry} />);
  expect(screen.queryByRole("region", {name: "Schedules"})).toBeNull();
  for (const {schedule, state, action} of cases) {
    view.rerender(<ScheduleSync schedules={[schedule]} busy={false} onRetry={retry} />);
    expect(screen.getByText(`At 9:00 AM (Asia/Kolkata) · ${state}`)).toBeTruthy();
    expect(screen.getByText(SCHEDULE.message)).toBeTruthy();
    expect(view.container.querySelector("time")).toBeNull();
    const button = screen.queryByRole("button");
    if (action) expect(button?.textContent).toBe(action);
    else expect(button).toBeNull();
    expect(screen.queryByText(/Save the fleet to activate it/)).toBeNull();
  }
  expect(retry).not.toHaveBeenCalled();
});
