import {expect, test} from "bun:test";
import {Effect} from "effect";
import {startProgress} from "../src/output/progress.ts";
import {resetCapabilityWarning} from "../src/output/capability.ts";

test("the terminal scout flaps while waiting and stops writing after cleanup", async () => {
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(Effect.gen(function* () {
    yield* startProgress({isTTY: true, columns: 31, write: text => chunks.push(text)}, "Contacting platform…", true, {});
    yield* Effect.sleep(300);
    expect(chunks[0]).toBe("\r ,(o,o),  Contacting platform…");
    expect(chunks[1]).toBe("\r /(o,o)\\  Contacting platform…");
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

test.each([{}, {NO_COLOR: "1"}])("colors the owl through the shared palette and honors %j", async env => {
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(startProgress({isTTY: true, write: text => chunks.push(text)}, "Scout", true, {TERM: "xterm-256color", ...env})));
  expect(chunks[0]).toBe(env.NO_COLOR ? "\r ,(o,o),  Scout" : "\r\u001b[38;5;79m ,(o,o), \u001b[0m Scout");
});

test("keeps the basic-color fallback notice on the invocation's stream", async () => {
  resetCapabilityWarning();
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(startProgress({isTTY: true, write: text => chunks.push(text)}, "Scout", true, {TERM: "vt100"})));
  expect(chunks[0]).toBe("note: terminal advertises <256 colors; using basic palette\n");
  expect(chunks[1]).toBe("\r\u001b[36m ,(o,o), \u001b[0m Scout");
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
  expect(chunks[0]).toContain("(o,o)");
  expect(chunks.at(-1)).toMatch(/^\r +\r$/);
  const writes = chunks.length;
  await Bun.sleep(300);
  expect(chunks).toHaveLength(writes);
});
