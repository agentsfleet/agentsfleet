import type { TaskActivity, TaskActivityHistory } from "./task-activity";
import { describe, expect, it } from "vitest";
import { addTaskActivity, latestTaskActivity, mergeTaskActivity, readTaskActivityHistory, taskActivityLabel } from "./task-activity";
import { applyLiveFrame, mergeBackfill } from "./streaming/fleet-stream-frames";
import { evt, row } from "@/tests/helpers/fleet-stream-fixtures";

const ID = "019f0000-0000-7000-8000-000000000001";

function stage(phase: TaskActivity["progress"]["phase"], completed = 0): TaskActivity {
  return {kind: "task_transfer_progress", progress: {activity_id: ID, task_id: ID, source_admission_id: null,
    operation: "capture", phase, provider: null, model: null, completed, total: 10}};
}

describe("task progress across stream loss", () => {
  it("restores progress on a running event without replacing its streamed answer", () => {
    const live = evt({id: "e1", reply: "Working on the repair", taskActivity: {items: [stage("uploading", 8)], truncated: false}});
    const detail = row({status: "received", task_activity: {items: [stage("preparing"), stage("uploading", 3)], truncated: false}});
    const merged = mergeBackfill([live], [detail]);
    expect(merged[0]?.reply).toBe("Working on the repair");
    expect(merged[0]?.taskActivity?.items.map(item => item.progress.completed)).toEqual([0, 8]);
    expect(mergeBackfill(merged, [detail])).toBe(merged);
  });

  it("keeps a saved completion above delayed progress and preserves native task status", () => {
    const history: TaskActivityHistory = {items: [stage("saved", 10)], truncated: false};
    const delayed = addTaskActivity(history, stage("uploading", 4));
    expect(latestTaskActivity(delayed).map(item => item.progress.phase)).toEqual(["saved"]);
    const live = evt({id: "e1", taskActivity: history});
    const changed = applyLiveFrame([live], {...stage("verifying", 10), event_id: "e1"});
    expect(changed[0]?.status).toBe("received");
    expect(taskActivityLabel(stage("saved"))).toBe("Recovery point saved");
  });

  it("rejects malformed progress and tells the reader when the bounded window omits steps", () => {
    expect(readTaskActivityHistory({items: [stage("saved", -1)], truncated: false})).toBeUndefined();
    expect(readTaskActivityHistory({items: [{...stage("saved"), progress: {...stage("saved").progress, activity_id: "-".repeat(36)}}], truncated: false})).toBeUndefined();
    let history: TaskActivityHistory | undefined;
    for (let i = 0; i < 130; i++) {
      const next = stage("preparing");
      next.progress.activity_id = `019f0000-0000-7000-8000-${i.toString(16).padStart(12, "0")}`;
      history = addTaskActivity(history, next);
    }
    expect(history?.items).toHaveLength(128);
    expect(history?.truncated).toBe(true);
    expect(mergeTaskActivity(history, {items: [], truncated: false})?.truncated).toBe(true);
    expect(mergeTaskActivity(history, undefined)).toBe(history);
  });
});
