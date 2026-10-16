import { expect, test } from "bun:test";
import { EventEmitter } from "node:events";
import { Effect } from "effect";
import { stopOnInterrupt } from "../../src/commands/chat/interrupt.ts";
import { ValidationError } from "../../src/errors/index.ts";
import { outputDouble } from "../helpers-output-double.ts";

test("Ctrl+C requests one durable Stop and disposes its listener after the followed answer settles", async () => {
  const source = new EventEmitter();
  const ready = Promise.withResolvers<void>();
  const cancelled = Promise.withResolvers<void>();
  let stops = 0;
  const follow = Effect.promise(async () => { ready.resolve(); await cancelled.promise; return "cancelled"; });
  const stop = Effect.sync(() => { stops++; cancelled.resolve(); });
  const running = Effect.runPromise(stopOnInterrupt(follow, stop, outputDouble(), source));
  await ready.promise;
  expect(source.listenerCount("SIGINT")).toBe(1);
  source.emit("SIGINT"); source.emit("SIGINT");
  expect(await running).toBe("cancelled");
  expect(stops).toBe(1);
  expect(source.listenerCount("SIGINT")).toBe(0);
  source.emit("SIGINT"); expect(stops).toBe(1);
});

test("a failed follow removes the handler and preserves its typed failure", async () => {
  const source = new EventEmitter();
  const failure = new ValidationError({detail: "access lost", suggestion: "sign in"});
  const result = await Effect.runPromise(Effect.flip(stopOnInterrupt(Effect.fail(failure), Effect.void, outputDouble(), source)));
  expect(result).toBe(failure);
  expect(source.listenerCount("SIGINT")).toBe(0);
});

test("a refused Stop is reported without inventing a cancelled answer", async () => {
  const source = new EventEmitter();
  const ready = Promise.withResolvers<void>();
  const reported = Promise.withResolvers<void>();
  const warnings: string[] = [];
  const output = {...outputDouble(), warn: (text: string) => Effect.sync(() => { warnings.push(text); reported.resolve(); })};
  const follow = Effect.promise(async () => { ready.resolve(); await reported.promise; return "still pending"; });
  const stop = Effect.fail(new ValidationError({detail: "Stop refused", suggestion: "refresh"}));
  const running = Effect.runPromise(stopOnInterrupt(follow, stop, output, source));
  await ready.promise; source.emit("SIGINT");
  expect(await running).toBe("still pending");
  expect(warnings).toEqual(["Stop refused. refresh"]);
  expect(source.listenerCount("SIGINT")).toBe(0);
});

test("should abandon an in-flight Stop once the followed answer settles first", async () => {
  const source = new EventEmitter();
  const ready = Promise.withResolvers<void>();
  const stopping = Promise.withResolvers<void>();
  let abandoned = false;
  const follow = Effect.promise(async () => { ready.resolve(); await stopping.promise; return "answered"; });
  const stop = Effect.sync(() => { stopping.resolve(); }).pipe(Effect.andThen(Effect.never),
    Effect.onInterrupt(() => Effect.sync(() => { abandoned = true; })));
  const running = Effect.runPromise(stopOnInterrupt(follow, stop, outputDouble(), source));
  await ready.promise; source.emit("SIGINT");
  expect(await running).toBe("answered");
  expect(abandoned).toBe(true);
  expect(source.listenerCount("SIGINT")).toBe(0);
});
