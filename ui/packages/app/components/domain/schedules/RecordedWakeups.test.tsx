import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Schedule } from "@/lib/api/draft-types";
import {RecordedWakeups} from "./RecordedWakeups";
import { TooltipProvider } from "@agentsfleet/design-system";

const {refresh} = vi.hoisted(() => ({refresh: vi.fn()}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh})}));
const SCHEDULE: Schedule = {
  schedule_id: "wake-up", fleet_id: "release-watch", sync: "synced", cron: "30 7 * * *",
  timezone: "America/New_York", message: "Check the release", status: "active",
  description: "Release check", run_state: "scheduled", next_run_at: "2026-11-11T12:30:00Z",
};
beforeEach(() => {vi.useFakeTimers(); vi.setSystemTime(new Date("2026-11-11T12:00:00Z")); refresh.mockClear();});
afterEach(() => {cleanup(); vi.useRealTimers();});

it("keeps a future local date until midnight, then shows only its time", async () => {
  vi.setSystemTime(new Date("2026-11-11T23:59:00Z"));
  render(<RecordedWakeups workspace="workspace" schedules={[{...SCHEDULE, next_run_at: "2026-11-12T00:10:00Z"}]} />, {wrapper: TooltipProvider});
  expect(document.querySelector("time")?.textContent).toContain("Nov 12");
  await act(async () => vi.advanceTimersByTime(60_000));
  expect(document.querySelector("time")?.textContent).toBe("12:10 AM");
  expect(document.querySelector("time")).toHaveProperty("dateTime", "2026-11-12T00:10:00.000Z");
});

it("shows the reader’s time instead of the stored schedule zone and stops promising a future run after its instant passes", async () => {
  render(<RecordedWakeups workspace="workspace" schedules={[SCHEDULE]} />, {wrapper: TooltipProvider});
  expect(screen.getByRole("group", {name: "Release check"}).textContent).toContain("12:30 PM");
  expect(screen.queryByText(/America\/New_York/)).toBeNull();
  expect(document.querySelector("time")).toHaveProperty("dateTime", "2026-11-11T12:30:00.000Z");
  await act(async () => vi.advanceTimersByTime(31 * 60 * 1000));
  expect(document.querySelector("time")).toBeNull();
  expect(screen.getByText("Next wake-up needs an update.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", {name: "Refresh schedule"}));
  expect(refresh).toHaveBeenCalledTimes(1);
});

it.each(["failed", "syncing", "unknown"])("keeps %s registration separate from a confirmed wake-up", sync => {
  render(<RecordedWakeups workspace="workspace" schedules={[{...SCHEDULE, sync}]} />);
  expect(document.querySelector("time")).toBeNull();
  expect(screen.getByText(sync === "failed" ? "Registration failed." : "Scheduler registration pending.")).toBeTruthy();
});

it("shows paused and unavailable states without inventing a scheduled time", () => {
  const view = render(<RecordedWakeups workspace="workspace" schedules={[{...SCHEDULE, status: "paused"}]} />);
  expect(screen.getByText("Paused")).toBeTruthy();
  expect(document.querySelector("time")).toBeNull();
  view.rerender(<RecordedWakeups workspace="workspace" schedules={[{...SCHEDULE, next_run_at: null, run_state: "next_run_unavailable"}]} />);
  expect(screen.getByText("Next wake-up unavailable.")).toBeTruthy();
  view.rerender(<RecordedWakeups workspace="workspace" schedules={[{...SCHEDULE, next_run_at: null}]} />);
  expect(screen.getByText("Next wake-up unavailable.")).toBeTruthy();
  expect(document.querySelector("time")).toBeNull();
});

it("omits an empty schedule group", () => {
  const view = render(<RecordedWakeups workspace="workspace" schedules={[]} />);
  expect(screen.queryByRole("group")).toBeNull();
  view.rerender(<RecordedWakeups workspace="workspace" schedules={[{...SCHEDULE, status: "removed"}]} />);
  expect(screen.queryByRole("group")).toBeNull();
});

it("shows an unknown lifecycle state without promising another wake-up", () => {
  render(<RecordedWakeups workspace="workspace" schedules={[{...SCHEDULE, status: "unknown"}]} />);
  expect(screen.getByText("Status unavailable")).toBeTruthy();
  expect(document.querySelector("time")).toBeNull();
});

it("shows attention before the nearest wake-up and retains additional schedule details", () => {
  const earlier = {...SCHEDULE, schedule_id: "earlier", description: "Earlier check", next_run_at: "2026-11-11T12:15:00Z"};
  const failed = {...SCHEDULE, schedule_id: "failed", sync: "failed", description: "Unregistered check"};
  const view = render(<RecordedWakeups workspace="workspace" schedules={[SCHEDULE, earlier, failed]} />, {wrapper: TooltipProvider});
  const first = view.container.querySelector("fieldset");
  expect(first?.textContent).toContain("Registration failed");
  expect(first?.getAttribute("aria-label")).toBe("Unregistered check");
  fireEvent.click(screen.getByRole("button", {name: "2 more wake-ups"}));
  expect(screen.getByRole("group", {name: "Earlier check"})).toBeTruthy();
  view.rerender(<RecordedWakeups workspace="workspace" schedules={[SCHEDULE, earlier]} />);
  expect(view.container.querySelector("fieldset time")?.getAttribute("datetime")).toBe("2026-11-11T12:15:00.000Z");
  expect(view.container.querySelector("fieldset")?.textContent).not.toContain("UTC");
  view.rerender(<RecordedWakeups workspace="workspace" schedules={[{...SCHEDULE, schedule_id: "missing", next_run_at: null}, SCHEDULE]} />);
  expect(view.container.querySelector("fieldset time")?.getAttribute("datetime")).toBe("2026-11-11T12:30:00.000Z");
  view.rerender(<RecordedWakeups workspace="workspace" schedules={[SCHEDULE, {...SCHEDULE, schedule_id: "missing", next_run_at: null}]} />);
  expect(view.container.querySelector("fieldset time")?.getAttribute("datetime")).toBe("2026-11-11T12:30:00.000Z");
});

it.each([
  ["paused", "syncing", "Pause pending."], ["paused", "failed", "Pause failed."],
  ["deleting", "syncing", "Removal pending."], ["deleting", "failed", "Removal failed."],
])("describes %s intent accurately while sync is %s", (status, sync, label) => {
  render(<RecordedWakeups workspace="workspace" schedules={[{...SCHEDULE, status, sync}]} />);
  expect(screen.getByText(label)).toBeTruthy();
  expect(document.querySelector("time")).toBeNull();
  expect(screen.queryByText(/registration/i)).toBeNull();
});
