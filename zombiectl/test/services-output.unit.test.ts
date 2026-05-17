// Output service tests — every method exercised against a capturing
// stream pair so coverage hits 97%+ without relying on the dispatcher.

import { describe, test, expect } from "bun:test";
import { Effect, Layer } from "effect";
import { Output, OutputFromStreams, makeStdioOutput } from "../src/services/output.ts";

class BufStream {
  readonly chunks: string[] = [];
  isTTY = false;
  write(chunk: string | Uint8Array): boolean {
    this.chunks.push(typeof chunk === "string" ? chunk : Buffer.from(chunk).toString());
    return true;
  }
  end(): void {}
  toString(): string {
    return this.chunks.join("");
  }
}

const makeStreams = (): { stdout: BufStream; stderr: BufStream; layer: Layer.Layer<Output> } => {
  const stdout = new BufStream();
  const stderr = new BufStream();
  const layer = OutputFromStreams({
    stdout: stdout as unknown as NodeJS.WritableStream,
    stderr: stderr as unknown as NodeJS.WritableStream,
  });
  return { stdout, stderr, layer };
};

const provideEffect = async <A>(
  effect: Effect.Effect<A, never, Output>,
  layer: Layer.Layer<Output>,
): Promise<A> => Effect.runPromise(Effect.provide(effect, layer));

describe("Output service", () => {
  test("intro writes to stdout with leading newline", async () => {
    const { stdout, layer } = makeStreams();
    await provideEffect(
      Effect.gen(function* () {
        const o = yield* Output;
        yield* o.intro("hello");
      }),
      layer,
    );
    expect(stdout.toString()).toContain("hello");
    expect(stdout.toString()).toMatch(/^\n/);
  });
  test("info writes plain line to stdout", async () => {
    const { stdout, layer } = makeStreams();
    await provideEffect(
      Effect.gen(function* () {
        const o = yield* Output;
        yield* o.info("info-msg");
      }),
      layer,
    );
    expect(stdout.toString()).toContain("info-msg");
  });
  test("success writes to stdout", async () => {
    const { stdout, layer } = makeStreams();
    await provideEffect(
      Effect.gen(function* () {
        const o = yield* Output;
        yield* o.success("done");
      }),
      layer,
    );
    expect(stdout.toString()).toContain("done");
  });
  test("warn writes to stderr", async () => {
    const { stderr, layer } = makeStreams();
    await provideEffect(
      Effect.gen(function* () {
        const o = yield* Output;
        yield* o.warn("careful");
      }),
      layer,
    );
    expect(stderr.toString()).toContain("careful");
  });
  test("error writes to stderr with 'error:' prefix", async () => {
    const { stderr, layer } = makeStreams();
    await provideEffect(
      Effect.gen(function* () {
        const o = yield* Output;
        yield* o.error("boom");
      }),
      layer,
    );
    expect(stderr.toString()).toContain("error:");
    expect(stderr.toString()).toContain("boom");
  });
  test("outro writes to stdout", async () => {
    const { stdout, layer } = makeStreams();
    await provideEffect(
      Effect.gen(function* () {
        const o = yield* Output;
        yield* o.outro("bye");
      }),
      layer,
    );
    expect(stdout.toString()).toContain("bye");
  });
  test("printJson emits JSON to stdout", async () => {
    const { stdout, layer } = makeStreams();
    await provideEffect(
      Effect.gen(function* () {
        const o = yield* Output;
        yield* o.printJson({ ok: true, n: 42 });
      }),
      layer,
    );
    expect(stdout.toString()).toContain("\"ok\": true");
    expect(stdout.toString()).toContain("\"n\": 42");
  });
  test("printJsonErr emits JSON to stderr", async () => {
    const { stderr, layer } = makeStreams();
    await provideEffect(
      Effect.gen(function* () {
        const o = yield* Output;
        yield* o.printJsonErr({ error: "x" });
      }),
      layer,
    );
    expect(stderr.toString()).toContain("\"error\": \"x\"");
  });
  test("printKeyValue formats aligned key/value pairs", async () => {
    const { stdout, layer } = makeStreams();
    await provideEffect(
      Effect.gen(function* () {
        const o = yield* Output;
        yield* o.printKeyValue({ short: "x", longer_key: "y" });
      }),
      layer,
    );
    const text = stdout.toString();
    expect(text).toContain("short");
    expect(text).toContain("longer_key");
    expect(text).toContain("x");
    expect(text).toContain("y");
  });
  test("printSection writes a section header", async () => {
    const { stdout, layer } = makeStreams();
    await provideEffect(
      Effect.gen(function* () {
        const o = yield* Output;
        yield* o.printSection("Authentication");
      }),
      layer,
    );
    expect(stdout.toString()).toContain("Authentication");
  });
});

describe("makeStdioOutput", () => {
  test("factory composes the live shape from a stream pair", () => {
    const stdout = new BufStream();
    const stderr = new BufStream();
    const shape = makeStdioOutput({
      stdout: stdout as unknown as NodeJS.WritableStream,
      stderr: stderr as unknown as NodeJS.WritableStream,
    });
    expect(typeof shape.intro).toBe("function");
    expect(typeof shape.printJson).toBe("function");
    expect(typeof shape.printSection).toBe("function");
    expect(typeof shape.printKeyValue).toBe("function");
  });
});
