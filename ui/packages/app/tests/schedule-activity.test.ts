// @vitest-environment node
import { expect, it } from "vitest";
import { fleetActivityText, readFleetActivity } from "@/lib/events/fleet-activity";
import { triggerBodyFor } from "@/lib/events/event-summary";
import { rowToEvent } from "@/lib/streaming/fleet-stream-row";
import { SCHEDULE_EVENT, SCHEDULE_SNAPSHOT } from "./helpers/schedule-activity";

it("reloads complete schedule lifecycle facts as non-runnable readable activity", () => {
  const facts = [
    {kind: "schedule_changed", change: "created", status: "active", sync: "syncing"},
    {kind: "schedule_changed", change: "changed", status: "active", sync: "syncing"},
    {kind: "schedule_changed", change: "paused", status: "paused", sync: "syncing"},
    {kind: "schedule_changed", change: "removal_requested", status: "deleting", sync: "syncing"},
    {kind: "schedule_registered", status: "active", sync: "synced"},
    {kind: "schedule_unregistered", status: "paused", sync: "synced"},
    {kind: "schedule_sync_failed", status: "active", sync: "failed", last_error: "Scheduler refused."},
    {kind: "schedule_removed", status: "removed", sync: "synced"},
    {kind: "schedule_expired", status: "expired", sync: "synced"},
  ];
  const rows = facts.map((fact, index) => rowToEvent({...SCHEDULE_EVENT, actor: "system", event_type: "activity", status: "recorded",
    event_id: `schedule:${index}`, request_json: JSON.stringify({...SCHEDULE_SNAPSHOT, ...fact, secret: "private-diagnostic"})}));
  expect(rows.map(row => row.text)).toEqual([
    "Schedule At 9:00 AM (Asia/Kolkata) set up: Review support failures — 東京 🦉 · Registration pending",
    "Schedule At 9:00 AM (Asia/Kolkata) changed: Review support failures — 東京 🦉 · Registration pending",
    "Schedule At 9:00 AM (Asia/Kolkata) pause requested: Review support failures — 東京 🦉 · Pause requested; scheduler sync pending",
    "Schedule At 9:00 AM (Asia/Kolkata) removal requested: Review support failures — 東京 🦉 · Removal requested",
    "Schedule At 9:00 AM (Asia/Kolkata) registered with the scheduler: Review support failures — 東京 🦉 · Active",
    "Schedule At 9:00 AM (Asia/Kolkata) removed from the scheduler: Review support failures — 東京 🦉 · Paused",
    "Schedule At 9:00 AM (Asia/Kolkata) could not sync: Review support failures — 東京 🦉 · Registration pending. Scheduler refused. Will retry on the next sync.",
    "Schedule At 9:00 AM (Asia/Kolkata) removed: Review support failures — 東京 🦉 · Removed",
    "Schedule At 9:00 AM (Asia/Kolkata) expired without firing: Review support failures — 東京 🦉 · Expired without firing",
  ]);
  expect(rows.map(row => [row.role, row.reply, row.outcome, row.status])).toEqual(facts.map(() => ["system", "", "", "recorded"]));
  expect(rows.some(row => row.text.includes("private-diagnostic"))).toBe(false);
});

it("refuses malformed schedule activity without exposing unrelated payload fields", () => {
  const invalid = ["{", "null", "[]", ...[
    {kind: "future_schedule", ...SCHEDULE_SNAPSHOT},
    {kind: "schedule_changed", ...SCHEDULE_SNAPSHOT, change: "invented"},
    {kind: "schedule_registered", ...SCHEDULE_SNAPSHOT, description: 9},
    {kind: "schedule_removed", ...SCHEDULE_SNAPSHOT, schedule_id: ""},
    {kind: "schedule_sync_failed", ...SCHEDULE_SNAPSHOT},
  ].map(value => JSON.stringify({...value, secret: "must stay private"}))];
  expect(invalid.map(readFleetActivity)).toEqual(invalid.map(() => null));
  expect(invalid.map(fleetActivityText)).toEqual(invalid.map(() => "Activity recorded"));
});

it("keeps fire messages readable while distinguishing native snapshots from older payloads", () => {
  const bodies = [SCHEDULE_EVENT.request_json, JSON.stringify({message: "Check the support queue"}),
    JSON.stringify({message: "", schedule: {description: "untrusted", schedule_id: 9}}), "{", "null", "[]"];
  expect(bodies.map(request_json => triggerBodyFor({...SCHEDULE_EVENT, request_json}))).toEqual([
    "Schedule At 9:00 AM (Asia/Kolkata) fired: Review support failures — 東京 🦉", "Schedule fired: Check the support queue",
    "Schedule fired", "cron received", "cron received", "cron received",
  ]);
  const unknown = JSON.stringify({...SCHEDULE_SNAPSHOT, kind: "schedule_changed", change: "changed", status: "future_state"});
  expect(fleetActivityText(unknown)).toBe("Schedule At 9:00 AM (Asia/Kolkata) changed: Review support failures — 東京 🦉 · Status unavailable");
});
