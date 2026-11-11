import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Schedule } from "@/lib/api/draft-types";
import { WorkWakeups } from "./WorkWakeups";

const {refresh} = vi.hoisted(() => ({refresh: vi.fn()}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh})}));
const SCHEDULE: Schedule = {
  schedule_id: "wake-up", fleet_id: "release-watch", sync: "synced", cron: "30 7 * * *",
  timezone: "America/New_York", message: "Check the release", status: "active",
  description: "Release check", run_state: "scheduled", next_run_at: "2026-11-11T12:30:00Z",
};
beforeEach(() => {vi.useFakeTimers(); vi.setSystemTime(new Date("2026-11-11T12:00:00Z")); refresh.mockClear();});
afterEach(() => {cleanup(); vi.useRealTimers();});

it("shows the actual schedule timezone and stops promising a future run after its instant passes", async () => {
  render(<WorkWakeups schedules={[SCHEDULE]} />);
  expect(screen.getByRole("region", {name: "Wake-ups"}).textContent).toContain("07:30 AM");
  expect(screen.getByText(/America\/New_York/)).toBeTruthy();
  expect(document.querySelector("time")).toHaveProperty("dateTime", "2026-11-11T12:30:00.000Z");
  await act(async () => vi.advanceTimersByTime(31 * 60 * 1000));
  expect(document.querySelector("time")).toBeNull();
  expect(screen.getByText("Next wake-up needs an update.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Refresh schedule"}));
  expect(refresh).toHaveBeenCalledTimes(1);
});

it.each(["failed", "syncing", "unknown"])("keeps %s registration separate from a confirmed wake-up", sync => {
  render(<WorkWakeups schedules={[{...SCHEDULE, sync}]} />);
  expect(document.querySelector("time")).toBeNull();
  expect(screen.getByText(sync === "failed" ? "Registration failed. Open setup to retry." : "Scheduler registration pending.")).toBeTruthy();
});

it("shows paused and unavailable states without inventing a scheduled time", () => {
  const view = render(<WorkWakeups schedules={[{...SCHEDULE, status: "paused"}]} />);
  expect(screen.getByText("Paused")).toBeTruthy();
  expect(document.querySelector("time")).toBeNull();
  view.rerender(<WorkWakeups schedules={[{...SCHEDULE, next_run_at: null, run_state: "next_run_unavailable"}]} />);
  expect(screen.getByText("Next wake-up unavailable.")).toBeTruthy();
});

it("omits an empty schedule group", () => {
  const view = render(<WorkWakeups schedules={[]} />);
  expect(screen.queryByRole("region")).toBeNull();
  view.rerender(<WorkWakeups schedules={[{...SCHEDULE, status: "removed"}]} />);
  expect(screen.queryByRole("region")).toBeNull();
});
