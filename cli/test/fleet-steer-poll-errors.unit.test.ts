import { Output } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { expect, test } from "bun:test";
import { Cause, Effect, Exit, Fiber, Layer, Option, Redacted } from "effect";
import { TestClock } from "effect/testing";
import { pollEventTerminal, STATUS_TIMEOUT } from "../src/commands/fleet_steer_events.ts";
import { NetworkError, ServerError } from "../src/errors/index.ts";
import { HttpClient, httpClientLayer } from "../src/services/http-client.ts";
import { cliConfigFromValuesLayer } from "../src/services/config.ts";
import { FLEET_ID, EVENT_ID, WS_ID, TOKEN } from "./helpers-fleet-steer.ts";

test.each([401, 403, 404])("event polling surfaces HTTP %s without another read", async (status) => {
  let reads = 0;
  const failure = new ServerError({status, detail: "event cannot be read", suggestion: "inspect access", code: "READ_REFUSED", requestId: null});
  const http = Layer.succeed(HttpClient, {
    request: <T>() => {
      reads++;
      return reads === 1 ? Effect.fail(failure) : Effect.succeed({event_id: EVENT_ID, status: "processed"} as T);
    },
  });
  const result = await Effect.runPromiseExit(
    pollEventTerminal(WS_ID, FLEET_ID, EVENT_ID, Redacted.make(TOKEN)).pipe(Effect.provide(Layer.merge(http, Layer.succeed(Output, outputDouble())))),
  );
  expect(Exit.isFailure(result)).toBe(true);
  const error = Exit.isFailure(result) ? Option.getOrNull(Cause.findErrorOption(result.cause)) : null;
  expect(error).toBe(failure);
  expect(error).toMatchObject({status, code: "READ_REFUSED"});
  expect(reads).toBe(1);
});

test("event polling interrupts a stalled read at the overall deadline", async () => {
  let released = false;
  const http = Layer.succeed(HttpClient, {
    request: () => Effect.never.pipe(Effect.onInterrupt(() => Effect.sync(() => { released = true; }))),
  });
  const result = await Effect.runPromise(Effect.gen(function* () {
    const fiber = yield* Effect.forkChild(
      pollEventTerminal(WS_ID, FLEET_ID, EVENT_ID, Redacted.make(TOKEN)).pipe(Effect.timeout("61 seconds")),
    );
    yield* TestClock.adjust("61 seconds");
    return yield* Fiber.await(fiber);
  }).pipe(Effect.provide(Layer.mergeAll(http, TestClock.layer(), Layer.succeed(Output, outputDouble())))));
  expect(Exit.isSuccess(result)).toBe(true);
  if (Exit.isSuccess(result)) expect(result.value).toEqual({kind: STATUS_TIMEOUT});
  expect(released).toBe(true);
});

test.each(["CERT_HAS_EXPIRED", "ERR_INVALID_URL"])("event polling surfaces permanent transport failure %s", async (code) => {
  let reads = 0;
  const config = cliConfigFromValuesLayer({apiUrl: "https://api.test.local", fetchImpl: async () => {
    reads++;
    if (reads === 1) throw Object.assign(new TypeError("fetch failed"), {cause: {code}});
    return Response.json({event_id: EVENT_ID, status: "processed"});
  }});
  const result = await Effect.runPromiseExit(
    pollEventTerminal(WS_ID, FLEET_ID, EVENT_ID, Redacted.make(TOKEN)).pipe(
      Effect.provide(httpClientLayer), Effect.provide(config), Effect.provide(Layer.succeed(Output, outputDouble())),
    ),
  );
  expect(Exit.isFailure(result)).toBe(true);
  const error = Exit.isFailure(result) ? Option.getOrNull(Cause.findErrorOption(result.cause)) : null;
  expect(error).toBeInstanceOf(NetworkError);
  expect(error).toMatchObject({transient: false, detail: "fetch failed"});
  expect(reads).toBe(1);
});

test("event polling recovers from a transient transport failure", async () => {
  let reads = 0;
  const config = cliConfigFromValuesLayer({apiUrl: "https://api.test.local", fetchImpl: async () => {
    reads++;
    if (reads === 1) throw Object.assign(new TypeError("fetch failed"), {code: "ECONNREFUSED"});
    return Response.json({event_id: EVENT_ID, status: "processed"});
  }});
  const result = await Effect.runPromise(
    pollEventTerminal(WS_ID, FLEET_ID, EVENT_ID, Redacted.make(TOKEN)).pipe(
      Effect.provide(httpClientLayer), Effect.provide(config), Effect.provide(Layer.succeed(Output, outputDouble())),
    ),
  );
  expect(result).toEqual({kind: "complete", status: "processed"});
  expect(reads).toBe(2);
});

test("event polling respects the server's retry opt-out", async () => {
  let reads = 0;
  const config = cliConfigFromValuesLayer({apiUrl: "https://api.test.local", fetchImpl: async () => {
    reads++;
    return Response.json({error: {retry_after_seconds: 0}}, {status: 503});
  }});
  const result = await Effect.runPromiseExit(
    pollEventTerminal(WS_ID, FLEET_ID, EVENT_ID, Redacted.make(TOKEN)).pipe(
      Effect.provide(httpClientLayer), Effect.provide(config), Effect.provide(Layer.succeed(Output, outputDouble())),
    ),
  );
  expect(Exit.isFailure(result)).toBe(true);
  const error = Exit.isFailure(result) ? Option.getOrNull(Cause.findErrorOption(result.cause)) : null;
  expect(error).toBeInstanceOf(ServerError);
  expect(error).toMatchObject({transient: false, status: 503, code: "HTTP_503"});
  expect(reads).toBe(1);
});

test("event polling waits for an admitted event to reach the read store", async () => {
  let reads = 0;
  const config = cliConfigFromValuesLayer({apiUrl: "https://api.test.local", fetchImpl: async () => {
    reads++;
    return reads === 1
      ? Response.json({error_code: "UZ-AGT-015", detail: "Event not found"}, {status: 404})
      : Response.json({event_id: EVENT_ID, status: "processed"});
  }});
  const result = await Effect.runPromise(
    pollEventTerminal(WS_ID, FLEET_ID, EVENT_ID, Redacted.make(TOKEN)).pipe(
      Effect.provide(httpClientLayer), Effect.provide(config), Effect.provide(Layer.succeed(Output, outputDouble())),
    ),
  );
  expect(result).toEqual({kind: "complete", status: "processed"});
  expect(reads).toBe(2);
});

test("event polling surfaces disappearance after observing the admitted event", async () => {
  let reads = 0;
  const config = cliConfigFromValuesLayer({apiUrl: "https://api.test.local", fetchImpl: async () => {
    reads++;
    if (reads === 1) return Response.json({event_id: EVENT_ID, status: "received"});
    if (reads === 2) return Response.json({error_code: "UZ-AGT-015", detail: "Event not found"}, {status: 404});
    return Response.json({event_id: EVENT_ID, status: "processed"});
  }});
  const result = await Effect.runPromiseExit(
    pollEventTerminal(WS_ID, FLEET_ID, EVENT_ID, Redacted.make(TOKEN)).pipe(
      Effect.provide(httpClientLayer), Effect.provide(config), Effect.provide(Layer.succeed(Output, outputDouble())),
    ),
  );
  const error = Exit.isFailure(result) ? Option.getOrNull(Cause.findErrorOption(result.cause)) : null;
  expect(error).toBeInstanceOf(ServerError);
  expect(error).toMatchObject({status: 404, code: "UZ-AGT-015", transient: false});
  expect(reads).toBe(2);
});
