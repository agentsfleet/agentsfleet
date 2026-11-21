import * as v from "valibot";
import type { FleetEvent, FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { observeRunTool, type RunResource } from "../../conversation/summary/recorded-run-resources";
import { TOOL_NAME } from "../tools/presentation/tool-call-shape";
import { firstLine, pathArg, stringArg } from "../tools/presentation/tool-call-text";

export const ACTIVITY_TRACE_PART = "fleet-work-activity";
const TRACE_ROW = v.object({kind: v.picklist(["files", "images", "agent", "edits", "integration", "process", "check"]),
  label: v.string(), detail: v.string(), outcome: v.optional(v.picklist(["failed", "interrupted"]))});
export type ActivityTraceRow = v.InferOutput<typeof TRACE_ROW>;
export function activityTracePart(rows: ActivityTraceRow[]) {
  return {type: "data" as const, name: ACTIVITY_TRACE_PART, data: rows};
}
export function readActivityTrace(data: unknown): ActivityTraceRow[] {
  const result = v.safeParse(v.array(TRACE_ROW), data);
  return result.success ? result.output : [];
}

type BatchKind = "files" | "images" | "edits";
type Batch = {kind: BatchKind; paths: Set<string>; commands: string[]};
const READ: ReadonlySet<string> = new Set([TOOL_NAME.FILE_READ, TOOL_NAME.FILE_READ_HASHED]);
const EDIT: ReadonlySet<string> = new Set([TOOL_NAME.FILE_EDIT, TOOL_NAME.FILE_EDIT_HASHED, TOOL_NAME.FILE_WRITE, TOOL_NAME.FILE_APPEND]);

/** Only confirmed, related calls share a row. The unabridged calls, including failures, stay in Worked. */
export function recordedActivityTrace(event: Pick<FleetEvent, "tools" | "id" | "status">): ActivityTraceRow[] {
  const rows: ActivityTraceRow[] = [];
  const resources = new Map<string, RunResource>();
  const terminalAgents = new Set<string>();
  const state = event.status === "received" ? "Running" : "Last seen running";
  let batch: Batch | undefined;
  function flush() {
    if (batch) {
      const row = batchRow(batch);
      if (row) rows.push(row);
      batch = undefined;
    }
  }
  for (const tool of event.tools ?? []) {
    if (!tool.done || tool.status !== TOOL_CALL_STATUS.SUCCEEDED) {flush(); continue;}
    const kind = READ.has(tool.name) ? "files" : tool.name === TOOL_NAME.IMAGE ? "images" : EDIT.has(tool.name) ? "edits" : undefined;
    const path = kind ? pathArg(tool.args) : "";
    if (kind && path) {
      if (batch?.kind !== kind || batch.commands.length) {flush(); batch = {kind, paths: new Set(), commands: []};}
      batch.paths.add(path);
      continue;
    }
    const cmd = stringArg(tool.args, "cmd");
    if (batch?.kind === "edits" && tool.name === TOOL_NAME.EXEC_COMMAND && tool.exitCode === 0 && cmd) {
      batch.commands.push(firstLine(cmd));
      continue;
    }
    flush();
    const observed = observeRunTool(tool, event.id, state, resources);
    const row = resourceRow(tool, observed, terminalAgents) ?? githubRow(tool);
    if (row) rows.push(row);
  }
  flush();
  return rows;
}

function batchRow({kind, paths, commands}: Batch): ActivityTraceRow | undefined {
  // A lone file already has an action and path in its standard tool disclosure.
  if (paths.size <= 1 && !commands.length) return undefined;
  const files = paths.size === 1 ? "file" : "files";
  const detail = [...paths, ...commands].join("\n");
  if (kind === "files") return {kind, label: `Read ${paths.size} ${files}`, detail};
  if (kind === "images") return {kind, label: `Viewed ${paths.size} images`, detail};
  const action = files === "file" ? "Edited file" : "Edited files";
  const ran = commands.length ? `, ran ${commands.length} command${commands.length === 1 ? "" : "s"}` : "";
  return {kind, label: action + ran, detail};
}

function resourceRow(tool: FleetToolCall, row: RunResource | undefined, terminal: Set<string>): ActivityTraceRow | undefined {
  if (row?.kind === "agent") {
    if (tool.name === TOOL_NAME.SPAWN) return {kind: "agent", label: `${row.label} started`, detail: row.detail ? `Assigned task: ${row.detail}` : "The runner confirmed the agent started."};
    if (terminal.has(row.key) || row.state === "Running" || row.state === "Last seen running") return undefined;
    terminal.add(row.key);
    if (row.state === "Done") return {kind: "agent", label: `${row.label} finished`, detail: row.detail || "The agent reported completion."};
    const outcome = row.state === "Failed" ? "failed" : "interrupted";
    return {kind: "agent", label: `${row.label} ${outcome}`, detail: row.detail || row.state, outcome};
  }
  if (row?.kind === "process" && row.label === "Preview server" && tool.name === TOOL_NAME.EXEC_COMMAND)
    return {kind: "process", label: "Started preview server", detail: row.detail};
  return undefined;
}

function githubRow(tool: FleetToolCall): ActivityTraceRow | undefined {
  if (tool.name !== TOOL_NAME.HTTP_REQUEST) return undefined;
  const url = URL.parse(stringArg(tool.args, "url") ?? "");
  if (url?.origin !== "https://api.github.com" || url.username || url.password) return undefined;
  const method = stringArg(tool.args, "method") ?? "GET";
  const checks = method === "GET" && /^\/repos\/([^/]+\/[^/]+)\/commits\/([^/]+)\/check-runs$/.exec(url.pathname);
  const detail = checks ? `Requested check runs for ${checks[1]} at ${checks[2]}.` : `${method} ${url.origin}${url.pathname}`;
  return {kind: "integration", label: "Used GitHub integration", detail};
}
