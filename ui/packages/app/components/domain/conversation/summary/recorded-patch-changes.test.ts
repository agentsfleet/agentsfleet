import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { expect, it } from "vitest";
import { readSavedTrace } from "@/lib/streaming/fleet-stream-tool-trace";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";
import { recordedChanges, recordedChangeTotals } from "./recorded-changes";

const HEADER = "Success. Updated the following files:";
const UNKNOWN = {kind: "unknown"} as const;
function patch(over: Partial<FleetToolCall> = {}): FleetToolCall {
  return {name: "apply_patch", done: true, status: "succeeded", startedAtMs: 0, ms: 1, ...over};
}

it("projects native saved add/update/delete receipts and allocates the actual deleted-line remainder", () => {
  // Same patch and receipt as afr_tools/src/sandbox/apply_patch/tests.rs::EACH_KIND.
  const input = "*** Begin Patch\n*** Add File: src/new.txt\n+hello\n+world\n*** Update File: src/keep.txt\n@@\n two\n+two and a half\n*** Delete File: src/old.txt\n*** End Patch\n";
  const trace = readSavedTrace({calls: [{call_id: "1", name: "apply_patch", arguments: {patch: input}, status: "succeeded", duration_ms: 1,
    output_head: `${HEADER}\nA src/new.txt\nM src/keep.txt\nD src/old.txt\n+3 −1`, output_line_count: 5}]}, 0);
  expect(trace?.calls).toHaveLength(1);
  expect(recordedChanges([evt({tools: trace?.calls})])).toEqual([
    {path: "src/old.txt", running: false, lines: {kind: "complete", added: 0, removed: 1}},
    {path: "src/keep.txt", running: false, lines: {kind: "complete", added: 1, removed: 0}},
    {path: "src/new.txt", running: false, lines: {kind: "complete", added: 2, removed: 0}},
  ]);
});

it("records a native move under its destination, with counts from the actual receipt", () => {
  const input = "*** Begin Patch\n*** Update File: src/keep.txt\n*** Move to: src/kept.txt\n@@\n-two\n+TWO\n*** End Patch\n";
  expect(recordedChanges([evt({tools: [patch({args: {patch: input}, outputHead: `${HEADER}\nM src/kept.txt\n+1 −1`, outputLineCount: 3})]})]))
    .toEqual([{path: "src/kept.txt", running: false, lines: {kind: "complete", added: 1, removed: 1}}]);
});

it("uses a single-file runner total when the patch argument was clipped or omitted", () => {
  for (const args of [{patch: `*** Begin Patch\n*** Update File: a\n@@\n-${"x".repeat(500)}`.slice(0, 256)}, undefined]) {
    expect(recordedChanges([evt({tools: [patch({args, outputHead: `${HEADER}\nM a\n+248 −3`, outputLineCount: 3})]})]))
      .toEqual([{path: "a", running: false, lines: {kind: "complete", added: 248, removed: 3}}]);
  }
});

it("keeps both native paths but never splits an aggregate count across clipped multi-file arguments", () => {
  const input = `*** Begin Patch\n*** Update File: a\n@@\n-${"€".repeat(100)}\n+x\n*** Update File: b\n@@\n-y\n+z\n*** End Patch`;
  const clipped = new TextDecoder().decode(new TextEncoder().encode(input).slice(0, 255)).replace(/�$/, "");
  const changes = recordedChanges([evt({tools: [patch({args: {patch: clipped}, outputHead: `${HEADER}\nM a\nM b\n+9 −4`, outputLineCount: 4})]})]);
  expect(changes.map(change => [change.path, change.lines])).toEqual([["b", UNKNOWN], ["a", UNKNOWN]]);
  expect(recordedChangeTotals(changes)).toEqual(UNKNOWN);
});

it("merges overlapping native output edges without duplicating paths", () => {
  const paths = Array.from({length: 6}, (_, index) => `src/${index}.ts`);
  const rows = [HEADER, ...paths.map(path => `M ${path}`), "+6 −6"];
  const changes = recordedChanges([evt({tools: [patch({args: {patch: "x".repeat(256)},
    outputHead: rows.slice(0, 5).join("\n") + "\n", outputTail: rows.slice(-5).join("\n"), outputLineCount: rows.length})]})]);
  expect(changes.map(change => change.path)).toEqual(paths.toReversed());
  expect(changes.every(change => change.lines.kind === "unknown")).toBe(true);
});

it("retains only recoverable paths when native five-line edges omit the middle", () => {
  const paths = Array.from({length: 12}, (_, index) => `src/${index}.ts`);
  const rows = [HEADER, ...paths.map(path => `M ${path}`), "+12 −12"];
  const changes = recordedChanges([evt({tools: [patch({args: {patch: "x".repeat(256)},
    outputHead: rows.slice(0, 5).join("\n") + "\n", outputTail: rows.slice(-5).join("\n"), outputLineCount: rows.length})]})]);
  expect(changes.map(change => change.path)).toEqual([...paths.slice(0, 4), ...paths.slice(-4)].toReversed());
  expect(changes.every(change => change.lines.kind === "unknown")).toBe(true);
});

it("does not turn native byte-cut path fragments into files or give another file their totals", () => {
  const output = `${HEADER}\nM ${"folder/".repeat(180)}long.ts\nM short.ts\n+2 −2`;
  const changes = recordedChanges([evt({tools: [patch({args: {patch: "x".repeat(256)},
    outputHead: output.slice(0, 1024), outputTail: output.slice(-1024), outputLineCount: 4})]})]);
  expect(changes).toEqual([{path: "short.ts", running: false, lines: UNKNOWN}]);
});

it("keeps known edits separate from two irrecoverable deletion totals", () => {
  const input = "*** Begin Patch\n*** Add File: new\n+x\n*** Delete File: old\n*** Delete File: older\n*** End Patch";
  const changes = recordedChanges([evt({tools: [patch({args: {patch: input}, outputHead: `${HEADER}\nA new\nD old\nD older\n+1 −8`, outputLineCount: 5})]})]);
  expect(changes.map(change => [change.path, change.lines])).toEqual([["older", UNKNOWN], ["old", UNKNOWN], ["new", {kind: "complete", added: 1, removed: 0}]]);
  expect(recordedChangeTotals(changes)).toEqual({kind: "partial", added: 1, removed: 0});
});

it("recovers complete patch paths, moves and counts when optional output was not retained", () => {
  const input = "*** Begin Patch\n*** Add File: ./new\n+x\n*** Update File: /workspace/old\n*** Move to: moved\n@@\n-o\n+n\n*** Delete File: deleted\n*** End Patch";
  expect(recordedChanges([evt({tools: [patch({args: {patch: input}})]})])).toEqual([
    {path: "deleted", running: false, lines: UNKNOWN},
    {path: "moved", running: false, lines: {kind: "complete", added: 1, removed: 1}},
    {path: "new", running: false, lines: {kind: "complete", added: 1, removed: 0}},
  ]);
});

it("keeps running patches provisional and excludes refused or interrupted native patches", () => {
  const args = {patch: "*** Begin Patch\n*** Add File: a\n+x\n*** End Patch"};
  const running = evt({tools: [patch({args, done: false, status: undefined})]});
  expect(recordedChanges([running])).toEqual([{path: "a", running: true, lines: UNKNOWN}]);
  expect(recordedChanges([running], {includeRunning: false})).toEqual([]);
  for (const status of ["failed", "interrupted", undefined] as const) {
    expect(recordedChanges([evt({tools: [patch({args, status, outputHead: `${HEADER}\nA a\n+1 −0`})]})])).toEqual([]);
  }
});

it("counts repeated native operations on one path once in the file total", () => {
  const input = "*** Begin Patch\n*** Add File: a\n+x\n*** Add File: a\n+y\n*** Add File: b\n+z\n*** End Patch";
  const changes = recordedChanges([evt({tools: [patch({args: {patch: input}, outputHead: `${HEADER}\nA a\nA a\nA b\n+3 −0`, outputLineCount: 5})]})]);
  expect(changes.map(change => [change.path, change.lines])).toEqual([["b", {kind: "complete", added: 1, removed: 0}], ["a", {kind: "complete", added: 2, removed: 0}]]);
});

it("does not count ambiguous empty EOF replacement lines as certain changes", () => {
  const input = "*** Begin Patch\n*** Update File: a\n@@\n-\n+\n*** Update File: b\n@@\n-o\n+n\n*** End Patch";
  expect(recordedChanges([evt({tools: [patch({args: {patch: input}})]})]).map(change => change.lines))
    .toEqual([{kind: "complete", added: 1, removed: 1}, UNKNOWN]);
});

it("retains a complete receipt path without inventing line totals when its footer is unavailable", () => {
  expect(recordedChanges([evt({tools: [patch({outputHead: `${HEADER}\nM a\n`})]})]))
    .toEqual([{path: "a", running: false, lines: UNKNOWN}]);
});

it.each(["", "not a patch", "*** Begin Patch\n*** Add File: a\n+x", "*** Begin Patch\n\n*** End Patch"])(
  "never invents files from an incomplete or empty patch before the runner has accepted it: %j", input => {
    expect(recordedChanges([evt({tools: [patch({args: {patch: input}, done: false, status: undefined})]})])).toEqual([]);
  });

it.each(["+1 −2", "+2 −1", "+3 −2", "+2 −3"])("refuses per-file allocations that contradict the actual runner total %s", total => {
  const input = "*** Begin Patch\n*** Update File: a\n@@\n-x\n+y\n*** Update File: b\n@@\n-z\n+w\n*** End Patch";
  expect(recordedChanges([evt({tools: [patch({args: {patch: input}, outputHead: `${HEADER}\nM a\nM b\n${total}`})]})])
    .every(change => change.lines.kind === "unknown")).toBe(true);
});

it("handles the native parser's surrounding frame whitespace and repeated deleted/recreated paths", () => {
  const input = "*** Begin Patch\n *** Delete File: a\n *** Add File: a\n+x\n*** Add File: b\n+y\n*** End Patch";
  const changes = recordedChanges([evt({tools: [patch({args: {patch: input}})]})]);
  expect(changes.map(change => [change.path, change.lines])).toEqual([["b", {kind: "complete", added: 1, removed: 0}], ["a", UNKNOWN]]);
});

it("recovers a missing middle receipt path from a whole native patch while preserving unknown per-file deletions", () => {
  const paths = ["a", "b", "c", "d", "e", "f", "g", "h", "i"];
  const input = ["*** Begin Patch", ...paths.map(path => `*** Delete File: ${path}`), "*** End Patch"].join("\n");
  const rows = [HEADER, ...paths.map(path => `D ${path}`), "+0 −9"];
  const changes = recordedChanges([evt({tools: [patch({args: {patch: input},
    outputHead: rows.slice(0, 5).join("\n") + "\n", outputTail: rows.slice(-5).join("\n"), outputLineCount: rows.length})]})]);
  expect(changes.map(change => change.path).toSorted()).toEqual(paths);
  expect(changes.every(change => change.lines.kind === "unknown")).toBe(true);
});
