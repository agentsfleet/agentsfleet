// Lifecycle commands share the existing native task owner; the CLI owns no execution loop.

import { Effect } from "effect";
import { v7 as uuidv7 } from "uuid";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { wsFleetPath } from "../lib/api-paths.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { TaskDetailSchema, TaskPageSchema, decodeTask, type TaskDetail } from "./task-read.ts";
import { IF_MATCH } from "./chat/transport.ts";

export type TaskMutation = {kind: "control"; state: "paused" | "ready" | "cancelled"}
  | {kind: "adopt"; revisionId: string};

export function taskListEffect(fleetId: string) {
  return Effect.gen(function* () {
    const http = yield* HttpClient;
    const output = yield* Output;
    const workspace = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const page = yield* http.request<unknown>({path: `${wsFleetPath(workspace, fleetId)}/tasks?limit=100`, token})
      .pipe(Effect.flatMap(value => decodeTask(TaskPageSchema, value)));
    if (output.format === OUTPUT_FORMAT.json) return yield* output.printJson(page);
    if (page.items.length === 0) return yield* output.info("No unfinished work.");
    for (const task of page.items) yield* output.printKeyValue({Work: task.title, State: task.status, ID: task.id});
    if (page.next_cursor) yield* output.info(`More work is available after ${page.next_cursor}.`);
  });
}

export function taskShowEffect(fleetId: string, taskId: string) {
  return Effect.gen(function* () {
    const http = yield* HttpClient;
    const workspace = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const detail = yield* http.request<unknown>({path: taskPath(workspace, fleetId, taskId), token})
      .pipe(Effect.flatMap(value => decodeTask(TaskDetailSchema, value)));
    yield* render(detail);
  });
}

export function taskChangeEffect(fleetId: string, taskId: string, mutation: TaskMutation) {
  return Effect.gen(function* () {
    const http = yield* HttpClient;
    const output = yield* Output;
    const workspace = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const path = taskPath(workspace, fleetId, taskId);
    const observed = yield* http.request<unknown>({path, token}).pipe(Effect.flatMap(value => decodeTask(TaskDetailSchema, value)));
    const body = mutation.kind === "control"
      ? {state: mutation.state, expected_generation: observed.context.generation}
      : {goal_revision_id: mutation.revisionId, expected_generation: observed.context.generation, operation_id: uuidv7()};
    const saved = yield* http.request<unknown>({path, method: "PATCH", token, headers: {[IF_MATCH]: observed.etag}, body, retry: null})
      .pipe(Effect.flatMap(value => decodeTask(TaskDetailSchema, value)));
    if (output.format !== OUTPUT_FORMAT.json) yield* output.info(mutation.kind === "adopt"
      ? `Success criteria now use setup v${saved.setup_revision}. Progress and limits are retained.`
      : `Confirmed state: ${saved.context.state}.`);
    yield* render(saved);
  });
}

function taskPath(workspace: string, fleet: string, task: string): string {
  return `${wsFleetPath(workspace, fleet)}/tasks/${encodeURIComponent(task)}`;
}

function render(detail: TaskDetail) {
  return Effect.gen(function* () {
    const output = yield* Output;
    if (output.format === OUTPUT_FORMAT.json) return yield* output.printJson(detail);
    yield* output.printKeyValue({Work: detail.title, State: detail.context.state, Objective: detail.context.objective,
      Setup: `v${detail.setup_revision}`, Attempts: `${detail.context.usage.leases} of ${detail.context.limits.leases}`,
      Recovery: detail.context.checkpoint_id ? "Saved recovery point available" : "No saved recovery point"});
    yield* output.info("What counts as done?");
    for (const [index, statement] of detail.context.questions.entries()) {
      yield* output.info(`${index + 1}. ${statement.question.instructions}\n  Yes: ${statement.question.criteria.true}\n  No: ${statement.question.criteria.false}`);
    }
  });
}
