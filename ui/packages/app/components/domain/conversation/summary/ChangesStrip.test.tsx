import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ChangesStrip } from "./ChangesStrip";
import type { FleetTaskProps } from "../../tasks/FleetTaskProvider";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";
import { WorkChanges } from "./WorkContextSections";
import { RecordedChangeCounts } from "./RecordedChangesDialog";
import { launchReviewEvent, LAUNCH_REVIEW_ID } from "@/tests/e2e/fixtures/launch-review-activity";
import { launchChangeRepositories } from "@/tests/e2e/fixtures/launch-review-changes";

const state = vi.hoisted(() => ({view: null as Pick<FleetTaskProps, "subject" | "events" | "changeRepositories"> | null}));
vi.mock("../../tasks/FleetTaskProvider", () => ({useFleetTaskView: () => state.view}));
afterEach(() => {cleanup(); state.view = null;});

it("shows no change claim without an identity or confirmed writes", () => {
  const view = render(<ChangesStrip />);
  state.view = {subject: "operator"};
  view.rerender(<ChangesStrip />);
  expect(screen.queryByRole("button")).toBeNull();
  state.view = {subject: "operator", events: [evt({tools: [
    {name: "file_write", args: {path: "pending.ts"}, done: false, startedAtMs: 0, ms: null},
    {name: "file_write", args: {path: "failed.ts"}, done: true, startedAtMs: 0, ms: 1, status: "failed"},
  ]})]};
  view.rerender(<ChangesStrip />);
  expect(screen.queryByRole("button")).toBeNull();
});

it("renders the approved web/docs example through the production flap, Summary and dialog", () => {
  const events = [launchReviewEvent(0)];
  const repositories = launchChangeRepositories(LAUNCH_REVIEW_ID);
  state.view = {subject: "operator", events, changeRepositories: repositories};
  render(<><ChangesStrip /><WorkChanges events={events} repositories={repositories} /></>);
  const flap = screen.getByRole("button", {name: /^6 files changed/});
  expect(within(flap).getByText("+271")).toBeTruthy();
  expect(within(flap).getByText("−3")).toBeTruthy();
  const summary = within(screen.getByRole("region", {name: "Changes"}));
  const [web, docs] = summary.getAllByRole("button");
  expect(web?.textContent).toBe("web4 files+248−3");
  expect(docs?.textContent).toBe("docs2 files+23");
  expect(screen.getAllByText("+248")).toHaveLength(1);
  expect(screen.getAllByText("+23")).toHaveLength(1);
  fireEvent.click(flap);
  const dialog = within(screen.getByRole("dialog", {name: "Recorded changes"}));
  expect(dialog.getByText("web · 4 files")).toBeTruthy();
  expect(dialog.getByText("docs · 2 files")).toBeTruthy();
  for (const path of repositories.map(binding => binding.path)) expect(dialog.getByText(path)).toBeTruthy();
  expect(dialog.getByText("+124").className).toContain("text-success");
  expect(dialog.queryByText("Line totals unavailable")).toBeNull();
});

it("keeps a one-file receipt compact when line totals are unavailable", () => {
  const events = [evt({tools: [{name: "file_write", args: {path: "web/record.md"}, done: true, status: "succeeded", startedAtMs: 0, ms: 1}]})];
  state.view = {subject: "operator", events};
  render(<><ChangesStrip /><WorkChanges events={events} /></>);
  expect(screen.getByRole("button", {name: "1 file changed"})).toBeTruthy();
  expect(screen.getByText("Recorded files")).toBeTruthy();
  expect(screen.queryByText("web")).toBeNull();
  expect(screen.queryByText("Line totals unavailable")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "1 file changed"}));
  expect(within(screen.getByRole("dialog")).getAllByText("Line totals unavailable")).toHaveLength(2);
});

it("opens the native apply_patch destination and actual totals through the production flap", () => {
  state.view = {subject: "operator", events: [evt({tools: [{name: "apply_patch", done: true, status: "succeeded", startedAtMs: 0, ms: 1,
    args: {patch: ("*** Begin Patch\n*** Update File: src/old.ts\n" + "x".repeat(256)).slice(0, 256)},
    outputHead: "Success. Updated the following files:\nM src/moved.ts\n+248 −3", outputLineCount: 3,
  }]})]};
  render(<ChangesStrip />);
  const flap = screen.getByRole("button", {name: "1 file changed +248 −3"});
  fireEvent.click(flap);
  expect(within(screen.getByRole("dialog")).getByText("src/moved.ts")).toBeTruthy();
  expect(screen.queryByText("src/old.ts")).toBeNull();
  expect(screen.queryByText("Line totals unavailable")).toBeNull();
});

it("preserves partial counts and distinguishes a known zero from unknown totals", () => {
  render(<><RecordedChangeCounts lines={{kind: "partial", added: 2, removed: 0}} />
    <RecordedChangeCounts lines={{kind: "complete", added: 0, removed: 3}} />
    <RecordedChangeCounts lines={{kind: "complete", added: 0, removed: 0}} /></>);
  expect(screen.getByText("At least")).toBeTruthy();
  expect(screen.getByText("+2")).toBeTruthy();
  expect(screen.getByText("−3")).toBeTruthy();
  expect(screen.getByText("+0")).toBeTruthy();
  expect(screen.queryByText("−0")).toBeNull();
  expect(screen.queryByText("Line totals unavailable")).toBeNull();
});

it("discloses deduplicated confirmed paths and removes them when identity is lost", async () => {
  const write = (path: string) => ({name: "file_write", args: {path}, done: true, startedAtMs: 0, ms: 1, status: "succeeded" as const});
  state.view = {subject: "operator", events: [evt({tools: [write("src/one.ts"), write("src/one.ts"),
    {name: "file_write", args: {path: "src/one.ts"}, done: false, startedAtMs: 1, ms: null},
  ]})]};
  const view = render(<ChangesStrip />);
  expect(screen.getByRole("button", {name: "1 file changed"})).toBeTruthy();
  state.view = {subject: "operator", events: [evt({tools: [write("src/one.ts"), write("src/two.ts")]})]};
  view.rerender(<ChangesStrip />);
  fireEvent.click(screen.getByRole("button", {name: "2 files changed"}));
  const dialog = await screen.findByRole("dialog", {name: "Recorded changes"});
  expect(dialog.textContent).toContain("src/one.ts");
  expect(dialog.textContent).toContain("src/two.ts");
  expect(screen.queryByRole("link")).toBeNull();
  state.view = {subject: null, events: state.view.events};
  view.rerender(<ChangesStrip />);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("button")).toBeNull();
});
