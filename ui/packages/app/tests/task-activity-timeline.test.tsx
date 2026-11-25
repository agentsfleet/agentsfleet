import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { DurableTaskActivityTimeline } from "@/components/domain/tasks/TaskActivityTimeline";
import { DURABLE_TASK_IDS } from "./helpers/task-reads";
afterEach(cleanup);
const progress = {activity_id: DURABLE_TASK_IDS[0], durable_task_id: DURABLE_TASK_IDS[0], source_turn_id: null,
  operation: "capture", phase: "uploading", provider: null, model: null, completed: 1, total: 2};

it("shows only valid public activity and does not infer success from a stopped stream", () => {
  const view = render(<DurableTaskActivityTimeline value={{items: []}} running={false} />);
  expect(screen.queryByRole("list")).toBeNull();
  view.rerender(<DurableTaskActivityTimeline value={{items: [{kind: "durable_task_transfer_progress", progress}]}} running={false} />);
  expect(screen.getByText(/no confirmed completion recorded/)).toBeTruthy();
  expect(screen.getByText("1 of 2 objects")).toBeTruthy();
  view.rerender(<DurableTaskActivityTimeline value={{items: [{kind: "durable_task_transfer_progress", progress: {...progress, total: null,
    bytes: {acknowledged: 1024, missing: 2048, reused: 4096}}}]}} running />);
  expect(screen.queryByText(/no confirmed completion recorded/)).toBeNull();
  expect(screen.queryByText("1 of 2 objects")).toBeNull();
  expect(screen.getByText(/1024 bytes acknowledged · 2048 known missing bytes · 4096 bytes reused/)).toBeTruthy();
  view.rerender(<DurableTaskActivityTimeline value={{items: [{kind: "durable_task_transfer_progress", progress: {...progress, phase: "saved"}}]}} running={false} />);
  expect(screen.queryByText(/no confirmed completion recorded/)).toBeNull();
  view.rerender(<DurableTaskActivityTimeline value={null} running={false} />);
  expect(screen.queryByRole("list")).toBeNull();
});
