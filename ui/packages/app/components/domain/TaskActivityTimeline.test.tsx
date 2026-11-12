import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TaskActivityTimeline } from "./TaskActivityTimeline";

afterEach(cleanup);

it("renders expandable public progress and labels an interrupted ending honestly", () => {
  const progress = {activity_id: "019f0000-0000-7000-8000-000000000001", task_id: null, source_admission_id: null,
    operation: "goal", phase: "starting", provider: "openai", model: "selected-model", completed: 0, total: null};
  const {container, rerender} = render(<TaskActivityTimeline running={false}
    value={{items: [{kind: "task_decision_started", progress}], truncated: true}} />);
  expect(container.querySelector("summary")?.textContent).toContain("no confirmed completion recorded");
  expect(screen.getAllByText(/selected-model/u).length).toBeGreaterThan(0);
  expect(screen.getByText(/Earlier progress/u)).toBeTruthy();
  expect(container.querySelector("details")?.open).toBe(false);
  rerender(<TaskActivityTimeline running value={{items: [], truncated: false}} />);
  expect(container.textContent).toBe("");
});

it("keeps pending, failed and uncertain assessment visible while routine completion moves into details", () => {
  const progress = {activity_id: "019f0000-0000-7000-8000-000000000001", task_id: null, source_admission_id: null,
    operation: "goal", phase: "recorded", provider: null, model: null, completed: 1, total: 1};
  const history = (phase: string) => ({items: [{kind: "task_decision_completed", progress: {...progress, phase}}], truncated: false});
  const view = render(<TaskActivityTimeline value={history("recorded")} running={false} attentionOnly />);
  expect(view.container.textContent).toBe("");
  for (const phase of ["starting", "failed", "unavailable"]) {
    view.rerender(<TaskActivityTimeline value={history(phase)} running={false} attentionOnly />);
    expect(view.container.querySelector("summary")).toBeTruthy();
  }
  view.rerender(<TaskActivityTimeline value={history("recorded")} running attentionOnly />);
  expect(view.container.querySelector("summary")).toBeTruthy();
});
