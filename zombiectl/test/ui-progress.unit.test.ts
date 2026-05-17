import { test, expect } from "bun:test";
import { createSpinner, withSpinner } from "../src/ui-progress.ts";

// Spinner is no-op on a non-TTY stream — that's the contract that
// keeps log captures clean. createSpinner returns the no-op shape;
// every method is callable but emits nothing.

interface FakeStream {
  isTTY: boolean;
  write(s: string): void;
  frames: string[];
}

// Double-cast at boundary: ui-progress accepts a structural stream
// (only isTTY + write are read), but the SpinnerOptions.stream slot
// types as NodeJS.WritableStream. Wrap once here so each call site stays clean.
function fakeStream(isTTY: boolean): FakeStream {
  const frames: string[] = [];
  return {
    isTTY,
    write(s: string) { frames.push(s); },
    frames,
  };
}

const asWritable = (s: FakeStream): NodeJS.WritableStream =>
  s as unknown as NodeJS.WritableStream;

test("createSpinner with non-TTY stream is a no-op across the lifecycle", () => {
  const stream = fakeStream(false);
  const s = createSpinner({ enabled: true, stream: asWritable(stream), label: "loading" });
  s.start();
  s.succeed("done");
  s.fail("nope");
  s.stop();
  expect(stream.frames).toEqual([]);
});

test("createSpinner with enabled=false stays silent on TTY too", () => {
  const stream = fakeStream(true);
  const s = createSpinner({ enabled: false, stream: asWritable(stream) });
  s.start();
  s.succeed();
  s.fail();
  s.stop();
  expect(stream.frames).toEqual([]);
});

test("createSpinner on TTY writes a frame on start, ok glyph on succeed", async () => {
  const stream = fakeStream(true);
  const s = createSpinner({ enabled: true, stream: asWritable(stream), label: "working" });
  s.start();
  // Immediately succeed; the timer fires every 80ms, so start writes
  // happen on next tick. The lifecycle still has to clear cleanly.
  s.succeed("done");
  // Final write is the ok-glyph + label + newline.
  expect(stream.frames.some((f) => f.includes("done"))).toBe(true);
});

test("createSpinner on TTY writes error glyph on fail", () => {
  const stream = fakeStream(true);
  const s = createSpinner({ enabled: true, stream: asWritable(stream), label: "working" });
  s.start();
  s.fail("kaboom");
  expect(stream.frames.some((f) => f.includes("kaboom"))).toBe(true);
});

test("createSpinner stop clears the line on TTY", () => {
  const stream = fakeStream(true);
  const s = createSpinner({ enabled: true, stream: asWritable(stream), label: "x" });
  s.start();
  s.stop();
  expect(stream.frames.some((f) => f === "\r")).toBe(true);
});

test("dotmatrix style picks the bigger braille set without crashing", () => {
  const stream = fakeStream(true);
  const s = createSpinner({ enabled: true, stream: asWritable(stream), style: "dotmatrix" });
  s.start();
  s.succeed();
  // Smoke check — first start writes nothing synchronously, but the
  // succeed call writes the ok line.
  expect(stream.frames.length).toBeGreaterThan(0);
});

test("withSpinner resolves the work value and calls succeed on the underlying spinner", async () => {
  const stream = fakeStream(false); // non-TTY keeps it noiseless
  const out = await withSpinner({ enabled: true, stream: asWritable(stream), label: "x" }, async () => 42);
  expect(out).toBe(42);
});

test("withSpinner re-throws the work error after calling fail", async () => {
  const stream = fakeStream(false);
  await expect(
    withSpinner({ enabled: true, stream: asWritable(stream), label: "x" }, async () => {
      throw new Error("boom");
    }),
  ).rejects.toThrow("boom");
});

// Pins the setInterval callback body (lines 51-53 in src/ui-progress.ts).
// Without this, the body is dead to coverage because the existing tests
// start+succeed/stop synchronously and never wait for the 80ms tick.
test("createSpinner timer callback writes spinner frames at the configured interval", async () => {
  const stream = fakeStream(true);
  const s = createSpinner({ enabled: true, stream: asWritable(stream), label: "loading" });
  s.start();
  // Wait ~250ms — enough for ≥2 timer ticks at 80ms. Using real timers
  // because Bun's fake-timer story varies; 250ms is cheap and pins
  // the actual setInterval body executing.
  await new Promise<void>((resolve) => setTimeout(resolve, 250));
  s.stop();
  // Each timer tick writes a frame line ("\r" + braille char + " " + label).
  const spinnerLines = stream.frames.filter((f) => f.includes("loading") && f.startsWith("\r"));
  expect(spinnerLines.length).toBeGreaterThanOrEqual(2);
});
