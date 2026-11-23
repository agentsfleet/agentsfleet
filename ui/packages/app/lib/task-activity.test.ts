import type { DurableTaskActivity, DurableTaskActivityHistory } from "./task-activity";
import { describe, expect, it } from "vitest";
import { addDurableTaskActivity, latestDurableTaskActivity, mergeDurableTaskActivity, readDurableTaskActivityHistory, durableTaskActivityLabel } from "./task-activity";
import { applyLiveFrame, mergeBackfill } from "./streaming/fleet-stream-frames";
import { evt, row } from "@/tests/helpers/fleet-stream-fixtures";

const ID = "019f0000-0000-7000-8000-000000000001";

function stage(phase: DurableTaskActivity["progress"]["phase"], completed = 0): DurableTaskActivity {
  return {kind: "durable_task_transfer_progress", progress: {activity_id: ID, durable_task_id: ID, source_turn_id: null,
    operation: "capture", phase, provider: null, model: null, completed, total: 10}};
}

describe("task progress across stream loss", () => {
  it("does not let an older live window evict newer saved progress", () => {
    const window = (start: number): DurableTaskActivityHistory => ({items: Array.from({length: 128}, (_, offset) => {
      const item = stage("uploading");
      item.progress.activity_id = `019f0000-0000-7000-8000-${(start + offset).toString(16).padStart(12, "0")}`;
      return item;
    }), truncated: true});
    const saved = window(128);
    expect(mergeDurableTaskActivity(window(0), saved)).toEqual(saved);
  });

  it("uses server observation order even when operation UUIDs run backward", () => {
    const later = stage("preparing");
    later.progress.sequence = 20;
    const earlier = stage("saved", 10);
    earlier.progress.activity_id = "019f0000-0000-7000-8000-ffffffffffff";
    earlier.progress.sequence = 10;
    const merged = mergeDurableTaskActivity({items: [later], truncated: false}, {items: [earlier], truncated: false});
    expect(merged?.items.map(item => item.progress.sequence)).toEqual([10, 20]);
    const stale = {...later, progress: {...later.progress, sequence: 19}};
    expect(addDurableTaskActivity(merged, stale)).toBe(merged);
    expect(readDurableTaskActivityHistory({items: [{...later, progress: {...later.progress, sequence: -1}}], truncated: false})).toBeUndefined();
  });

  it("retains a historical live tail only after a shared saved anchor", () => {
    const preparing = stage("preparing");
    const uploading = stage("uploading", 3);
    const saved = stage("saved", 10);
    const merged = mergeDurableTaskActivity({items: [uploading, saved], truncated: false}, {items: [preparing, uploading], truncated: false});
    expect(merged?.items.map(item => item.progress.phase)).toEqual(["preparing", "uploading", "saved"]);
  });

  it("preserves acknowledged bytes when a delayed frame has the same object count", () => {
    const latest = stage("uploading", 1);
    latest.progress.bytes = {acknowledged: 4096, missing: 2048, reused: 1024};
    const earlier = stage("uploading", 1);
    earlier.progress.bytes = {acknowledged: 1024, missing: 5120, reused: 1024};
    const history = {items: [latest], truncated: false};
    expect(addDurableTaskActivity(history, earlier)).toBe(history);
    expect(readDurableTaskActivityHistory({items: [{...latest, progress: {...latest.progress,
      bytes: {acknowledged: -1, missing: 0, reused: 0}}}], truncated: false})).toBeUndefined();
  });
  it("restores progress on a running turn without replacing its streamed answer", () => {
    const live = evt({id: "e1", reply: "Working on the repair", durableTaskActivity: {items: [stage("uploading", 8)], truncated: false}});
    const detail = row({status: "received", durable_task_activity: {items: [stage("preparing"), stage("uploading", 3)], truncated: false}});
    const merged = mergeBackfill([live], [detail]);
    expect(merged[0]?.reply).toBe("Working on the repair");
    expect(merged[0]?.durableTaskActivity?.items.map(item => item.progress.completed)).toEqual([0, 8]);
    expect(mergeBackfill(merged, [detail])).toBe(merged);
  });

  it("keeps a saved completion above delayed progress and preserves native task status", () => {
    const history: DurableTaskActivityHistory = {items: [stage("saved", 10)], truncated: false};
    const delayed = addDurableTaskActivity(history, stage("uploading", 4));
    expect(latestDurableTaskActivity(delayed).map(item => item.progress.phase)).toEqual(["saved"]);
    const live = evt({id: "e1", durableTaskActivity: history});
    const changed = applyLiveFrame([live], {...stage("verifying", 10), turn_id: "e1"});
    expect(changed[0]?.status).toBe("received");
    expect(durableTaskActivityLabel(stage("saved"))).toBe("Recovery point saved");
  });

  it("rejects malformed progress and keeps every live stage past the retired 128 window", () => {
    expect(readDurableTaskActivityHistory({items: [stage("saved", -1)], truncated: false})).toBeUndefined();
    expect(readDurableTaskActivityHistory({items: [{...stage("saved"), progress: {...stage("saved").progress, activity_id: "-".repeat(36)}}], truncated: false})).toBeUndefined();
    let history: DurableTaskActivityHistory | undefined;
    for (let i = 0; i < 130; i++) {
      const next = stage("preparing");
      next.progress.activity_id = `019f0000-0000-7000-8000-${i.toString(16).padStart(12, "0")}`;
      history = addDurableTaskActivity(history, next);
    }
    expect(history?.items).toHaveLength(130);
    expect(history?.truncated).toBe(false);
    expect(readDurableTaskActivityHistory(history)).toEqual(history);
    expect(mergeDurableTaskActivity(history, {items: [], truncated: false})).toBe(history);
    expect(mergeDurableTaskActivity(history, undefined)).toBe(history);
  });

  it("reads values past the retired caps and still refuses an unsafe integer", () => {
    const past = stage("uploading", 200_001);
    past.progress = {...past.progress, total: 200_002, provider: "p".repeat(257), model: "m".repeat(257),
      bytes: {acknowledged: 68_719_476_737, missing: 0, reused: 0}};
    expect(readDurableTaskActivityHistory({items: [past], truncated: false})?.items).toEqual([past]);
    expect(readDurableTaskActivityHistory({items: [stage("uploading", 2 ** 53)], truncated: false})).toBeUndefined();
  });
});

it("keeps authoritative observations above legacy arrivals and carries truncation across a reconnect", () => {
  const legacy = stage("preparing");
  const newer = stage("uploading", 2);
  newer.progress.sequence = 2;
  const withSequence = addDurableTaskActivity({items: [newer], truncated: false}, legacy);
  expect(addDurableTaskActivity({items: [legacy], truncated: false}, newer)).toEqual(withSequence);
  expect(withSequence.items.map(item => item.progress.phase)).toEqual(["preparing", "uploading"]);
  expect(addDurableTaskActivity(withSequence, {...newer, progress: {...newer.progress, sequence: undefined}})).toBe(withSequence);
  const saved = {items: [legacy], truncated: false};
  expect(mergeDurableTaskActivity(undefined, saved)).toBe(saved);
  expect(mergeDurableTaskActivity({items: [legacy], truncated: true}, saved)?.truncated).toBe(true);
  const goal = {...legacy, progress: {...legacy.progress, operation: "goal" as const, phase: "recorded" as const, provider: "openai", model: "selected"}};
  expect(durableTaskActivityLabel(goal)).toBe("Assessment recorded · goal progress · openai / selected");
  expect(durableTaskActivityLabel({...goal, progress: {...goal.progress, operation: "intent", provider: null}})).toBe("Assessment recorded · request intent ·  / selected");
});
