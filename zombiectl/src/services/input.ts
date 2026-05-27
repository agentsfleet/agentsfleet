// Input service — readline-backed prompt surface for interactive
// commands. Fronts `node:readline/promises` so login can ask for the
// verification code without commands importing readline directly. The
// service abstraction also gives tests a Layer.succeed seam: queue
// expected responses, run the Effect, assert on the captured prompts.

import { Context, Effect, Layer } from "effect";
import * as readline from "node:readline/promises";

export interface InputShape {
  // Writes `prompt` (no newline) to stdout and reads one line from stdin.
  // Returns the line ("" for a bare Enter), or `null` when stdin is closed
  // (EOF / Ctrl-D) or the prompt is aborted via `signal` (SIGINT). Callers
  // distinguish the two: "" → re-prompt locally; null → cancel cleanly.
  readonly readLine: (prompt: string, signal?: AbortSignal) => Effect.Effect<string | null>;
}

export class Input extends Context.Service<Input, InputShape>()(
  "zombiectl/runtime/Input",
) {}

// Narrowed readline surface — just the two members the prompt path uses.
// Injecting the factory (rather than calling readline directly) lets a
// test drive the resolve / reject / close paths without a real terminal.
interface PromptInterface {
  question(query: string, options?: { signal?: AbortSignal }): Promise<string>;
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
  readLine: (prompt: string, signal?: AbortSignal) =>
    Effect.promise(async (): Promise<string | null> => {
      const rl = createInterface({ input: process.stdin, output: process.stdout });
      try {
        return await rl.question(prompt, signal ? { signal } : undefined);
      } catch {
        // AbortError (SIGINT via signal) or a closed stdin (EOF) — the
        // caller reads null as "no line", never an empty answer.
        return null;
      } finally {
        rl.close();
      }
    }),
});

export const inputLayer: Layer.Layer<Input> = Layer.succeed(Input, Input.of(makeLive()));
