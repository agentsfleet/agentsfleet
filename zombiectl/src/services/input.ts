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

// Narrowed readline surface — just the two members the prompt path uses.
// Injecting the factory (rather than calling readline directly) lets a
// test drive the resolve / reject / close paths without a real terminal.
interface PromptInterface {
  question(query: string): Promise<string>;
  close(): void;
}
type CreateInterface = (opts: {
  input: NodeJS.ReadableStream;
  output: NodeJS.WritableStream;
}) => PromptInterface;

const defaultCreateInterface: CreateInterface = (opts) => readline.createInterface(opts);

export const makeLive = (
  createInterface: CreateInterface = defaultCreateInterface,
): InputShape => ({
  readLine: (prompt: string) =>
    Effect.promise(async () => {
      const rl = createInterface({ input: process.stdin, output: process.stdout });
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
