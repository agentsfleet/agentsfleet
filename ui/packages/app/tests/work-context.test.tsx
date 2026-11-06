import type { TaskActivity } from "@/lib/task-activity";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { FleetWorkSidebar } from "@/components/domain/FleetWorkSidebar";
import { WorkAction, workActivity } from "@/components/domain/chat/WorkActivity";
import { TaskNextStep } from "@/components/domain/chat/TaskNextStep";
import { evt } from "./helpers/fleet-stream-fixtures";
import { detail, TASK_IDS } from "./helpers/task-reads";
afterEach(() => {cleanup(); vi.restoreAllMocks();});

it("uses the existing accessible dialog on mobile without touching the conversation draft", () => {
  render(<><textarea aria-label="Draft" defaultValue="Keep my exact draft" /><FleetWorkSidebar><p>PR 123 evidence</p></FleetWorkSidebar></>);
  expect(screen.queryByText("PR 123 evidence")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Work context"}));
  expect(screen.getByRole("dialog").textContent).toContain("PR 123 evidence");
  expect(screen.getByRole("textbox", {name: "Draft", hidden: true})).toHaveProperty("value", "Keep my exact draft");
});

it("mounts one desktop sidebar, and no work surface when no child exists", () => {
  const original = window.matchMedia;
  vi.spyOn(window, "matchMedia").mockImplementation(query => ({...original(query), matches: true}));
  const view = render(<FleetWorkSidebar><p>PR 123 evidence</p></FleetWorkSidebar>);
  expect(screen.getByRole("complementary", {name: "Work context"}).textContent).toBe("PR 123 evidence");
  expect(screen.queryByRole("button", {name: "Work context"})).toBeNull();
  view.rerender(<FleetWorkSidebar>{null}</FleetWorkSidebar>);
  expect(screen.queryByText("PR 123 evidence")).toBeNull();
});

it("attributes actions only to explicit task identities and retains recorded provenance", () => {
  const item: TaskActivity = {kind: "task_transfer_progress", progress: {activity_id: TASK_IDS[0], task_id: TASK_IDS[0],
    source_admission_id: null, operation: "capture", phase: "uploading", provider: null, model: null, completed: 1, total: 2}};
  const events = [evt(), evt({taskActivity: {items: [item, {...item, progress: {...item.progress, task_id: null}}], truncated: true}})];
  const history = workActivity(events, TASK_IDS[0]);
  expect(history.items).toEqual([item]);
  expect(history.truncated).toBe(true);
  const recent = {...item, progress: {...item.progress, sequence: 1, phase: "saved" as const}};
  expect(workActivity([...events, evt({taskActivity: {items: [recent], truncated: false}})], TASK_IDS[0]).items).toEqual([recent]);
  expect(workActivity(events, TASK_IDS[1])).toEqual({items: [], truncated: false});
  const view = render(<WorkAction history={history} />);
  expect(screen.getByText("Latest recorded action: Saving encrypted files")).toBeTruthy();
  view.rerender(<WorkAction history={{items: [], truncated: false}} />);
  expect(screen.getByText(/Open for the objective/)).toBeTruthy();
});

it("explains saved dependencies without inventing a next action from status", () => {
  const value = detail();
  const view = render(<TaskNextStep detail={value} />);
  expect(screen.getByText(/No active check dependency/)).toBeTruthy();
  const dependency = {active: true, head: "a".repeat(40), updated_at: "2026-11-03T10:00:00Z", checks: []};
  view.rerender(<TaskNextStep detail={{...value, dependency}} />);
  expect(screen.getByText(/continuation is not yet confirmed/)).toBeTruthy();
  const check = {workflow_id: 42, run_id: null, run_number: null, run_attempt: null, status: "queued", conclusion: null};
  view.rerender(<TaskNextStep detail={{...value, dependency: {...dependency, checks: [check]}}} />);
  expect(screen.getByText("Waiting on: Workflow 42.")).toBeTruthy();
  const target = {kind: "existing_pr" as const, repository: "a/b", number: 1, branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/a/b/pull/1", workflows: [{id: 42, path: "checks.yml", digest: "x"}]};
  view.rerender(<TaskNextStep detail={{...value, context: {...value.context, target}, dependency: {...dependency, checks: [check]}}} />);
  expect(screen.getByText("Waiting on: checks.yml.")).toBeTruthy();
});

it("hydrates server-rendered mobile controls into exactly one desktop work surface", async () => {
  const {renderToString} = await import("react-dom/server");
  const original = window.matchMedia;
  vi.spyOn(window, "matchMedia").mockImplementation(query => ({...original(query), matches: true}));
  const element = <FleetWorkSidebar><p>Confirmed repair evidence</p></FleetWorkSidebar>;
  const container = document.createElement("div");
  container.innerHTML = renderToString(element);
  document.body.append(container);
  expect(container.textContent).toContain("Work context");
  expect(container.textContent).not.toContain("Confirmed repair evidence");
  render(element, {container, hydrate: true});
  expect(await screen.findByRole("complementary", {name: "Work context"})).toBeTruthy();
  expect(screen.getAllByText("Confirmed repair evidence")).toHaveLength(1);
  expect(screen.queryByRole("button", {name: "Work context"})).toBeNull();
});
