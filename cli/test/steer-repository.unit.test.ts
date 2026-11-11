import { expect, test } from "bun:test";
import { Effect, Layer } from "effect";
import { renderRepository } from "../src/commands/fleet/tasks.ts";
import { Output } from "../src/services/output.ts";
import type { TaskDetail } from "../src/commands/fleet/task-read.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { taskDetail, HEAD, COMMIT, TASK, INSTANT } from "./helpers-steered-work.ts";
async function render(task: TaskDetail): Promise<string[]> {
  const lines: string[] = [];
  await Effect.runPromise(renderRepository(task).pipe(Effect.provide(Layer.succeed(Output,
    {...outputDouble(), info: (line: string) => Effect.sync(() => {lines.push(line);})}))));
  return lines;
}

test("saved publications distinguish confirmed heads from unknown effects and local edits", async () => {
  const detail = taskDetail();
  const receipt = {effect_id: TASK, ordinal: 1, state: "unknown" as const, expected_head: HEAD,
    tested_tree: HEAD, commit: COMMIT, pull_request_url: "https://github.com/agentsfleet/agentsfleet/pull/803"};
  const unknown = await render({...detail, publication: receipt});
  expect(unknown.join("\n")).toContain("Local edits and saved recovery files are not a confirmed publication.");
  expect(unknown.join("\n")).toContain("Publication unknown: no confirmed published head");
  expect(unknown.join("\n")).not.toContain(COMMIT);
  expect(unknown.at(-1)).toBe("No saved check dependency is recorded.");
  expect((await render({...detail, publication: {...receipt, state: "confirmed"}})).join("\n")).toContain(`Published head: ${COMMIT}`);
  expect((await render({...detail, publication: {...receipt, state: "confirmed", commit: null}})).join("\n")).toContain("Publication confirmed: no confirmed published head");
});

test("check results carry saved-head provenance, missing run evidence, and terminal-safe labels", async () => {
  const detail = taskDetail();
  const dependency = {active: true, head: HEAD, updated_at: INSTANT, checks: [
    {workflow_id: 1, run_id: 10, run_number: 3, run_attempt: 2, status: "completed", conclusion: "success"},
    {workflow_id: 2, run_id: null, run_number: null, run_attempt: null, status: "queued", conclusion: null},
    {workflow_id: 3, run_id: 11, run_number: null, run_attempt: null, status: "\u001b[31min_progress\u001b[0m", conclusion: null},
  ]};
  const active = (await render({...detail, dependency})).join("\n");
  expect(active).toContain(`Waiting for reviewed checks at ${HEAD}`);
  expect(active).toContain("not a fresh remote check");
  expect(active).toContain("check.yml: completed / success · run 10, attempt 2");
  expect(active).toContain("Workflow 2: queued / no conclusion");
  expect(active).toContain("Workflow 3: in_progress / no conclusion · run 11, attempt unknown");
  expect(active).not.toContain("\u001b");
  expect((await render({...detail, dependency: {...dependency, active: false}})).join("\n")).toContain(`Saved check results at ${HEAD}`);
});

test("API-only and older tasks produce no invented repository progress", async () => {
  const detail = taskDetail();
  expect(await render({...detail, context: {...detail.context, target: {kind: "api_only"}}})).toEqual([]);
  const {target: _target, ...context} = detail.context;
  expect(await render({...detail, context})).toEqual([]);
});
