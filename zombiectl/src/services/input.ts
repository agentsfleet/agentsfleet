// Input service — readline-backed prompt surface for interactive
// commands. Fronts `node:readline/promises` so login can ask for the
// verification code without commands importing readline directly. The
// service abstraction also gives tests a Layer.succeed seam: queue
// expected responses, run the Effect, assert on the captured prompts.

import { Context, Effect, Layer } from "effect";
import * as readline from "node:readline/promises";

export interface InputShape {
  // Writes `prompt` (no newline) to stdout, reads a line from stdin,
  // returns the line trimmed of a trailing newline. Empty string when
  // the user just presses Enter.
  readonly readLine: (prompt: string) => Effect.Effect<string>;
}

export class Input extends Context.Service<Input, InputShape>()(
  "zombiectl/runtime/Input",
) {}

const makeLive = (): InputShape => ({
  readLine: (prompt: string) =>
    Effect.promise(async () => {
      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
      });
      try {
        return await rl.question(prompt);
      } catch {
        return "";
      } finally {
        rl.close();
      }
    }),
});

export const inputLayer: Layer.Layer<Input> = Layer.succeed(Input, Input.of(makeLive()));
