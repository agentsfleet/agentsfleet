import {afterEach, expect, it} from "vitest";
import {cleanup, render, screen, within} from "@testing-library/react";
import type {TaskDetail} from "@/lib/api/tasks";
import {detail, TASK_IDS} from "@/tests/helpers/task-reads";
import {WorkOutputs} from "./WorkContextSections";

afterEach(cleanup);
const TARGET = {kind: "existing_pr" as const, repository: "a/b", number: 123, branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/a/b/pull/123", workflows: []};
function saved(state: NonNullable<TaskDetail["publication"]>["state"], commit: string | null = "c".repeat(40)): TaskDetail {
  const value = detail();
  return {...value, context: {...value.context, target: TARGET}, publication: {state, commit,
    effect_id: TASK_IDS[1], ordinal: 1, expected_head: TARGET.head, tested_tree: TARGET.tree, pull_request_url: TARGET.url}};
}

// Value: protects=truthful output status; fails_when=a task target or uncertain publication is presented as produced work;
// why_new=the compact Summary separates outputs from related PRs; seam=parsed API detail; no_claim=GitHub publication.
it.each(["prepared", "issued", "unknown", "refused"] as const)("keeps a %s publication under related pull requests", state => {
  render(<WorkOutputs details={new Map([[TASK_IDS[0], {kind: "ready", value: saved(state)}]])} />);
  expect(within(screen.getByRole("region", {name: "Pull requests"})).getByRole("link")).toHaveProperty("href", TARGET.url);
  expect(screen.queryByRole("region", {name: "Outputs"})).toBeNull();
});

it("requires the confirmed commit and matching PR before reporting a published output", () => {
  const view = render(<WorkOutputs details={new Map([[TASK_IDS[0], {kind: "ready", value: saved("confirmed", null)}]])} />);
  expect(screen.queryByText("Changes published")).toBeNull();
  view.rerender(<WorkOutputs details={new Map([[TASK_IDS[0], {kind: "ready", value: saved("confirmed")} ]])} />);
  expect(within(screen.getByRole("region", {name: "Outputs"})).getByText("Changes published")).toBeTruthy();
  const changed = saved("confirmed");
  if (changed.publication) changed.publication.pull_request_url = "https://github.com/a/b/pull/124";
  view.rerender(<WorkOutputs details={new Map([[TASK_IDS[0], {kind: "ready", value: changed}]])} />);
  expect(screen.queryByText("Changes published")).toBeNull();
});
