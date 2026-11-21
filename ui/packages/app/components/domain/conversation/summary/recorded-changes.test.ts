import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import type { RecordedChange, RecordedChangeRepository } from "./recorded-changes";
import { expect, it } from "vitest";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";
import { groupRecordedChanges, recordedChanges, recordedChangeTotals } from "./recorded-changes";

function write(over: Partial<FleetToolCall> = {}): FleetToolCall {
  return {name: "file_edit", args: {path: "web/page.ts", old_text: "shared\nold", new_text: "shared\nnew\nextra"},
    done: true, status: "succeeded", startedAtMs: 0, ms: 1, ...over};
}
const WEB = {id: "web-source", label: "web", fullName: "company/web"};
const UNKNOWN = {kind: "unknown"} as const;

it("keeps an all-unknown receipt unknown and distinguishes an empty receipt", () => {
  expect(recordedChangeTotals([])).toEqual({kind: "complete", added: 0, removed: 0});
  expect(recordedChangeTotals([{path: "a", running: false, lines: UNKNOWN}])).toEqual(UNKNOWN);
});

it("counts complete recorded edit operations once per file, including repeated edits", () => {
  const changes = recordedChanges([evt({tools: [write(), write({args: {path: "web/page.ts", old_text: "new", new_text: "again"}})]})]);
  expect(changes).toEqual([{path: "web/page.ts", running: false, lines: {kind: "complete", added: 3, removed: 2}}]);
  const repeated = recordedChanges([evt({tools: [write()]}), evt({id: "later", tools: [write()]})]);
  expect(repeated[0]?.lines).toEqual({kind: "complete", added: 4, removed: 2});
});

const incompleteWrites: Partial<FleetToolCall>[] = [
  {name: "file_write", args: {path: "a", content: "not a before/after diff"}},
  {name: "file_append", args: {path: "a", content: "the previous last line is unknown"}},
  {name: "file_delete", args: {path: "a"}},
  {name: "file_edit_hashed", args: {path: "a", target: "L1:abcd", new_text: "replacement"}},
  {args: {path: "a", new_text: "missing old text"}},
  {args: {path: "a", old_text: "missing new text"}},
  {args: {path: "a", old_text: "x".repeat(257), new_text: "short"}},
  {args: {path: "a", old_text: "short", new_text: "x".repeat(257)}},
  {args: {path: "a", old_text: "€".repeat(85), new_text: "short"}},
  {args: {path: "a", old_text: "short", new_text: "€".repeat(85)}},
];
it.each(incompleteWrites)("does not invent totals for incomplete or clipped write data: %j", over => {
  expect(recordedChanges([evt({tools: [write(over)]})])[0]?.lines).toEqual(UNKNOWN);
});

it("keeps partial counts visible rather than treating unknown operations as zero", () => {
  const unknown = write({name: "file_write", args: {path: "web/page.ts"}});
  const running = write({done: false, status: undefined});
  const changes = recordedChanges([evt({tools: [unknown, write(), running]})]);
  expect(changes[0]).toMatchObject({running: true, lines: {kind: "partial", added: 2, removed: 1}});
  expect(recordedChangeTotals([...changes, {path: "other", running: false, lines: {kind: "complete", added: 1, removed: 0}}]))
    .toEqual({kind: "partial", added: 3, removed: 1});
  expect(recordedChanges([evt({tools: [running]})], {includeRunning: false})).toEqual([]);
});

it("never reads large unrelated content or edit payloads covered by a complete index receipt", () => {
  const args = Object.defineProperties({path: "web/page.ts"}, {
    content: {get() {throw new Error("A whole file body was read");}},
    old_text: {get() {throw new Error("An unnecessary diff was computed");}},
    new_text: {get() {throw new Error("An unnecessary diff was computed");}},
  });
  const repositories: RecordedChangeRepository[] = [{eventId: "e0", path: "web/page.ts", repository: WEB, lines: {added: 248, removed: 3}}];
  expect(recordedChanges([evt({tools: [write({args})]})], {repositories})[0]?.lines).toEqual({kind: "complete", added: 248, removed: 3});
  expect(recordedChanges([evt({tools: [write({name: "file_write", args})]})])[0]?.lines).toEqual(UNKNOWN);
});

it("uses explicit event/path repository bindings without guessing from directory names", () => {
  const repositories: RecordedChangeRepository[] = [{eventId: "e0", path: "web/page.ts", repository: WEB}];
  const changes = recordedChanges([evt({tools: [write()]}), evt({id: "other-event", tools: [write({args: {path: "docs/page.md"}})]})], {repositories});
  expect(changes.find(change => change.path === "web/page.ts")?.repository).toEqual(WEB);
  expect(changes.find(change => change.path === "docs/page.md")?.repository).toBeUndefined();
  expect(groupRecordedChanges(changes).map(group => group.label)).toEqual(["web", "Recorded files"]);
});

it("does not assign confirmed line receipts to a write that is still running", () => {
  const repositories: RecordedChangeRepository[] = [{eventId: "e0", path: "web/page.ts", repository: WEB, lines: {added: 8, removed: 2}}];
  const changes = recordedChanges([evt({tools: [write({done: false, status: undefined})]})], {repositories});
  expect(changes[0]).toMatchObject({repository: WEB, running: true, lines: UNKNOWN});
});

it("keeps same-named repositories and same-named files distinct using full source names", () => {
  const changes: RecordedChange[] = [
    {path: "README.md", running: false, repository: WEB, lines: {kind: "complete", added: 1, removed: 0}},
    {path: "README.md", running: false, repository: {...WEB, id: "another-web", fullName: "another-company/web"}, lines: UNKNOWN},
  ];
  expect(groupRecordedChanges(changes).map(group => group.label)).toEqual(["another-company/web", "company/web"]);
  expect(recordedChangeTotals(changes)).toEqual({kind: "partial", added: 1, removed: 0});
});

it("normalizes immutable historical writes only once as events stream, without caching repository context", () => {
  let reads = 0;
  const args = Object.defineProperties({}, {
    path: {get() {reads += 1; return "web/page.ts";}},
    old_text: {get() {reads += 1; return "old";}},
    new_text: {get() {reads += 1; return "new";}},
  });
  const past = evt({tools: [write({args})]});
  expect(recordedChanges([past])[0]?.lines).toEqual({kind: "complete", added: 1, removed: 1});
  expect(reads).toBe(3);
  const next = evt({id: "next", tools: [write({args: {path: "new.ts", old_text: "a", new_text: "b"}})]});
  const repositories: RecordedChangeRepository[] = [{eventId: "e0", path: "web/page.ts", repository: WEB, lines: {added: 8, removed: 2}}];
  expect(recordedChanges([past, next], {repositories}).find(change => change.path === "web/page.ts"))
    .toMatchObject({repository: WEB, lines: {kind: "complete", added: 8, removed: 2}});
  expect(recordedChanges([past, next]).find(change => change.path === "web/page.ts"))
    .toEqual({path: "web/page.ts", running: false, lines: {kind: "complete", added: 1, removed: 1}});
  expect(reads).toBe(3);
});

it("never rereads a historical native patch receipt or argument when another event arrives", () => {
  let reads = 0;
  const native = Object.defineProperties(write({name: "apply_patch"}), {
    args: {get() {reads += 1; return {patch: "*** Begin Patch\n*** Add File: a\n+x\n*** Add File: b\n+y\n*** End Patch"};}},
    outputHead: {get() {reads += 1; return "Success. Updated the following files:\nA a\nA b\n+2 −0";}},
  });
  const past = evt({tools: [native]});
  expect(recordedChanges([past]).map(change => change.path)).toEqual(["b", "a"]);
  expect(reads).toBe(2);
  expect(recordedChanges([past, evt({id: "next"})]).map(change => change.path)).toEqual(["b", "a"]);
  expect(reads).toBe(2);
});
