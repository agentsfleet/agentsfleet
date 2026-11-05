import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { TaskActivityTimeline } from "@/components/domain/TaskActivityTimeline";
import { TASK_IDS } from "./helpers/task-reads";
afterEach(cleanup);
const progress = {activity_id: TASK_IDS[0], task_id: TASK_IDS[0], source_admission_id: null,
  operation: "capture", phase: "uploading", provider: null, model: null, completed: 1, total: 2};

it("shows only valid public activity and does not infer success from a stopped stream", () => {
  const view = render(<TaskActivityTimeline value={{items: [], truncated: false}} running={false} />);
  expect(screen.queryByRole("list")).toBeNull();
  view.rerender(<TaskActivityTimeline value={{items: [{kind: "task_transfer_progress", progress}], truncated: true}} running={false} />);
  expect(screen.getByText(/no confirmed completion recorded/)).toBeTruthy();
  expect(screen.getByText("1 of 2 objects")).toBeTruthy();
  expect(screen.getByText(/Earlier progress stages/)).toBeTruthy();
  view.rerender(<TaskActivityTimeline value={{items: [{kind: "task_transfer_progress", progress: {...progress, total: null,
    bytes: {acknowledged: 1024, missing: 2048, reused: 4096}}}], truncated: false}} running />);
  expect(screen.queryByText(/no confirmed completion recorded/)).toBeNull();
  expect(screen.queryByText("1 of 2 objects")).toBeNull();
  expect(screen.getByText(/1024 bytes acknowledged · 2048 known missing bytes · 4096 bytes reused/)).toBeTruthy();
  view.rerender(<TaskActivityTimeline value={{items: [{kind: "task_transfer_progress", progress: {...progress, phase: "saved"}}], truncated: false}} running={false} />);
  expect(screen.queryByText(/no confirmed completion recorded/)).toBeNull();
  view.rerender(<TaskActivityTimeline value={null} running={false} />);
  expect(screen.queryByRole("list")).toBeNull();
});
