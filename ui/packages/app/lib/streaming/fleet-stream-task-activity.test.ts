import { expect, it } from "vitest";
import { applyLiveFrame } from "./fleet-stream-frames";
import { applyFinalReply } from "./fleet-stream-reply-frames";
import { evt, row } from "@/tests/helpers/fleet-stream-fixtures";
const ID = "019f0000-0000-7000-8000-000000000001";
const progress = {activity_id: ID, task_id: ID, source_admission_id: null, operation: "goal", phase: "starting",
  provider: null, model: null, completed: 0, total: null};

it("ignores unknown owners and malformed frames, deduplicates decisions and restores final saved activity", () => {
  const turns = [evt({id: "e1"}), evt({id: "e2"})];
  expect(applyLiveFrame(turns, {kind: "task_decision_started", turn_id: "missing", progress})).toBe(turns);
  expect(applyLiveFrame(turns, {kind: "task_decision_started", turn_id: "e1", progress: {}})).toBe(turns);
  const started = applyLiveFrame(turns, {kind: "task_decision_started", turn_id: "e1", progress});
  expect(applyLiveFrame(started, {kind: "task_decision_started", turn_id: "e1", progress})).toBe(started);
  const completed = {kind: "task_decision_completed" as const, progress: {...progress, phase: "recorded"}};
  const live = applyLiveFrame(started, {...completed, turn_id: "e1"});
  expect(live[0]?.taskActivity?.items).toHaveLength(2);
  const restored = applyFinalReply(turns, row({turn_id: "e1", response_text: "done", task_activity: {items: [completed], truncated: false}}));
  expect(restored[0]?.taskActivity?.items).toHaveLength(1);
  expect(restored[1]).toBe(turns[1]);
});
