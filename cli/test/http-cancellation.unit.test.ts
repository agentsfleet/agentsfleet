import { expect, test } from "bun:test";
import { Effect, Fiber, Layer, Option } from "effect";
import { apiRequest, type FetchImpl } from "../src/lib/http.ts";
import { apiRequestWithRetry } from "../src/lib/http-retry.ts";
import { CliConfig } from "../src/services/config.ts";
import { HttpClient, httpClientLayer } from "../src/services/http-client.ts";

const URL = "https://api.test.local/request";

test("an already cancelled request never reaches fetch", async () => {
  const abort = new AbortController();
  abort.abort(new Error("operator stopped"));
  let requests = 0;
  await expect(apiRequest(URL, {signal: abort.signal, fetchImpl: async () => {
    requests++;
    return Response.json({});
  }})).rejects.toThrow("operator stopped");
  expect(requests).toBe(0);
});

test("cancelling HTTP backoff prevents another POST attempt", async () => {
  const abort = new AbortController();
  const retryStarted = Promise.withResolvers<void>();
  let requests = 0;
  const pending = apiRequestWithRetry(URL, {
    method: "POST", signal: abort.signal,
    retry: {baseDelayMs: 10_000},
    onRetry: () => { retryStarted.resolve(); },
    fetchImpl: async () => { requests++; return Response.json({}, {status: 429}); },
  });
  await retryStarted.promise;
  abort.abort(new Error("operator stopped"));
  await expect(pending).rejects.toMatchObject({name: "AbortError", cause: abort.signal.reason});
  expect(requests).toBe(1);
});

test("cancelling an accepted HTTP write stops waiting without replaying it", async () => {
  const accepted = Promise.withResolvers<void>();
  const answer = Promise.withResolvers<Response>();
  let writes = 0;
  const server = Bun.serve({hostname: "127.0.0.1", port: 0, fetch: () => {
    writes++;
    accepted.resolve();
    return answer.promise;
  }});
  const abort = new AbortController();
  try {
    const pending = apiRequestWithRetry(server.url.href, {method: "POST", signal: abort.signal});
    await accepted.promise;
    abort.abort(new Error("operator stopped"));
    await expect(pending).rejects.toThrow("operator stopped");
    expect(writes).toBe(1);
  } finally {
    answer.resolve(Response.json({accepted: true}));
    await server.stop(true);
  }
});

test("Effect interruption aborts the HTTP request owned by that fiber", async () => {
  const started = Promise.withResolvers<void>();
  let signal: AbortSignal | null | undefined;
  const fetchImpl: FetchImpl = (_url, init) => new Promise((_resolve, reject) => {
    signal = init?.signal;
    signal?.addEventListener("abort", () => reject(signal?.reason), {once: true});
    started.resolve();
  });
  const config = Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local", dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(), jsonMode: false, noOpen: true,
    telemetryPosthogKey: "", telemetryPosthogHost: "https://telemetry.test.local", fetchImpl,
  });
  await Effect.runPromise(Effect.gen(function* () {
    const http = yield* HttpClient;
    const fiber = yield* Effect.forkChild(http.request({path: "/request", method: "POST"}));
    yield* Effect.promise(() => started.promise);
    yield* Fiber.interrupt(fiber);
  }).pipe(Effect.provide(Layer.provide(httpClientLayer, config))));
  expect(signal?.aborted).toBe(true);
});
