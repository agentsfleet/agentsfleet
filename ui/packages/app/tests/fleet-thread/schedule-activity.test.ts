import { mockStream, renderThread, serverEvent, threadElement } from "./harness";
import { expect, it } from "vitest";
import { act, fireEvent, screen } from "@testing-library/react";
import { rowToEvent } from "@/lib/streaming/fleet-stream-row";
import { SCHEDULE_EVENT, SCHEDULE_SNAPSHOT } from "../helpers/schedule-activity";

it("keeps schedule messages and confirmed outcomes readable through a queued-to-run update", () => {
  const queued = serverEvent({...SCHEDULE_EVENT, status: "queued"});
  mockStream([rowToEvent(queued)]);
  const view = renderThread();
  const headline = "Schedule At 9:00 AM (Asia/Kolkata) fired: Review support failures — 東京 🦉";
  expect(screen.getByText(headline)).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Run details"})).toBeNull();
  expect(screen.queryByRole("button", {name: "View event"})).toBeNull();
  mockStream([rowToEvent({...queued, status: "fleet_error", failure_label: "oom_kill", failure_detail: "Memory cap reached"})]);
  view.rerender(threadElement());
  expect(screen.getByText(headline)).toBeTruthy();
  expect(screen.getByText(/Ran out of memory/)).toBeTruthy();
  expect(screen.getByRole("button", {name: "Run details"})).toBeTruthy();
  expect(view.container.querySelectorAll('[data-compact="true"]')).toHaveLength(1);
  expect(view.container.querySelector("pre")).toBeNull();
});

it("retains native run identity for every expanded schedule fire in a burst", async () => {
  const first = rowToEvent(SCHEDULE_EVENT);
  mockStream([first, {...first, id: "second-fire"}]);
  renderThread();
  expect(screen.queryByRole("button", {name: "Run details"})).toBeNull();
  await act(async () => {fireEvent.click(screen.getByRole("button", {name: /×2/, expanded: false}));});
  expect(screen.getAllByRole("button", {name: "Run details"})).toHaveLength(2);
  expect(screen.getAllByText("Completed.")).toHaveLength(3);
  expect(screen.queryByRole("button", {name: "View event"})).toBeNull();
});

it("shows a sync failure as a recorded fact without JSON, a run link, or an assistant reply", () => {
  const fact = {...SCHEDULE_SNAPSHOT, kind: "schedule_sync_failed", sync: "failed", last_error: "Scheduler refused.",
    url: "https://should-stay-private.test", diagnostic: "unapproved body"};
  mockStream([rowToEvent({...SCHEDULE_EVENT, actor: "system", event_type: "activity", status: "recorded", request_json: JSON.stringify(fact)})]);
  const {container} = renderThread();
  expect(screen.getByText(/Scheduler refused. Will retry on the next sync./)).toBeTruthy();
  expect(screen.queryByText("unapproved body")).toBeNull();
  expect(screen.queryByRole("button", {name: "Run details"})).toBeNull();
  expect(screen.queryByRole("button", {name: "View event"})).toBeNull();
  expect(container.querySelector('[data-role="assistant"]')).toBeNull();
  expect(screen.queryByRole("link")).toBeNull();
  expect(container.querySelector("pre")).toBeNull();
});

it("keeps excluded payload fields hidden when repeated schedule receipts are expanded", async () => {
  const payload = JSON.stringify({...SCHEDULE_SNAPSHOT, kind: "schedule_sync_failed", sync: "failed",
    last_error: "Scheduler refused.", diagnostic: "private diagnostic", url: "https://private.test"});
  const first = rowToEvent({...SCHEDULE_EVENT, actor: "system", event_type: "activity", status: "recorded", request_json: payload});
  mockStream([first, {...first, id: "repeated-sync-failure"}]);
  const {container} = renderThread();
  expect(screen.getByTestId("group-count").textContent).toBe("×2");
  await act(async () => {fireEvent.click(screen.getByRole("button", {name: /×2/, expanded: false}));});
  expect(screen.getAllByText(/Scheduler refused. Will retry on the next sync./)).toHaveLength(3);
  expect(screen.queryByRole("button", {name: "View event"})).toBeNull();
  expect(screen.queryByRole("button", {name: "Run details"})).toBeNull();
  expect(screen.queryByText("private diagnostic")).toBeNull();
  expect(container.querySelector("pre, a")).toBeNull();
});
