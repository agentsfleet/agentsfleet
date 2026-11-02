import { expect, test } from "bun:test";
import { Cause, Effect, Exit, Option } from "effect";
import { steerEffectFromArgs } from "../src/commands/fleet_steer.ts";
import { InterruptedError, ServerError } from "../src/errors/index.ts";
import { cliConfigFromValuesLayer } from "../src/services/config.ts";
import { httpClientLayer } from "../src/services/http-client.ts";
import { FLEET_ID, makeLayer, makeRecorder, nullOutput, streamFrom } from "./helpers-fleet-steer.ts";

test("steer preserves a native message refusal without another send", async () => {
  const rec = makeRecorder();
  let posts = 0;
  const config = cliConfigFromValuesLayer({apiUrl: "https://api.test.local", fetchImpl: async (_url, init) => {
    expect(init?.method).toBe("POST");
    posts++;
    return Response.json({error_code: "UZ-AUTHOR-001", detail: "message refused"}, {status: 403});
  }});
  const result = await Effect.runPromiseExit(
    steerEffectFromArgs(FLEET_ID, "investigate", {}, {
      stdin: streamFrom([], false), stdout: nullOutput(), streamGet: async () => {},
    }).pipe(Effect.provide(httpClientLayer), Effect.provide(config), Effect.provide(makeLayer(rec))),
  );
  const error = Exit.isFailure(result) ? Option.getOrNull(Cause.findErrorOption(result.cause)) : null;
  expect(error).toBeInstanceOf(ServerError);
  expect(error).toMatchObject({status: 403, code: "UZ-AUTHOR-001", detail: "message refused"});
  expect(posts).toBe(1);
});

test("steer cancellation after message acceptance reports interruption without replay", async () => {
  const rec = makeRecorder();
  const accepted = Promise.withResolvers<void>();
  const answer = Promise.withResolvers<Response>();
  let posts = 0;
  const server = Bun.serve({hostname: "127.0.0.1", port: 0, fetch: async (request) => {
    expect(request.method).toBe("POST");
    expect(await request.json()).toEqual({message: "investigate"});
    posts++;
    accepted.resolve();
    return answer.promise;
  }});
  const config = cliConfigFromValuesLayer({apiUrl: server.url.origin});
  const abort = new AbortController();
  try {
    const result = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, undefined, {forceTty: true}, {
        stdin: streamFrom([], false), stdout: nullOutput(), streamGet: async () => {},
        runRepl: async ({runTurn}) => {
          const pending = runTurn("investigate", abort.signal);
          await accepted.promise;
          abort.abort(new Error("operator stopped"));
          await pending;
          return 0;
        },
      }).pipe(Effect.provide(httpClientLayer), Effect.provide(config), Effect.provide(makeLayer(rec))),
    );
    const error = Exit.isFailure(result) ? Option.getOrNull(Cause.findErrorOption(result.cause)) : null;
    expect(error).toBeInstanceOf(InterruptedError);
    expect(error).toMatchObject({detail: "steer interrupted"});
    expect(error?.suggestion).toContain("an accepted message may still run");
    expect(posts).toBe(1);
  } finally {
    answer.resolve(Response.json({accepted: true}));
    await server.stop(true);
  }
});
