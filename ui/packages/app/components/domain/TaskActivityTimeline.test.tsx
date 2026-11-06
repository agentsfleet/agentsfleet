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
