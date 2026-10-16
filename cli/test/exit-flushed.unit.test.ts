import { describe, expect, test } from "bun:test";
import { type EndableStream, exitFlushed, flushed } from "../src/lib/exit-flushed.ts";

const MODULE_PATH = new URL("../src/lib/exit-flushed.ts", import.meta.url).pathname;
/** Far past the 128 KiB a piped write had reached when `process.exit` cut it. */
const LARGE_OUTPUT_BYTES = 2_000_000;
const CHILD_EXIT_CODE = 7;

/** A stream whose `end` callback fires only when the test says so. */
const heldStream = () => {
  let release: () => void = () => {};
  let ended = false;
  const stream: EndableStream = {
    end(callback) {
      ended = true;
      release = callback;
    },
  };
  return { stream, release: () => release(), wasEnded: () => ended };
};

class Exited extends Error {
  constructor(readonly code: number) {
    super(`exited with ${code}`);
  }
}

const recordingExit = (codes: number[]) => (code: number): never => {
  codes.push(code);
  throw new Exited(code);
};

describe("flushed", () => {
  test("ends every stream and waits for each to flush", async () => {
    const first = heldStream();
    const second = heldStream();
    let settled = false;
    const done = flushed([first.stream, second.stream]).then(() => {
      settled = true;
    });

    await Promise.resolve();
    expect(first.wasEnded() && second.wasEnded()).toBe(true);
    first.release();
    await Promise.resolve();
    expect(settled).toBe(false);

    second.release();
    await done;
    expect(settled).toBe(true);
  });
});

describe("exitFlushed", () => {
  test("exits with the code only after every stream has flushed", async () => {
    const stdout = heldStream();
    const stderr = heldStream();
    const codes: number[] = [];
    const exiting = exitFlushed(CHILD_EXIT_CODE, [stdout.stream, stderr.stream], recordingExit(codes));

    await Promise.resolve();
    expect(codes).toEqual([]);
    stdout.release();
    stderr.release();

    await expect(exiting).rejects.toBeInstanceOf(Exited);
    expect(codes).toEqual([CHILD_EXIT_CODE]);
  });

  test("a large piped write reaches its reader whole, with the exit code kept", async () => {
    const script = `
      import { exitFlushed } from ${JSON.stringify(MODULE_PATH)};
      process.stdout.write("o".repeat(${LARGE_OUTPUT_BYTES}));
      process.stderr.write("e".repeat(${LARGE_OUTPUT_BYTES}));
      await exitFlushed(${CHILD_EXIT_CODE}, [process.stdout, process.stderr]);
    `;
    const child = Bun.spawn([process.execPath, "-e", script], { stdout: "pipe", stderr: "pipe" });

    const [stdout, stderr, code] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);

    expect(stdout.length).toBe(LARGE_OUTPUT_BYTES);
    expect(stderr.length).toBe(LARGE_OUTPUT_BYTES);
    expect(code).toBe(CHILD_EXIT_CODE);
  });
});
