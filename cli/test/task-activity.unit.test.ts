import { expect, test } from "bun:test";
import { ActivityPrinter, activityText } from "../src/commands/task-activity.ts";
import { ToolActivityPrinter } from "../src/commands/tool-activity.ts";

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
