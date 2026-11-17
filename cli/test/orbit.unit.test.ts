import {expect, test} from "bun:test";
import {Effect} from "effect";
import {ColorMode} from "../src/output/capability.ts";
import {orbitRow, terminal} from "../src/output/palette.ts";
import {orbitFrame} from "../src/output/orbit.ts";
import {startProgress} from "../src/output/progress.ts";
import {PET_DISPLAY, PET_FRAME_MS, PET_ROWS, PET_STATE} from "../src/constants/pet.ts";

const ESC = "\u001b[";
const COLOR_SEQUENCE = /\u001b\[[\d;]*m/g;
const OPTIONS = {env: {TERM: "xterm-256color"}, mode: ColorMode.XTERM256};

test("Orbit has bounded, distinct idle, working, blinking and celebration frames", () => {
  const idle = orbitFrame(0, PET_STATE.idle, OPTIONS);
  expect(idle).toHaveLength(18);
  expect(idle.join("")).toContain("Just floating.");
  expect(orbitFrame(0, PET_STATE.working, OPTIONS).join("")).toContain("On the lookout.");
  expect(orbitFrame(0, PET_STATE.done, OPTIONS).join("")).toContain("A little victory dance.");
  expect(orbitFrame(24, PET_STATE.idle, OPTIONS)[5]).not.toBe(idle[5]);
  for (const state of Object.values(PET_STATE)) {
    const frames = new Set<string>();
    for (let tick = 0; tick < 40; tick += 1) {
      const frame = orbitFrame(tick, state, OPTIONS);
      expect(frame).toHaveLength(18);
      for (const row of frame) {
        expect(row.replace(COLOR_SEQUENCE, "").length).toBeLessThan(61);
        expect(row).not.toContain("undefined");
      }
      frames.add(frame.join("\n"));
    }
    expect(frames.size).toBeGreaterThan(10);
  }
});

test("truecolor pixels preserve the terminal background around half blocks", () => {
  expect(orbitRow(["M", null, null], ["L", "M", null], {env: {COLORTERM: "truecolor"}, mode: ColorMode.XTERM256})).toBe(
    `${ESC}38;2;160;210;223m${ESC}48;2;200;233;236m▀${ESC}0m${ESC}38;2;160;210;223m${ESC}49m▄${ESC}0m `);
});

test.each([ColorMode.NONE, ColorMode.BASIC16])("unsupported pixel color mode %s emits plain characters", mode => {
  expect(orbitRow(["M", null, "L", null], ["L", "M", null, null], {mode})).toBe("#_^ ");
});

test("a forced 256-color palette stays indexed in a truecolor terminal", () => {
  expect(orbitRow(["M"], [null], {env: {FORCE_COLOR: "2", COLORTERM: "truecolor"}, mode: ColorMode.XTERM256}))
    .toBe(`${ESC}38;5;152m${ESC}49m▀${ESC}0m`);
});

test("redraw and cleanup return to their owned rows without erasing the screen", () => {
  expect(terminal.reserveRows(2)).toBe("\r\n\r\n");
  expect(terminal.repaintRows(["one", "two"])).toBe(`${ESC}2A\r${ESC}2Kone\r\n${ESC}2Ktwo\r\n`);
  expect(terminal.clearRows(2)).toBe(`${ESC}2A\r${ESC}2K\r\n${ESC}2K${ESC}1A\r`);
});

test.each([{columns: 60, rows: 24}, {columns: 80, rows: 19}, {}])("uses compact Orbit when the preview does not fit: %j", async size => {
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(startProgress({isTTY: true, ...size, write: text => chunks.push(text)},
    "Orbit", true, {TERM: "xterm-256color"}, PET_DISPLAY.preview)));
  expect(chunks[0]).toContain("(o.o)~");
  expect(chunks.join("")).not.toContain(`${ESC}18A`);
  expect(chunks.at(-1)).toMatch(/^\r +\r$/);
});

test.each([{NO_COLOR: "1"}, {TERM: "vt100"}])("uses compact Orbit for a limited color palette: %j", async env => {
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(startProgress({isTTY: true, columns: 80, rows: 24, write: text => chunks.push(text)},
    "Orbit", true, {TERM: "xterm-256color", ...env}, PET_DISPLAY.preview)));
  expect(chunks.join("")).toContain("(o.o)~");
  expect(chunks.join("")).not.toContain("▀");
  if ("NO_COLOR" in env) expect(chunks.join("")).not.toContain(ESC);
});

test.each([{AGENTSFLEET_REDUCED_MOTION: "1"}, {CI: "true"}, {TERM: "dumb"}])("preview is static when animation is disabled by %j", async env => {
  const chunks: string[] = [];
  await Effect.runPromise(Effect.scoped(startProgress({isTTY: true, columns: 80, rows: 24, write: text => chunks.push(text)},
    "Orbit", true, {TERM: "xterm-256color", ...env}, PET_DISPLAY.preview)));
  expect(chunks).toEqual([" (o.o)~  Orbit\n"]);
});

test.each([Effect.void, Effect.fail("refused"), Effect.interrupt])("full preview clears and stops on every scope exit", async outcome => {
  const chunks: string[] = [];
  await Effect.runPromiseExit(Effect.scoped(Effect.gen(function* () {
    yield* startProgress({isTTY: true, columns: 80, rows: 24, write: text => chunks.push(text)},
      "Orbit", true, {TERM: "xterm-256color"}, PET_DISPLAY.preview);
    yield* Effect.sleep(PET_FRAME_MS + 10);
    yield* outcome;
  })));
  expect(chunks[0]).toBe("\r\n".repeat(PET_ROWS));
  expect(chunks[1]).toContain(`${ESC}18A`);
  expect(chunks[1]).not.toBe(chunks[2]);
  expect(chunks.at(-1)).toEndWith(`${ESC}17A\r`);
  const count = chunks.length;
  await Bun.sleep(PET_FRAME_MS + 10);
  expect(chunks).toHaveLength(count);
});

test.each([
  {dimension: "columns", beforeRedraw: false}, {dimension: "rows", beforeRedraw: false},
  {dimension: "columns", beforeRedraw: true}, {dimension: "rows", beforeRedraw: true},
] as const)("stops drawing and avoids cursor cleanup after a resize: %j", async ({dimension, beforeRedraw}) => {
  const chunks: string[] = [];
  const stream = {isTTY: true, columns: 80, rows: 24, write: (text: string) => chunks.push(text)};
  await Effect.runPromise(Effect.scoped(Effect.gen(function* () {
    yield* startProgress(stream, "Orbit", true, {TERM: "xterm-256color"}, PET_DISPLAY.preview);
    stream[dimension] = dimension === "columns" ? 40 : 10;
    if (!beforeRedraw) yield* Effect.sleep(PET_FRAME_MS * 2 + 10);
  })));
  expect(chunks).toHaveLength(3);
  expect(chunks[1]).toContain("Just floating.");
  expect(chunks[2]).toBe("\r\n");
});
