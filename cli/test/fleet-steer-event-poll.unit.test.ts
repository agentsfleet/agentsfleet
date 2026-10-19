import { describe, expect, test } from "bun:test";
import { Effect, Layer, Redacted } from "effect";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import { pollEventTerminal, STATUS_COMPLETE, STATUS_TIMEOUT } from "../src/commands/fleet_steer_events.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { ServerError } from "../src/errors/index.ts";

const WORKSPACE = "01910000-0000-7000-8000-000000000001";
const FLEET = "01910000-0000-7000-8000-000000000002";
const EVENT = "019febb0-272b-78aa-aa3a-03f92e543014";
const TOKEN = Redacted.make("test-token");
const EVENT_PATH = `/v1/workspaces/${WORKSPACE}/fleets/${FLEET}/events/${EVENT}`;

describe("steer recovery reads the admitted event directly", () => {
  test("finds a UUID event without inferring its age or searching a bounded page", async () => {
    const requests: HttpRequestInput[] = [];
    const abort = new AbortController();
    const http = Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput) => Effect.sync(() => {
        requests.push(input);
        abort.abort();
        return { event_id: EVENT, status: EVENT_STATUS.PROCESSED } as T;
      }),
    });
    const outcome = await Effect.runPromise(
      pollEventTerminal(WORKSPACE, FLEET, EVENT, TOKEN, abort.signal).pipe(Effect.provide(http)),
    );
    expect(outcome).toEqual({ kind: STATUS_COMPLETE, status: EVENT_STATUS.PROCESSED });
    expect(requests).toEqual([{ path: EVENT_PATH, token: TOKEN }]);
  });

  test("encodes opaque event IDs as one path segment", async () => {
    const event = "provider/retry?attempt=2";
    const requests: HttpRequestInput[] = [];
    const abort = new AbortController();
    const http = Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput) => Effect.sync(() => {
        requests.push(input);
        abort.abort();
        return { event_id: event, status: EVENT_STATUS.PROCESSED } as T;
      }),
    });
    const outcome = await Effect.runPromise(
      pollEventTerminal(WORKSPACE, FLEET, event, TOKEN, abort.signal).pipe(Effect.provide(http)),
    );
    expect(outcome.kind).toBe(STATUS_COMPLETE);
    expect(requests[0]?.path).toBe(`/v1/workspaces/${WORKSPACE}/fleets/${FLEET}/events/provider%2Fretry%3Fattempt%3D2`);
  });

  test("a transient read failure recovers on the next read of the same event", async () => {
    const paths: string[] = [];
    const abort = new AbortController();
    const http = Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput) => {
        paths.push(input.path);
        if (paths.length === 1) return Effect.fail(new ServerError({
          code: "TEMPORARY_OUTAGE", detail: "Unavailable", status: 503, requestId: null, suggestion: "Retry",
        }));
        abort.abort();
        return Effect.succeed({ event_id: EVENT, status: EVENT_STATUS.PROCESSED } as T);
      },
    });
    const outcome = await Effect.runPromise(
      pollEventTerminal(WORKSPACE, FLEET, EVENT, TOKEN, abort.signal).pipe(Effect.provide(http)),
    );
    expect(outcome.kind).toBe(STATUS_COMPLETE);
    expect(paths).toEqual([EVENT_PATH, EVENT_PATH]);
  });

  test.each([
    { event_id: "another-event", status: EVENT_STATUS.PROCESSED },
    { event_id: EVENT, status: "received" },
    { event_id: EVENT },
  ])("does not report unrelated, pending or incomplete data as completion: %j", async (row) => {
    const abort = new AbortController();
    const http = Layer.succeed(HttpClient, {
      request: <T>() => Effect.sync(() => { abort.abort(); return row as T; }),
    });
    const outcome = await Effect.runPromise(
      pollEventTerminal(WORKSPACE, FLEET, EVENT, TOKEN, abort.signal).pipe(Effect.provide(http)),
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
      pollEventTerminal(WORKSPACE, FLEET, EVENT, TOKEN, abort.signal).pipe(Effect.provide(http)),
    );
    expect(outcome).toEqual({ kind: STATUS_TIMEOUT });
    expect(requests).toBe(0);
  });
});
