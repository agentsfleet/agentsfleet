import type { FleetEvent, FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import type { TaskPage } from "@/lib/api/tasks";
import { detail, summary, TASK_IDS } from "@/tests/helpers/task-reads";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";

export const VISUAL_SCENARIOS = ["empty", "running", "completed", "error"] as const;
export type VisualScenario = typeof VISUAL_SCENARIOS[number];

/** Explicit sample records, shared by the transcript and its Summary readers. */
export function visualEvents(scenario: VisualScenario, started: number, includeCode = false): FleetEvent[] {
  if (scenario === "empty") return [];
  const completed = scenario === "completed";
  const tools: FleetToolCall[] = [
    tool("read", "http_request", {url: "https://api.github.com/repos/agentsfleet/platform-images/actions/runs"}, started),
    tool("scan", "exec_command", {cmd: "trivy image --severity CRITICAL release:latest"}, started, scenario === "running", scenario === "error"),
    ...(completed ? [
      tool("file", "file_read", {path: "release.yaml"}, started),
      tool("edit", "file_edit", {path: "release.yaml", old_text: "ready: false", new_text: "ready: true"}, started),
      tool("report", "custom_release_report", {release: "current"}, started),
      tool("manifest", "file_write", {path: "runner-image.yaml"}, started),
      tool("notes", "file_write", {path: "release-notes.md"}, started),
    ] : []),
  ];
  return [{id: "answer", role: "assistant", actor: "fleet", text: "", tools,
    reply: completed ? "The runner update is ready. The image builds for both architectures, and both security scans passed.\n\n## Ready for your review\n\nI opened the pull request with the changes and scan results. Publishing is paused until you approve it." + (includeCode ? "\n\nReview `release.yaml` and [the build report](https://example.test/release).\n\n```yaml\nready: true\n```" : "")
      : "I’m checking the image builds and security scans before publishing.",
    outcome: "", failureLabel: scenario === "error" ? "transport_loss" : null,
    failureDetail: scenario === "error" ? "The scan response was interrupted. Earlier build results are retained." : null,
    createdAt: new Date(started - 60_000), status: scenario === "running" ? "received" : scenario === "error" ? "fleet_error" : "processed",
    wallMs: completed ? 138_000 : null, tokens: null, costNanos: null,
    custom: {requestJson: JSON.stringify({message: "Update the runner image using the release checklist. Run the scans, then ask me before publishing."})},
    ...(completed ? {taskActivity: {truncated: false, items: [{kind: "task_decision_completed" as const, progress: {
      activity_id: TASK_IDS[1], task_id: TASK_IDS[0], source_admission_id: null, operation: "goal" as const,
      phase: "recorded" as const, provider: null, model: null, completed: 1, total: 1}}]}} : {})}];
}

export function visualTaskPage(scenario: VisualScenario, started: number): TaskPage {
  return {items: scenario === "empty" ? [] : [{...summary(undefined, scenario === "running" ? "running" : scenario === "error" ? "blocked" : "waiting"),
    title: "Release readiness", created_at: new Date(started - 138_000).toISOString()}], next_cursor: null};
}

export function visualTaskDetail(scenario: VisualScenario, started: number) {
  const task = visualTaskPage(scenario, started).items[0] ?? summary();
  const saved = detail(task);
  return {...saved, context: {...saved.context, objective: "Update the runner image, run the scans, and ask before publishing.",
    target: scenario === "completed" ? {kind: "existing_pr" as const, repository: "agentsfleet/platform-images", number: 58,
      branch: "runner-refresh", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/agentsfleet/platform-images/pull/58", workflows: []} : saved.context.target}};
}

function tool(callId: string, name: string, args: Record<string, string>, started: number, running = false, failed = false): FleetToolCall {
  return {callId, name, args, startedAtMs: started - 138_000, ms: running ? null : 2000, done: !running,
    ...(running ? {} : {status: failed ? TOOL_CALL_STATUS.FAILED : TOOL_CALL_STATUS.SUCCEEDED, outputHead: "Recorded example result", outputLineCount: 1})};
}
