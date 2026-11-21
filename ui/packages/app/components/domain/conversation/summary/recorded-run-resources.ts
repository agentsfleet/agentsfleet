import type { FleetEvent, FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { AGENTSFLEET_EVENT_STATUS } from "@/lib/streaming/fleet-stream-row";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { TOOL_NAME } from "../../activity/tools/presentation/tool-call-shape";

type ResourceState = "Running" | "Last seen running" | "Done" | "Failed" | "Interrupted";
export type RunResource = {key: string; kind: "agent" | "process"; label: string; state: ResourceState; detail: string};
type RunResources = {rows: RunResource[]; previews: string[]};
const CHILD_STATES = new Map<string, ResourceState>([["done", "Done"], ["failed", "Failed"], ["interrupted", "Interrupted"]]);
const EMPTY: RunResources = {rows: [], previews: []};
const cache = new WeakMap<FleetEvent, RunResources>();

/** Child and terminal session identifiers belong to the run that created them. */
export function recordedRunResources(event: FleetEvent): RunResources {
  const known = cache.get(event);
  if (known) return known;
  if (!event.tools?.length) return EMPTY;
  const rows = new Map<string, RunResource>();
  const running = event.status === AGENTSFLEET_EVENT_STATUS.RECEIVED ? "Running" : "Last seen running";
  const previews = new Set<string>();
  for (const tool of event.tools) {
    const row = observeRunTool(tool, event.id, running, rows);
    if (row?.kind === "process" && row.label === "Preview server") {
      // A background server may first print its URL in a subsequent poll.
      for (const href of previewLinks([tool.outputHead, tool.outputTail].join("\n"), event.reply)) previews.add(href);
    }
  }
  const result = rows.size ? {rows: [...rows.values()], previews: [...previews]} : EMPTY;
  cache.set(event, result);
  return result;
}

export function observeRunTool(tool: FleetToolCall, eventId: string, running: "Running" | "Last seen running", rows: Map<string, RunResource>): RunResource | undefined {
  const {name, args} = tool;
  if (name === TOOL_NAME.WRITE_STDIN && (tool.status === TOOL_CALL_STATUS.SUCCEEDED || tool.status === TOOL_CALL_STATUS.FAILED)) {
    if (typeof args?.session_id !== "number") return;
    const row = rows.get(eventId + ":process:" + args.session_id);
    if (!row || tool.exitCode === undefined) return row;
    return record(rows, {...row, state: tool.exitCode === 0 ? "Done" : "Failed"});
  }
  if (tool.status !== TOOL_CALL_STATUS.SUCCEEDED) return;
  if (name === TOOL_NAME.SPAWN || name === "wait_agent" || name === "interrupt_agent") {
    const output = objectOutput(tool.outputHead) ?? clippedChildState(tool);
    if (!output) return;
    const id = name === TOOL_NAME.SPAWN ? output.child_id : args?.child_id;
    if (typeof id !== "number" || !Number.isSafeInteger(id) || id < 0) return;
    const key = eventId + ":agent:" + id;
    if (name === TOOL_NAME.SPAWN) {
      const task = typeof args?.task === "string" ? args.task.trim() : "";
      return record(rows, {key, kind: "agent", label: task || "Agent " + id, state: running, detail: task});
    } else {
      const row = rows.get(key);
      const state = typeof output.status === "string" && (output.status === "running" ? running : CHILD_STATES.get(output.status));
      if (row && state) {
        const detail = typeof output.answer === "string" ? output.answer : typeof output.detail === "string" ? output.detail : row.detail;
        return record(rows, {...row, state, detail});
      }
    }
  } else if (name === TOOL_NAME.EXEC_COMMAND) {
    return observeProcess(tool, eventId, running, rows);
  }
}

function record(rows: Map<string, RunResource>, row: RunResource): RunResource {
  rows.set(row.key, row);
  return row;
}

function objectOutput(text?: string): Record<string, unknown> | null {
  if (!text) return null;
  try {
    const value: unknown = JSON.parse(text);
    return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : null;
  } catch { return null; }
}

// The native State struct serializes its status first, followed by its answer or detail.
// A long JSON string is split into two wire edges. Recover only that complete status field.
function clippedChildState(tool: FleetToolCall): Record<string, unknown> | null {
  if (tool.name !== "wait_agent" || !tool.outputTail?.endsWith('"}')) return null;
  const head = tool.outputHead ?? "";
  if (head.startsWith('{"status":"done","answer":"')) return {status: "done", detail: "The recorded answer was shortened."};
  if (head.startsWith('{"status":"failed","detail":"')) return {status: "failed", detail: "The recorded error detail was shortened."};
  return null;
}

function observeProcess(tool: FleetToolCall, eventId: string, running: ResourceState, rows: Map<string, RunResource>) {
  const output = [tool.outputHead, tool.outputTail].filter(Boolean).join("\n").trim();
  const session = /(?:^|\n)Process running with session ID (\d+)$/.exec(output);
  if (session && typeof tool.args?.cmd === "string") {
    const command = tool.args.cmd;
    const key = eventId + ":process:" + session[1];
    const label = /^(?:bun|npm|pnpm|yarn)(?: run)? preview(?:\s|$)/.test(command) ? "Preview server" : command.split("\n")[0] || "Command";
    return record(rows, {key, kind: "process", label, state: running, detail: command + "\n" + output});
  }
}

function previewLinks(detail: string, reply: string): string[] {
  const links: string[] = [];
  const reported = new Set(detail.split(/\s+/));
  for (const match of reply.matchAll(/(?<=\[(?:launch|open|view) preview\]\()https?:\/\/[^\s()<>]{1,2048}(?=\))/gi)) {
    const href = match[0];
    if (!reported.has(href)) continue;
    const url = URL.parse(href);
    // A runner-local address would point at the reader’s own machine.
    if (url && !url.username && !url.password && !/(?:^|\.)localhost\.?$|^127\.|^0\.0\.0\.0$|^\[::1?\]$/i.test(url.hostname)) links.push(url.href);
  }
  return links;
}
