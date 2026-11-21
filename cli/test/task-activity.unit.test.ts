import { expect, test } from "bun:test";
import { ActivityPrinter, activityText } from "../src/commands/fleet/task-activity.ts";
import { ToolActivityPrinter } from "../src/commands/fleet/tool-activity.ts";
import { fileActivity } from "../src/commands/fleet/file-activity.ts";

test("a confirmed edit prints the files and counts the runner recorded", () => {
  const changes = [{path: "src/a.rs", added: 24, removed: 8}, {path: "README.md", added: 1, removed: 0}];
  expect(fileActivity("apply_patch", changes, "succeeded"))
    .toBe("Confirmed local edit · not a publication\nsrc/a.rs +24 −8\nREADME.md +1 −0");
  for (const tool of ["file_write", "file_append", "file_edit", "file_edit_hashed"]) {
    expect(fileActivity(tool, changes, "succeeded")).toStartWith("Confirmed local edit");
  }
});

test("an edit with no change the CLI can vouch for prints none", () => {
  const changes = [{path: "src/a.rs", added: 24, removed: 8}];
  // Not succeeded, not an edit, or nothing recorded.
  expect(fileActivity("file_edit", changes, "failed")).toBeUndefined();
  expect(fileActivity("shell", changes, "succeeded")).toBeUndefined();
  expect(fileActivity("file_edit", undefined, "succeeded")).toBeUndefined();
  expect(fileActivity("file_edit", [], "succeeded")).toBeUndefined();
  // One entry the CLI cannot read drops the list rather than printing part of it.
  for (const broken of [
    [{path: "", added: 1, removed: 0}],
    [{path: "a.rs", added: -1, removed: 0}],
    [{path: "a.rs", added: 1.5, removed: 0}],
    [{path: "a.rs", added: 1}],
    [...changes, "a.rs +1 -0"],
    {path: "a.rs", added: 1, removed: 0},
  ]) expect(fileActivity("file_edit", broken, "succeeded")).toBeUndefined();
});

test("an edit's arguments are never diffed into a preview", () => {
  const lines: string[] = [];
  const printer = new ToolActivityPrinter(line => lines.push(line));
  printer.frame("tool_call_completed", {name: "file_edit", status: "succeeded", call_id: "1",
    args_redacted: {path: "x", old_text: "before", new_text: "after"}});
  expect(lines[0]).not.toContain("- before");
  expect(lines[0]).toContain('"old_text": "before"');
  printer.history({calls: [{call_id: "9:2", name: "file_edit", arguments: {path: "x"}, status: "succeeded", duration_ms: 3,
    changes: [{path: "x", added: 2, removed: 1}]}]});
  expect(lines[1]).toContain("x +2 −1");
});

test("byte progress labels distinguish acknowledged, missing and reused ciphertext", () => {
  const lines: string[] = [];
  const printer = new ActivityPrinter(line => lines.push(line));
  const payload = {progress: {activity_id: "019f0000-0000-7000-8000-000000000001", operation: "capture", phase: "uploading",
    provider: null, model: null, completed: 2, total: 3, bytes: {acknowledged: 128, missing: 64, reused: 256}}};
  printer.frame("task_transfer_progress", payload);
  printer.frame("task_transfer_progress", payload);
  expect(lines).toEqual(["Saving encrypted files · capture · 2/3 objects · 128 bytes acknowledged · 64 known missing bytes · 256 bytes reused"]);
  printer.frame("task_transfer_progress", {...payload, progress: {...payload.progress, bytes: {...payload.progress.bytes, missing: -1}}});
  expect(lines).toHaveLength(1);
});

test("replayed decisions and recovery steps remain bounded public observations", () => {
  const lines: string[] = [];
  const printer = new ActivityPrinter(line => lines.push(line));
  const item = {kind: "task_decision_completed", progress: {activity_id: "019f0000-0000-7000-8000-000000000001",
    operation: "goal", phase: "unavailable", provider: "openai", model: "selected-model", completed: 0, total: null}};
  expect(printer.frame(item.kind, item)).toBe(true);
  printer.history({items: [item], truncated: true});
  printer.history({items: [item], truncated: true});
  expect(lines).toEqual(["Assessment unavailable · goal · openai / selected-model", "Earlier progress stages are outside the retained activity window."]);
  expect(printer.frame("unknown", {})).toBe(false);
  expect(printer.frame(item.kind, {progress: {...item.progress, completed: -1}})).toBe(true);
  expect(lines).toHaveLength(2);
});

test("command output preserves failures, exit codes and bounded terminal-safe previews", () => {
  const lines: string[] = [];
  const printer = new ToolActivityPrinter(line => lines.push(line));
  const call = {call_id: "9:1", name: "shell", arguments: {command: "make check"}, status: "failed",
    output_head: "first failure", output_tail: "last failure", exit_code: 2, duration_ms: 18};
  printer.history({calls: [call]});
  expect(lines[0]).toContain("shell failed 18ms");
  expect(lines[0]).toContain("make check");
  expect(lines[0]).toContain("last failure\nExit 2");
  printer.frame("tool_call_completed", {...call, args_redacted: call.arguments, ms: call.duration_ms});
  expect(lines).toHaveLength(1);
  expect(activityText("\u001b]8;;evil\u0007spoof\u202e")).not.toMatch(/[\u001b\u0007\u202e]/u);
  expect(activityText("x".repeat(9000))).toEndWith("… output truncated");
  expect(printer.frame("tool_call_completed", {name: "shell"})).toBe(true);
  expect(lines.at(-1)).toContain("outcome unavailable");
  expect(printer.frame("other", {})).toBe(false);
});


test("recorded paths and activity text strip complete terminal sequences", () => {
  const changes = [{path: "x\u001b[31m.rs", added: 1, removed: 0}];
  expect(fileActivity("file_edit", changes, "succeeded")).toContain("x.rs +1 −0");
  expect(activityText("\u001b[31mred\u001b[0m\u001b]8;;https://evil.example\u0007link\u001b]8;;\u0007")).toBe("redlink");
});

test("older server observations cannot repeat changed missing-byte estimates", () => {
  const lines: string[] = [];
  const printer = new ActivityPrinter(line => lines.push(line));
  const progress = {activity_id: "019f0000-0000-7000-8000-000000000001", sequence: 3,
    operation: "capture", phase: "uploading", provider: null, model: null, completed: 1, total: 2,
    bytes: {acknowledged: 128, missing: 128, reused: 0}};
  printer.frame("task_transfer_progress", {progress});
  printer.frame("task_transfer_progress", {progress: {...progress, sequence: 2, bytes: {...progress.bytes, missing: 256}}});
  expect(lines).toHaveLength(1);
});

test("a patch prints its recorded files, never its text", () => {
  expect(fileActivity("apply_patch", {patch: "*** Update File: x\n-old\n+new"}, "succeeded")).toBeUndefined();
  expect(fileActivity("apply_patch", [{path: "x", added: 1, removed: 1}], "succeeded"))
    .toBe("Confirmed local edit · not a publication\nx +1 −1");
});

test("long event streams bound replay memory while retaining recent stage deduplication", () => {
  const lines: string[] = [];
  const printer = new ActivityPrinter(line => lines.push(line));
  const stage = (index: number) => ({progress: {activity_id: `019f0000-0000-7000-8000-${index.toString(16).padStart(12, "0")}`,
    operation: "capture", phase: "saved", provider: null, model: null, completed: 1, total: 1}});
  for (let index = 0; index < 257; index++) printer.frame("task_transfer_progress", stage(index));
  printer.frame("task_transfer_progress", stage(256));
  expect(lines).toHaveLength(257);
  printer.frame("task_transfer_progress", stage(0));
  expect(lines).toHaveLength(258);
});

test("long tool streams retain bounded replay history without losing new command outcomes", () => {
  const lines: string[] = [];
  const printer = new ToolActivityPrinter(line => lines.push(line));
  const call = (index: number) => ({call_id: `call-${index}`, name: "shell", status: "failed", exit_code: 2});
  for (let index = 0; index < 601; index++) printer.frame("tool_call_completed", call(index));
  printer.frame("tool_call_completed", call(600));
  expect(lines).toHaveLength(601);
  printer.frame("tool_call_completed", call(0));
  expect(lines).toHaveLength(602);
  expect(lines.at(-1)).toContain("Exit 2");
});
