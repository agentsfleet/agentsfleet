// Input service surface test. The live readline-backed implementation
// can't run against stdin/stdout inside bun:test (would block the
// runner waiting for keystrokes), so what we exercise here is the
// Service shape itself + a hand-rolled Layer.succeed fake — the same
// pattern login-device-flow tests in the dimension batch will use.

import { describe, test, expect } from "bun:test";
import { Effect, Layer } from "effect";
import { Input, inputLayer } from "../src/services/input.ts";

describe("Input service", () => {
  test("readLine returns whatever the underlying impl resolves with", async () => {
    const captured: { prompt: string | null } = { prompt: null };
    const fakeLayer: Layer.Layer<Input> = Layer.succeed(
      Input,
      Input.of({
        readLine: (prompt) =>
          Effect.sync(() => {
            captured.prompt = prompt;
            return "user-typed-answer";
          }),
      }),
    );
    const program = Effect.gen(function* () {
      const input = yield* Input;
      return yield* input.readLine("Enter code: ");
    });
    const result = await Effect.runPromise(program.pipe(Effect.provide(fakeLayer)));
    expect(result).toBe("user-typed-answer");
    expect(captured.prompt).toBe("Enter code: ");
  });

  test("inputLayer is a valid Layer.Layer<Input> that composes", () => {
    // Pure compile-time check: if the type signature drifts (e.g. someone
    // accidentally narrows Input's tag), this assignment fails at typecheck.
    const _check: Layer.Layer<Input> = inputLayer;
    expect(_check).toBeDefined();
  });
});
