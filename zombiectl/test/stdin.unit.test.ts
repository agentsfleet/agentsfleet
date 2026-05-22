// Exercises the real Stdin implementation (makeLive → readStreamToEnd).
// The login/auth tests inject Stdin via Layer.succeed stubs, so the live
// drain path is otherwise uncovered. Also pins the listener-cleanup
// contract: every listener is removed on settle (no MaxListeners leak).

import { describe, test, expect } from "bun:test";
import { Effect } from "effect";
import { Readable } from "node:stream";
import { makeLive } from "../src/services/stdin.ts";

const readVia = (stream: Readable): Promise<string> =>
  Effect.runPromise(makeLive(stream).readToEnd);

describe("Stdin.readToEnd (makeLive)", () => {
  test("drains buffered chunks to the full utf8 text", async () => {
    const stream = Readable.from([Buffer.from("zmb_t_"), Buffer.from("abc\n")]);
    expect(await readVia(stream)).toBe("zmb_t_abc\n");
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
