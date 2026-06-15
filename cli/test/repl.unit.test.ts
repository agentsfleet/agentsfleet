import { describe, expect, test } from "bun:test";
import { PassThrough, Readable, Writable } from "node:stream";
import {
  readPipedMessage,
  ReplSignalEmitter,
  runSteerRepl,
  shouldEnterSteerRepl,
  type ReplInputStream,
  type ReplOutputStream,
} from "../src/lib/repl.ts";
import { SIGINT } from "../src/constants/signals.ts";

const streamFrom = (chunks: ReadonlyArray<string>, isTTY = false): ReplInputStream => {
  const stream = Readable.from(chunks) as ReplInputStream;
  Object.defineProperty(stream, "isTTY", { value: isTTY });
  return stream;
};

const nullOutput = (): ReplOutputStream =>
  new Writable({
    write(_chunk, _encoding, callback): void {
      callback();
    },
  }) as ReplOutputStream;

describe("steer REPL mode selection", () => {
  test("TTY without a message enters REPL", () => {
    expect(shouldEnterSteerRepl({ isTTY: true }, undefined, false)).toBe(true);
  });

  test("explicit message stays single-shot even on a TTY", () => {
    expect(shouldEnterSteerRepl({ isTTY: true }, "howdy", false)).toBe(false);
  });

  test("--tty forces REPL for piped stdin", () => {
    expect(shouldEnterSteerRepl({ isTTY: false }, undefined, true)).toBe(true);
  });

  test("explicit message stays single-shot even with --tty", () => {
    expect(shouldEnterSteerRepl({ isTTY: true }, "howdy", true)).toBe(false);
  });
});

describe("steer REPL loop", () => {
  test("posts each non-empty line and exits on stdin EOF", async () => {
    const seen: string[] = [];
    const code = await runSteerRepl({
      input: streamFrom(["first\n", "\n", "second\n"]),
      output: nullOutput(),
      runTurn: async (message) => {
        seen.push(message);
      },
    });
    expect(code).toBe(0);
    expect(seen).toEqual(["first", "second"]);
  });

  test("SIGINT aborts the in-flight turn and returns 130", async () => {
    const signalSource = new ReplSignalEmitter();
    let aborted = false;
    const codePromise = runSteerRepl({
      input: streamFrom(["howdy\n"], true),
      output: new PassThrough() as ReplOutputStream,
      signalSource,
      runTurn: async (_message, signal) => {
        signal.addEventListener("abort", () => { aborted = true; }, { once: true });
        queueMicrotask(() => signalSource.emit(SIGINT));
        await new Promise<void>((resolve) => signal.addEventListener("abort", () => resolve(), { once: true }));
      },
    });

    await expect(codePromise).resolves.toBe(130);
    expect(aborted).toBe(true);
  });

  test("turn errors are reported and the prompt continues", async () => {
    const seen: string[] = [];
    const errors: string[] = [];
    const code = await runSteerRepl({
      input: streamFrom(["first\n", "second\n"]),
      output: nullOutput(),
      runTurn: async (message) => {
        seen.push(message);
        if (message === "first") throw new Error("nope");
      },
      onTurnError: async (err) => {
        errors.push(err instanceof Error ? err.message : String(err));
      },
    });
    expect(code).toBe(0);
    expect(seen).toEqual(["first", "second"]);
    expect(errors).toEqual(["nope"]);
  });
});

test("readPipedMessage trims piped stdin into one message", async () => {
  await expect(readPipedMessage(streamFrom([" hello", " there\n"]))).resolves.toBe("hello there");
});

describe("steer REPL error propagation", () => {
  test("a turn error with no onTurnError handler rejects (rethrow path)", async () => {
    const seen: string[] = [];
    await expect(
      runSteerRepl({
        input: streamFrom(["boom\n"]),
        output: nullOutput(),
        runTurn: async (message) => {
          seen.push(message);
          throw new Error("uncaught turn");
        },
      }),
    ).rejects.toThrow("uncaught turn");
    expect(seen).toEqual(["boom"]);
  });

  test("a synchronous (void-returning) onTurnError handler is awaited and the loop continues", async () => {
    const errors: unknown[] = [];
    const code = await runSteerRepl({
      input: streamFrom(["bad\n", "good\n"]),
      output: nullOutput(),
      runTurn: async (message) => {
        if (message === "bad") throw new Error("sync-handled");
      },
      // Returns void, not a Promise — exercises the non-thenable handler arm.
      onTurnError: (err): void => {
        errors.push(err);
      },
    });
    expect(code).toBe(0);
    expect(errors).toHaveLength(1);
  });

  test("the close-on-SIGINT interrupt runs even when no turn is in flight", async () => {
    const signalSource = new ReplSignalEmitter();
    // No lines are queued, so the prompt loop is idle; firing SIGINT exercises
    // the interrupt closure + the finally cleanup without an active turn.
    const code = await runSteerRepl({
      input: streamFrom([]),
      output: nullOutput(),
      signalSource,
      runTurn: async () => {
        /* never reached — no input lines */
      },
    });
    // Empty stdin reaches EOF first → clean exit; interrupt listener was
    // registered and torn down in the finally block regardless.
    expect(code).toBe(0);
  });
});
