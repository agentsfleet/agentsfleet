import { ActivityPrinter } from "../src/commands/fleet/task-activity.ts";
import { ToolActivityPrinter } from "../src/commands/fleet/tool-activity.ts";
import { Output } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { describe, expect, test } from "bun:test";
import { Effect, Layer, Redacted } from "effect";
import { TURN_STATUS } from "../src/constants/turn-status.ts";
import { pollTurnTerminal, STATUS_COMPLETE, STATUS_TIMEOUT } from "../src/commands/fleet/steer-turns.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { ServerError } from "../src/errors/index.ts";

const WORKSPACE = "01910000-0000-7000-8000-000000000001";
const FLEET = "01910000-0000-7000-8000-000000000002";
const TURN = "019febb0-272b-78aa-aa3a-03f92e543014";
const TOKEN = Redacted.make("test-token");
const TURN_PATH = `/v1/workspaces/${WORKSPACE}/fleets/${FLEET}/turns/${TURN}`;

describe("steer recovery reads the admitted event directly", () => {
  test("fallback shares stream presentation and suppresses stale progress and repeated tools", async () => {
    const lines: string[] = [];
    const print = (line: string) => { lines.push(line); };
    const presentation = {activity: new ActivityPrinter(print), tools: new ToolActivityPrinter(print)};
    const progress = {activity_id: TURN, operation: "capture", phase: "uploading", provider: null, model: null,
      sequence: 10, completed: 2, total: 3, bytes: {acknowledged: 128, missing: 64, reused: 256}};
    const call = {name: "shell", call_id: "one", status: "succeeded", output_head: "checked"};
    presentation.activity.frame("task_transfer_progress", {progress});
    presentation.tools.frame("tool_call_completed", call);
    const http = Layer.succeed(HttpClient, {request: <T>() => Effect.succeed({turn_id: TURN,
      status: TURN_STATUS.PROCESSED, task_activity: {items: [{kind: "task_transfer_progress",
        progress: {...progress, sequence: 9, completed: 1}}], truncated: false}, tool_calls: {calls: [call]},
    } as T)});
    await Effect.runPromise(pollTurnTerminal(WORKSPACE, FLEET, TURN, TOKEN, undefined, presentation)
      .pipe(Effect.provide(Layer.merge(http, Layer.succeed(Output, outputDouble())))));
    expect(lines).toHaveLength(2);
    expect(lines[0]).toContain("128 bytes acknowledged");
    expect(lines[1]).toContain("checked");
  });

  test("JSON polling emits neither progress nor final-reply prose", async () => {
    const lines: string[] = [];
    const http = Layer.succeed(HttpClient, {
      request: <T>() => Effect.succeed({turn_id: TURN, status: TURN_STATUS.PROCESSED, response_text: "repair complete",
        task_activity: {items: [{kind: "task_transfer_progress", progress: {activity_id: TURN,
          operation: "capture", phase: "saved", provider: null, model: null, completed: 1, total: 1,
          bytes: {acknowledged: 128, missing: 0, reused: 64}}}], truncated: false},
        tool_calls: {calls: [{name: "shell", call_id: "one", status: "succeeded", output_head: "checked"}]},
      } as T),
    });
    const output = Layer.succeed(Output, {...outputDouble({jsonMode: true}), info: (line: string) => Effect.sync(() => { lines.push(line); })});
    const outcome = await Effect.runPromise(pollTurnTerminal(WORKSPACE, FLEET, TURN, TOKEN)
      .pipe(Effect.provide(Layer.merge(http, output))));
    expect(outcome).toEqual({kind: STATUS_COMPLETE, status: TURN_STATUS.PROCESSED});
    expect(lines).toEqual([]);
  });

  test("finds a UUID event without inferring its age or searching a bounded page", async () => {
    const requests: HttpRequestInput[] = [];
    const abort = new AbortController();
    const http = Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput) => Effect.sync(() => {
        requests.push(input);
        abort.abort();
        return { turn_id: TURN, status: TURN_STATUS.PROCESSED } as T;
      }),
    });
    const outcome = await Effect.runPromise(
      pollTurnTerminal(WORKSPACE, FLEET, TURN, TOKEN, abort.signal).pipe(Effect.provide(Layer.merge(http, Layer.succeed(Output, outputDouble())))),
    );
    expect(outcome).toEqual({ kind: STATUS_COMPLETE, status: TURN_STATUS.PROCESSED });
    expect(requests).toHaveLength(1);
    expect(requests[0]).toMatchObject({path: TURN_PATH, token: TOKEN, signal: abort.signal, retry: {maxAttempts: 1}});
    expect(requests[0]?.timeoutMs).toBeGreaterThan(0);
    expect(requests[0]?.timeoutMs).toBeLessThanOrEqual(60_000);
  });

  test("encodes opaque event IDs as one path segment", async () => {
    const event = "provider/retry?attempt=2";
    const requests: HttpRequestInput[] = [];
    const abort = new AbortController();
    const http = Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput) => Effect.sync(() => {
        requests.push(input);
        abort.abort();
        return { turn_id: event, status: TURN_STATUS.PROCESSED } as T;
      }),
    });
    const outcome = await Effect.runPromise(
      pollTurnTerminal(WORKSPACE, FLEET, event, TOKEN, abort.signal).pipe(Effect.provide(Layer.merge(http, Layer.succeed(Output, outputDouble())))),
    );
    expect(outcome.kind).toBe(STATUS_COMPLETE);
    expect(requests[0]?.path).toBe(`/v1/workspaces/${WORKSPACE}/fleets/${FLEET}/turns/provider%2Fretry%3Fattempt%3D2`);
  });

  test("a transient read failure recovers on the next read of the same event", async () => {
    const paths: string[] = [];
    const abort = new AbortController();
    const http = Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput) => {
        paths.push(input.path);
        if (paths.length === 1) return Effect.fail(new ServerError({
          code: "TEMPORARY_OUTAGE", detail: "Unavailable", status: 503, requestId: null, suggestion: "Retry", transient: true,
        }));
        abort.abort();
        return Effect.succeed({ turn_id: TURN, status: TURN_STATUS.PROCESSED } as T);
      },
    });
    const outcome = await Effect.runPromise(
      pollTurnTerminal(WORKSPACE, FLEET, TURN, TOKEN, abort.signal).pipe(Effect.provide(Layer.merge(http, Layer.succeed(Output, outputDouble())))),
    );
    expect(outcome.kind).toBe(STATUS_COMPLETE);
    expect(paths).toEqual([TURN_PATH, TURN_PATH]);
  });

  test.each([
    { turn_id: "another-event", status: TURN_STATUS.PROCESSED },
    { turn_id: TURN, status: "received" },
    { turn_id: TURN },
  ])("does not report unrelated, pending or incomplete data as completion: %j", async (row) => {
    const abort = new AbortController();
    const http = Layer.succeed(HttpClient, {
      request: <T>() => Effect.sync(() => { abort.abort(); return row as T; }),
    });
    const outcome = await Effect.runPromise(
      pollTurnTerminal(WORKSPACE, FLEET, TURN, TOKEN, abort.signal).pipe(Effect.provide(Layer.merge(http, Layer.succeed(Output, outputDouble())))),
    );
    expect(outcome).toEqual({ kind: STATUS_TIMEOUT });
  });

  test("cancellation before the first read sends no request", async () => {
    const abort = new AbortController();
    abort.abort();
    let requests = 0;
    const http = Layer.succeed(HttpClient, {
      request: <T>() => Effect.sync(() => { requests += 1; return {} as T; }),
    });
    const outcome = await Effect.runPromise(
      pollTurnTerminal(WORKSPACE, FLEET, TURN, TOKEN, abort.signal).pipe(Effect.provide(Layer.merge(http, Layer.succeed(Output, outputDouble())))),
    );
    expect(outcome).toEqual({ kind: STATUS_TIMEOUT });
    expect(requests).toBe(0);
  });
});
