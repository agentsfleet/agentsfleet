// Exercises the real Stdin implementation (makeLive → readStreamToEnd).
// The login/auth tests inject Stdin via Layer.succeed stubs, so the live
// drain path is otherwise uncovered. Also pins the listener-cleanup
// contract: every listener is removed on settle (no MaxListeners leak).

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Option } from "effect";
import { Readable } from "node:stream";
import { UnexpectedError, ValidationError } from "../src/errors/index.ts";
import {
  makeLive,
  stdinFromStreamLayer,
  Stdin,
} from "../src/services/stdin.ts";

const readVia = (stream: Readable): Promise<string> =>
  Effect.runPromise(makeLive(stream).readBounded(1024));

describe("bounded standard input", () => {
  test("accepts multibyte input exactly at the byte limit", async () => {
    const stream = Readable.from(["🌱", Buffer.from("é")]);
    expect(await Effect.runPromise(makeLive(stream).readBounded(6))).toBe("🌱é");
    expect(stream.listenerCount("data")).toBe(0);
  });

  test("refuses excess bytes without waiting for EOF and releases the stream", async () => {
    let sent = false;
    const stream = new Readable({read() {if (!sent) {sent = true; this.push(Buffer.from("🌱é"));}}});
    try {
      const exit = await Effect.runPromiseExit(makeLive(stream).readBounded(4));
      expect(Exit.isFailure(exit)).toBe(true);
      if (Exit.isFailure(exit)) {
        const error = Option.getOrNull(Cause.findErrorOption(exit.cause));
        expect(error).toBeInstanceOf(ValidationError);
        expect(error).toMatchObject({_tag: "ValidationError", detail: "Standard input exceeds 4 bytes", suggestion: "pipe a smaller input"});
      }
      expect(stream.readableEnded).toBe(false);
      expect(stream.isPaused()).toBe(true);
      for (const event of ["data", "end", "error"]) expect(stream.listenerCount(event)).toBe(0);
    } finally {stream.destroy();}
  });

  test("reports an input failure without exposing its contents", async () => {
    const stream = new Readable({read() {this.destroy(new Error("private pipe contents"));}});
    const exit = await Effect.runPromiseExit(makeLive(stream).readBounded(4));
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const error = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(error).toBeInstanceOf(UnexpectedError);
      expect(error).toMatchObject({_tag: "UnexpectedError", detail: "Could not read standard input", suggestion: "check the input pipe and try again"});
    }
    for (const event of ["data", "end", "error"]) expect(stream.listenerCount(event)).toBe(0);
  });

  test("releases a waiting input when the caller cancels", async () => {
    const stream = new Readable({read() {}});
    const controller = new AbortController();
    const pending = Effect.runPromiseExit(makeLive(stream).readBounded(4), {signal: controller.signal});
    await Promise.resolve(); controller.abort();
    const exit = await pending;
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) expect(Cause.hasInterruptsOnly(exit.cause)).toBe(true);
    expect(stream.isPaused()).toBe(true);
    for (const event of ["data", "end", "error"]) expect(stream.listenerCount(event)).toBe(0);
    stream.destroy();
  });
});

describe("Stdin.readBounded (makeLive)", () => {
  test("drains buffered chunks to the full utf8 text", async () => {
    const stream = Readable.from([Buffer.from("agt_t"), Buffer.from("abc\n")]);
    expect(await readVia(stream)).toBe("agt_tabc\n");
  });

  test("coerces string chunks to utf8 (mixed string/Buffer input)", async () => {
    const stream = Readable.from(["héllo ", Buffer.from("wörld")]);
    expect(await readVia(stream)).toBe("héllo wörld");
  });

  test("empty stream resolves to the empty string", async () => {
    expect(await readVia(Readable.from([] as Buffer[]))).toBe("");
  });

  test("removes every listener after end (no MaxListeners leak)", async () => {
    const stream = Readable.from([Buffer.from("x")]);
    await readVia(stream);
    expect(stream.listenerCount("data")).toBe(0);
    expect(stream.listenerCount("end")).toBe(0);
    expect(stream.listenerCount("error")).toBe(0);
  });

  test("isTTY reflects the underlying stream", () => {
    expect(makeLive(Readable.from([Buffer.from("x")])).isTTY).toBe(false);
  });
});

// The two Layer factories (stdinLayer, stdinFromStreamLayer) are wired
// into runCli in production but bypassed by the login/auth tests, which
// inject Stdin via Layer.succeed stubs. Resolve each through Effect so
// the layer-construction closures are counted.
describe("Stdin layer factories", () => {
  test("stdinFromStreamLayer threads a custom stream's drain + isTTY", async () => {
    const stream = Readable.from([Buffer.from("agt_tpiped\n")]);
    const result = await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const s = yield* Stdin;
          const text = yield* s.readBounded(1024);
          return { text, isTTY: s.isTTY };
        }),
        stdinFromStreamLayer(stream),
      ),
    );
    expect(result.text).toBe("agt_tpiped\n");
    expect(result.isTTY).toBe(false);
  });
});
