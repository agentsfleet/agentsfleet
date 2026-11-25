import {expect, test} from "bun:test";
import {Effect, Layer} from "effect";
import {startProgress} from "../src/output/progress.ts";
import {resetCapabilityWarning} from "../src/output/capability.ts";
import {CliConfig, resolveCliConfig} from "../src/services/config.ts";
import {HttpClient, httpClientWithProgressLayer} from "../src/services/http-client.ts";
import {Output} from "../src/services/output.ts";
import {asFetchImpl} from "./helpers.ts";
import {outputDouble} from "./helpers-output-double.ts";

test("returns the accepted HTTP write when the terminal animation fails", async () => {
  const draw = Promise.withResolvers<void>();
  const signal = Promise.withResolvers<AbortSignal | null | undefined>();
  const config = {...resolveCliConfig(), apiUrl: "https://api.example.test",
    fetchImpl: asFetchImpl(async (_url, init) => {
      signal.resolve(init?.signal);
      await draw.promise;
      await new Promise<void>(resolve => setImmediate(resolve));
      return Response.json({accepted: true});
    })};
  const output = {...outputDouble({stdoutIsTty: true}), progress: () => Effect.sync(() => {
    draw.resolve();
    throw new Error("Terminal disconnected");
  })};
  const layer = httpClientWithProgressLayer.pipe(Layer.provide(Layer.merge(
    Layer.succeed(CliConfig, config), Layer.succeed(Output, output),
  )));
  const result = await Effect.runPromise(Effect.gen(function* () {
    const client = yield* HttpClient;
    return yield* client.request({path: "/fixture", method: "POST", body: {accepted: true}});
  }).pipe(Effect.provide(layer)));
  expect(result).toEqual({accepted: true});
  expect((await signal.promise)?.aborted).toBe(false);
});

test("the terminal ball moves while waiting and stops writing after cleanup", async () => {
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(Effect.gen(function* () {
    yield* startProgress({isTTY: true, columns: 40, write: text => chunks.push(text)}, "Contacting platform…", true, {});
    yield* Effect.sleep(300);
    expect(chunks[0]).toBe("\r[o      ] Contacting platform…");
    expect(chunks[1]).toBe("\r[ o     ] Contacting platform…");
  })));
  expect(chunks.at(-1)).toMatch(/^\r +\r$/);
  const writes = chunks.length;
  await Bun.sleep(300);
  expect(chunks).toHaveLength(writes);
});

test.each([{AGENTSFLEET_REDUCED_MOTION: "1"}, {CI: "true"}, {TERM: "dumb"}])("shows a static status when animation is disabled by %j", async env => {
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(startProgress({isTTY: true, write: text => chunks.push(text)}, "Contacting platform…", true, env)));
  expect(chunks).toEqual(["Contacting platform…\n"]);
});

test.each([12, 30])("uses static progress when the terminal is only %s columns wide", async columns => {
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(startProgress({isTTY: true, columns, write: text => chunks.push(text)}, "Contacting platform…", true, {})));
  expect(chunks).toEqual(["Contacting platform…\n"]);
});

test.each([{}, {NO_COLOR: "1"}])("colors the ball through the shared palette and honors %j", async env => {
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(startProgress({isTTY: true, write: text => chunks.push(text)}, "Orbit", true, {TERM: "xterm-256color", ...env})));
  expect(chunks[0]).toBe(env.NO_COLOR ? "\r[o      ] Orbit" : "\r\u001b[38;5;79m[o      ]\u001b[0m Orbit");
});

test("keeps the basic-color fallback notice on the invocation's stream", async () => {
  resetCapabilityWarning();
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(startProgress({isTTY: true, write: text => chunks.push(text)}, "Orbit", true, {TERM: "vt100"})));
  expect(chunks[0]).toBe("note: terminal advertises <256 colors; using basic palette\n");
  expect(chunks[1]).toBe("\r\u001b[36m[o      ]\u001b[0m Orbit");
});

test.each([[false, true], [true, false], [true, undefined]] as const)("stays silent when disabled or redirected (enabled: %s, terminal: %s)", async (enabled, tty) => {
  const chunks: string[] = [];
  const stream = {write: (text: string) => chunks.push(text), ...(tty === undefined ? {} : {isTTY: tty})};
  await Effect.runPromise(Effect.scoped(startProgress(stream, "Contacting platform…", enabled, {})));
  expect(chunks).toEqual([]);
});

test.each([["failure", Effect.fail("Refused")], ["cancellation", Effect.interrupt]] as const)("releases the animation on request %s", async (_, outcome) => {
  const chunks: string[] = [];
  const result = await Effect.runPromiseExit(Effect.scoped(Effect.gen(function* () {
    yield* startProgress({isTTY: true, write: text => chunks.push(text)}, "Contacting platform…", true, {});
    yield* outcome;
  })));
  expect(result._tag).toBe("Failure");
  expect(chunks[0]).toContain("[o      ]");
  expect(chunks.at(-1)).toMatch(/^\r +\r$/);
  const writes = chunks.length;
  await Bun.sleep(300);
  expect(chunks).toHaveLength(writes);
});

test.each([false, true])("releases a status line that reflows after the terminal narrows (%s)", async redraw => {
  const chunks: string[] = [];
  const stream = {isTTY: true, columns: 80, write: (text: string) => chunks.push(text)};
  await Effect.runPromise(Effect.scoped(Effect.gen(function* () {
    yield* startProgress(stream, "Waiting for agentsfleet…", true, {});
    stream.columns = 20;
    if (redraw) yield* Effect.sleep(300);
  })));
  expect(chunks).toEqual(["\r[o      ] Waiting for agentsfleet…", "\r\n"]);
});

test("reverses the ball at the other edge of the lane", async () => {
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(Effect.gen(function* () {
    yield* startProgress({isTTY: true, columns: 80, write: text => chunks.push(text)}, "Working", true, {});
    yield* Effect.sleep(1150);
  })));
  expect(chunks.slice(5, 8)).toEqual(["\r[     o ] Working", "\r[      o] Working", "\r[     o ] Working"]);
});
