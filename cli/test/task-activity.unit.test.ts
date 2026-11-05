import { expect, test } from "bun:test";
import { ActivityPrinter, activityText } from "../src/commands/task-activity.ts";
import { ToolActivityPrinter } from "../src/commands/tool-activity.ts";
import { fileActivity } from "../src/commands/file-activity.ts";

test("local edit previews distinguish requested changes from confirmed edits", () => {
  const args = {path: "src/app.ts", old_text: "before", new_text: "after"};
  expect(fileActivity("file_edit", args, "succeeded")).toBe("Confirmed local edit: src/app.ts · not a publication\n- before\n+ after");
  expect(fileActivity("file_edit", args, "failed")).toStartWith("Requested local edit:");
  expect(fileActivity("file_edit", {...args, old_text: null}, "succeeded")).toBeUndefined();
  expect(fileActivity("shell", args, "succeeded")).toBeUndefined();
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
  printer.history({calls: [call], omitted_call_count: 3});
  expect(lines[0]).toContain("shell failed 18ms");
  expect(lines[0]).toContain("make check");
  expect(lines[0]).toContain("last failure\nExit 2");
  expect(lines[1]).toContain("3 earlier tool calls");
  printer.frame("tool_call_completed", {...call, args_redacted: call.arguments, ms: call.duration_ms});
  expect(lines).toHaveLength(2);
  expect(activityText("\u001b]8;;evil\u0007spoof\u202e")).not.toMatch(/[\u001b\u0007\u202e]/u);
  expect(activityText("x".repeat(9000))).toEndWith("… output truncated");
  expect(printer.frame("tool_call_completed", {name: "shell"})).toBe(true);
  expect(lines.at(-1)).toContain("outcome unavailable");
  expect(printer.frame("other", {})).toBe(false);
});


test("file previews retain unchanged context and strip complete terminal sequences", () => {
  const preview = fileActivity("file_edit", {path: "x", old_text: "same\nbefore\nend", new_text: "same\nafter\nend"}, "succeeded");
  expect(preview).toContain("  same\n- before\n+ after\n  end");
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
