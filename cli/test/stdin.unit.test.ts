// Exercises the real Stdin implementation (makeLive → readStreamToEnd).
// The login/auth tests inject Stdin via Layer.succeed stubs, so the live
// drain path is otherwise uncovered. Also pins the listener-cleanup
// contract: every listener is removed on settle (no MaxListeners leak).

import { describe, test, expect } from "bun:test";
import { Effect } from "effect";
import { Readable } from "node:stream";
import {
  makeLive,
  stdinLayer,
  stdinFromStreamLayer,
  Stdin,
} from "../src/services/stdin.ts";

const readVia = (stream: Readable): Promise<string> =>
  Effect.runPromise(makeLive(stream).readToEnd);

describe("Stdin.readToEnd (makeLive)", () => {
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

  test("rejects and cleans up listeners on stream error", async () => {
    const stream = new Readable({
      read() {
        this.destroy(new Error("boom"));
      },
    });
    let threw = false;
    try {
      await readVia(stream);
    } catch {
      threw = true;
    }
    expect(threw).toBe(true);
    expect(stream.listenerCount("data")).toBe(0);
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
          const text = yield* s.readToEnd;
          return { text, isTTY: s.isTTY };
        }),
        stdinFromStreamLayer(stream),
      ),
    );
    expect(result.text).toBe("agt_tpiped\n");
    expect(result.isTTY).toBe(false);
  });

  test("stdinLayer (process.stdin default) resolves an isTTY boolean", async () => {
    // Only read isTTY — draining the real process.stdin would block the
    // runner. Constructing + resolving the layer is what counts the
    // makeLive() default-argument closure.
    const isTTY = await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const s = yield* Stdin;
          return s.isTTY;
        }),
        stdinLayer,
      ),
    );
    expect(typeof isTTY).toBe("boolean");
  });
});
