import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { TaskRepositoryProgress } from "@/components/domain/chat/TaskRepositoryProgress";
import { detail, INSTANT, TASK_IDS } from "./helpers/task-reads";

const HEAD = "a".repeat(40);
const PUBLISHED = "b".repeat(40);

afterEach(cleanup);

it("distinguishes active dependency, consumed check evidence, and confirmed publication", () => {
  const task = detail();
  task.context.target = {kind: "existing_pr", repository: "agentsfleet/agentsfleet", number: 803,
    branch: "feat/m236-durable-tasks", head: HEAD, tree: HEAD, url: "https://github.com/agentsfleet/agentsfleet/pull/803",
    workflows: [{id: 1, path: ".github/workflows/check.yml", digest: "workflow-digest"}]};
  task.dependency = {active: true, head: HEAD, updated_at: INSTANT, checks: [{workflow_id: 1,
    run_id: 10, run_number: 3, run_attempt: 2, status: "completed", conclusion: "success"}]};
  task.publication = {effect_id: TASK_IDS[0], ordinal: 1, state: "unknown", expected_head: HEAD,
    tested_tree: HEAD, commit: PUBLISHED, pull_request_url: task.context.target.url};
  const {rerender} = render(<TaskRepositoryProgress detail={task} />);
  expect(screen.getByText("Waiting for reviewed checks")).toBeTruthy();
  expect(screen.getByText(/Publication unknown · no confirmed published head/)).toBeTruthy();
  expect(screen.queryByText(PUBLISHED)).toBeNull();
  rerender(<TaskRepositoryProgress detail={{...task, dependency: {...task.dependency, active: false},
    publication: {...task.publication, state: "confirmed"}}} />);
  expect(screen.queryByText("Waiting for reviewed checks")).toBeNull();
  expect(screen.getByText("Saved check results")).toBeTruthy();
  expect(screen.getByText(PUBLISHED)).toBeTruthy();
  expect(screen.getByText(/completed · success/)).toBeTruthy();
});

it("keeps absent and incomplete repository evidence explicit", () => {
  const task = detail();
  const view = render(<TaskRepositoryProgress detail={task} />);
  expect(screen.queryByText(/Confirmed repository head/)).toBeNull();
  task.context.target = {kind: "api_only"};
  view.rerender(<TaskRepositoryProgress detail={{...task}} />);
  expect(screen.queryByText(/Confirmed repository head/)).toBeNull();
  task.context.target = {kind: "existing_pr", repository: "a/b", number: 1, branch: "work", head: HEAD, tree: HEAD,
    url: "https://github.com/a/b/pull/1", workflows: []};
  view.rerender(<TaskRepositoryProgress detail={{...task}} />);
  expect(screen.getByText("No saved check dependency is recorded.")).toBeTruthy();
  task.dependency = {active: false, head: HEAD, updated_at: INSTANT, checks: [
    {workflow_id: 1, run_id: null, run_number: null, run_attempt: null, status: "queued", conclusion: null},
    {workflow_id: 2, run_id: 10, run_number: null, run_attempt: null, status: "in_progress", conclusion: null},
  ]};
  task.publication = {effect_id: TASK_IDS[0], ordinal: 1, state: "confirmed", expected_head: HEAD,
    tested_tree: HEAD, commit: null, pull_request_url: task.context.target.url};
  view.rerender(<TaskRepositoryProgress detail={{...task}} />);
  expect(screen.getByText(/Publication confirmed · no confirmed published head/)).toBeTruthy();
  expect(screen.getByText("Workflow 1")).toBeTruthy();
  expect(screen.getByText(/run 10, attempt unknown/)).toBeTruthy();
  expect(screen.getAllByText(/no conclusion/)).toHaveLength(2);
});
