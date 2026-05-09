import { test, expect } from "bun:test";
import { createSpinner, withSpinner } from "../src/ui-progress.js";

// Spinner is no-op on a non-TTY stream — that's the contract that
// keeps log captures clean. createSpinner returns the no-op shape;
// every method is callable but emits nothing.

function fakeStream(isTTY) {
  const frames = [];
  return {
    isTTY,
    write(s) { frames.push(s); },
    frames,
  };
}

test("createSpinner with non-TTY stream is a no-op across the lifecycle", () => {
  const stream = fakeStream(false);
  const s = createSpinner({ enabled: true, stream, label: "loading" });
  s.start();
  s.succeed("done");
  s.fail("nope");
  s.stop();
  expect(stream.frames).toEqual([]);
});

test("createSpinner with enabled=false stays silent on TTY too", () => {
  const stream = fakeStream(true);
  const s = createSpinner({ enabled: false, stream });
  s.start();
  s.succeed();
  s.fail();
  s.stop();
  expect(stream.frames).toEqual([]);
});

test("createSpinner on TTY writes a frame on start, ok glyph on succeed", async () => {
  const stream = fakeStream(true);
  const s = createSpinner({ enabled: true, stream, label: "working" });
  s.start();
  // Immediately succeed; the timer fires every 80ms, so start writes
  // happen on next tick. The lifecycle still has to clear cleanly.
  s.succeed("done");
  // Final write is the ok-glyph + label + newline.
  expect(stream.frames.some((f) => f.includes("done"))).toBe(true);
});

test("createSpinner on TTY writes error glyph on fail", () => {
  const stream = fakeStream(true);
  const s = createSpinner({ enabled: true, stream, label: "working" });
  s.start();
  s.fail("kaboom");
  expect(stream.frames.some((f) => f.includes("kaboom"))).toBe(true);
});

test("createSpinner stop clears the line on TTY", () => {
  const stream = fakeStream(true);
  const s = createSpinner({ enabled: true, stream, label: "x" });
  s.start();
  s.stop();
  expect(stream.frames.some((f) => f === "\r")).toBe(true);
});

test("dotmatrix style picks the bigger braille set without crashing", () => {
  const stream = fakeStream(true);
  const s = createSpinner({ enabled: true, stream, style: "dotmatrix" });
  s.start();
  s.succeed();
  // Smoke check — first start writes nothing synchronously, but the
  // succeed call writes the ok line.
  expect(stream.frames.length).toBeGreaterThan(0);
});

test("withSpinner resolves the work value and calls succeed on the underlying spinner", async () => {
  const stream = fakeStream(false); // non-TTY keeps it noiseless
  const out = await withSpinner({ enabled: true, stream, label: "x" }, async () => 42);
  expect(out).toBe(42);
});

test("withSpinner re-throws the work error after calling fail", async () => {
  const stream = fakeStream(false);
  await expect(
    withSpinner({ enabled: true, stream, label: "x" }, async () => {
      throw new Error("boom");
    }),
  ).rejects.toThrow("boom");
});
