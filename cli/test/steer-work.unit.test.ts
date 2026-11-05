import { taskDetail, HEAD, INSTANT } from "./helpers-steered-work.ts";
import { ServerError } from "../src/errors/index.ts";
import { expect, test } from "bun:test";
import { Effect, Layer, Redacted } from "effect";
import { readSteeredWork } from "../src/commands/steer-work.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";

const WORKSPACE = "01910000-0000-7000-8000-000000000001";
const FLEET = "01910000-0000-7000-8000-000000000002";
const EVENT = "01910000-0000-7000-8000-000000000003";
const TASK_A = "01910000-0000-7000-8000-000000000004";
const TASK_B = "01910000-0000-7000-8000-000000000005";
const TOKEN = Redacted.make("test-token");

test("saved work selects the last occurrence, not the last distinct task identity", async () => {
  const requests: HttpRequestInput[] = [];
  const lines: string[] = [];
  const http = Layer.succeed(HttpClient, {request: <T>(input: HttpRequestInput) => Effect.sync(() => {
    requests.push(input);
    return (requests.length === 1 ? {event_id: EVENT, task_activity: {
      items: [TASK_A, TASK_B, TASK_A].map(task_id => ({progress: {task_id}})),
    }} : {}) as T;
  })});
  const output = Layer.succeed(Output, {...outputDouble(), info: (line: string) => Effect.sync(() => { lines.push(line); })});
  await Effect.runPromise(readSteeredWork(WORKSPACE, FLEET, EVENT, TOKEN).pipe(Effect.provide(Layer.merge(http, output))));
  expect(requests.map(request => request.path)).toEqual([
    `/v1/workspaces/${WORKSPACE}/fleets/${FLEET}/events/${EVENT}`,
    `/v1/workspaces/${WORKSPACE}/fleets/${FLEET}/tasks/${TASK_A}`,
  ]);
  // An incompatible detail is reported without admitting/resending any work.
  expect(lines).toEqual(["Saved work details could not be refreshed. The admitted request was not resent."]);
});

test("JSON mode does not make presentation-only detail requests", async () => {
  let requests = 0;
  const http = Layer.succeed(HttpClient, {request: <T>() => Effect.sync(() => { requests++; return {} as T; })});
  await Effect.runPromise(readSteeredWork(WORKSPACE, FLEET, EVENT, TOKEN).pipe(Effect.provide(
    Layer.merge(http, Layer.succeed(Output, outputDouble({jsonMode: true}))),
  )));
  expect(requests).toBe(0);
});

test("successful readback validates saved task detail and presents the reviewed dependency", async () => {
  const requests: HttpRequestInput[] = [];
  const lines: string[] = [];
  const detail = taskDetail();
  const http = Layer.succeed(HttpClient, {request: <T>(input: HttpRequestInput) => Effect.sync(() => {
    requests.push(input);
    return (requests.length === 1 ? {event_id: EVENT, task_activity: {items: [
      {progress: {task_id: TASK_A}}, null, {}, {progress: null}, {progress: {task_id: 2}}, {progress: {task_id: "invalid"}},
    ]}} : {...detail, dependency: {active: false, head: HEAD, updated_at: INSTANT, checks: []}}) as T;
  })});
  const output = Layer.succeed(Output, {...outputDouble(), info: (line: string) => Effect.sync(() => {lines.push(line);})});
  await Effect.runPromise(readSteeredWork(WORKSPACE, FLEET, EVENT, TOKEN).pipe(Effect.provide(Layer.merge(http, output))));
  expect(lines.join("\n")).toContain(`Saved check results at ${HEAD}`);
  expect(requests).toHaveLength(2);
  for (const request of requests) {
    expect(request.method ?? "GET").toBe("GET");
    expect(request.retry).toEqual({maxAttempts: 1});
    expect(request.timeoutMs).toBe(5000);
  }
});

test.each([null, {event_id: "another-event"}, {event_id: EVENT}, {event_id: EVENT, task_activity: {}},
  {event_id: EVENT, task_activity: {items: []}}])("unowned or absent activity cannot select or invent a task: %j", async saved => {
  let requests = 0;
  const lines: string[] = [];
  const http = Layer.succeed(HttpClient, {request: <T>() => Effect.sync(() => {requests++; return saved as T;})});
  const output = Layer.succeed(Output, {...outputDouble(), info: (line: string) => Effect.sync(() => {lines.push(line);})});
  await Effect.runPromise(readSteeredWork(WORKSPACE, FLEET, EVENT, TOKEN).pipe(Effect.provide(Layer.merge(http, output))));
  expect(requests).toBe(1);
  expect(lines).toEqual([]);
});

test("cancellation before or during the event read prevents a task request", async () => {
  for (const before of [true, false]) {
    const owner = new AbortController();
    let requests = 0;
    if (before) owner.abort();
    const http = Layer.succeed(HttpClient, {request: <T>() => Effect.sync(() => {
      requests++; owner.abort();
      return {event_id: EVENT, task_activity: {items: [{progress: {task_id: TASK_A}}]}} as T;
    })});
    await Effect.runPromise(readSteeredWork(WORKSPACE, FLEET, EVENT, TOKEN, owner.signal).pipe(Effect.provide(
      Layer.merge(http, Layer.succeed(Output, outputDouble())),
    )));
    expect(requests).toBe(before ? 0 : 1);
  }
});

test("aborted uncertain reads stay silent while ordinary failures explain that no work was resent", async () => {
  for (const abort of [true, false]) {
    const owner = new AbortController();
    const lines: string[] = [];
    let requests = 0;
    const http = Layer.succeed(HttpClient, {request: () => Effect.sync(() => {
      requests++;
      if (abort) owner.abort();
    }).pipe(Effect.flatMap(() => Effect.fail(new ServerError({code: "UNAVAILABLE", detail: "offline", status: 503,
      requestId: null, suggestion: "Retry", transient: true}))))});
    const output = Layer.succeed(Output, {...outputDouble(), info: (line: string) => Effect.sync(() => {lines.push(line);})});
    await Effect.runPromise(readSteeredWork(WORKSPACE, FLEET, EVENT, TOKEN, owner.signal).pipe(Effect.provide(Layer.merge(http, output))));
    expect(requests).toBe(1);
    expect(lines).toEqual(abort ? [] : ["Saved work details could not be refreshed. The admitted request was not resent."]);
  }
});
