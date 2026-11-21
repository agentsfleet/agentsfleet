import { expect, it } from "vitest";
import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { launchReviewEvent } from "@/tests/e2e/fixtures/launch-review-activity";
import { ACTIVITY_TRACE_PART, recordedActivityTrace, readActivityTrace } from "./fleet-activity-trace";
import { replyParts } from "./fleetReplyMessage";

const event = launchReviewEvent(200_000);
const tool = (name: string, path: string, overrides?: Partial<FleetToolCall>): FleetToolCall => ({name, args: {path},
  startedAtMs: 0, done: true, ms: 200, status: "succeeded", ...overrides});
const labels = (tools: FleetToolCall[]) => recordedActivityTrace({...event, tools}).map(row => row.label);

it("uses only supported native work records in a saved conversation; preserves every tool call and the whole reply", () => {
  const parts = replyParts(event);
  const trace = parts.find(part => part.type === "data");
  expect(trace?.name).toBe(ACTIVITY_TRACE_PART);
  const rows = readActivityTrace(trace?.data);
  expect(rows.map(row => row.label)).toEqual(["Read 3 files", "Viewed 2 images", "UI reviewer started", "Edited files, ran 3 commands",
    "UI reviewer finished", "Used GitHub integration", "Started preview server"]);
  expect(rows[0]?.detail).toBe("LaunchPage.tsx\ntheme.css\nlaunch-brief.md");
  expect(rows[4]?.detail).toContain("Two spacing issues were corrected");
  expect(rows[6]?.detail).toMatch(/Process running with session ID 12$/);
  expect(parts.filter(part => part.type === "tool-call")).toHaveLength(event.tools!.length);
  expect(parts.at(-1)).toEqual({type: "text", text: event.reply});
});

it("counts distinct consecutive read paths and never joins reads across failed or unknown records", () => {
  const read = tool("file_read", "/workspace/theme.css");
  expect(labels([read, read])).toEqual([]);
  expect(labels([read, tool("file_read_hashed", "launch.md"), tool("file_read", "", {status: "failed"}), read])).toEqual(["Read 2 files"]);
  expect(labels([read, tool("image", "logo.png", {done: false}), tool("file_read", "launch.md")])).toEqual([]);
  expect(labels([tool("file_edit", "theme.css"), tool("exec_command", "", {args: {cmd: "build"}, exitCode: 1})])).toEqual([]);
});

it("requires matching child IDs, a reported terminal status, and an actual preview session", () => {
  const spawn = tool("spawn", "", {args: {task: "Reviewer"}, outputHead: '{"child_id":3}'});
  const wait = tool("wait_agent", "", {args: {child_id: 7}, outputHead: '{"status":"done"}'});
  expect(labels([wait, spawn, wait, {...wait, args: {child_id: 3}, outputHead: '{"status":"running"}'}])).toEqual(["Reviewer started"]);
  const observed = {...wait, args: {child_id: 3}, outputHead: '{"status":"failed","detail":"Worker stopped"}'};
  expect(recordedActivityTrace({...event, tools: [spawn, observed, observed]}).at(-1)).toMatchObject({label: "Reviewer failed", detail: "Worker stopped", outcome: "failed"});
  expect(labels([spawn, observed, observed])).toEqual(["Reviewer started", "Reviewer failed"]);
  expect(labels([{...spawn, outputHead: "truncated{"}, wait])).toEqual([]);
  expect(labels([tool("exec_command", "", {args: {cmd: "bun run preview"}, outputHead: "Starting server"})])).toEqual([]);
  expect(labels([tool("exec_command", "", {args: {cmd: "cat test-report"}, outputHead: "Process running with session ID 3"})])).toEqual([]);
});

it("omits failed requests, foreign origins and credentials, while preserving a successful GitHub route without query parameters", () => {
  const request = tool("http_request", "", {args: {url: "https://api.github.com/repos/example/site?secret=sample#here"}});
  expect(recordedActivityTrace({...event, tools: [request]})[0]?.detail).toBe("GET https://api.github.com/repos/example/site");
  expect(labels([{...request, status: "interrupted"}, {...request, args: {url: "https://api.github.com.example.test/repos/ex/site"}},
    {...request, args: {url: "https://u:p@api.github.com/repos/ex/site"}}])).toEqual([]);
  expect(readActivityTrace([{kind: "iframe", label: "open", detail: "hidden"}])).toEqual([]);
});

it("collects multiple edits and splits a new edit after the previous command", () => {
  const edit = tool("file_edit", "theme.css");
  const command = tool("exec_command", "", {args: {cmd: "check"}, exitCode: 0});
  expect(labels([edit, command, tool("file_write", "file.md"), edit])).toEqual(["Edited file, ran 1 command", "Edited files"]);
  expect(recordedActivityTrace({id: event.id, status: "received"})).toEqual([]);
  expect(labels([tool("http_request", "")])).toEqual([]);
});

it("explains unnamed children starting, completing and being interrupted without a reported detail", () => {
  const spawn = tool("spawn", "", {args: {}, outputHead: '{"child_id":7}'});
  for (const [state, label, detail] of [
    ["done", "Agent 7 finished", "The agent reported completion."],
    ["interrupted", "Agent 7 interrupted", "Interrupted"],
  ]) {
    const wait = tool("wait_agent", "", {args: {child_id: 7}, outputHead: JSON.stringify({status: state})});
    const rows = recordedActivityTrace({...event, status: "received", tools: [spawn, wait]});
    expect(rows.map(row => [row.label, row.detail])).toEqual([
      ["Agent 7 started", "The runner confirmed the agent started."], [label, detail],
    ]);
  }
});
