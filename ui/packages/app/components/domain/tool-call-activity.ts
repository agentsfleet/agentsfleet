import type { MessageState } from "@assistant-ui/react";
import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import type { CellState } from "./tool-call-copy";
import { readToolResult } from "./fleetReplyMessage";
import { CELL_STATE, cellState, toolCopy } from "./tool-call-copy";
import { ARGS_NOT_RECORDED, TOOL_NAME } from "./tool-call-shape";
import { pathArg, stringArg } from "./tool-call-text";

type Kind = "read" | "write" | "list" | "command" | "request" | "pull-request" | "check" | "tool";
type Description = {running: string; done: string; target: string; kind: Kind};
export type ToolActivity = {label: string; target: string; kind: Kind; state: CellState};
const KNOWN: ReadonlySet<string> = new Set(Object.values(TOOL_NAME));
const COMPLETED: Record<Kind, string> = {read: "Read sources", write: "Edited files", list: "Listed items", command: "Ran commands",
  request: "Made requests", "pull-request": "Opened pull requests", check: "Completed checks", tool: "Used tools"};
const PREFIX = {[CELL_STATE.FAILED]: "Failed", [CELL_STATE.INTERRUPTED]: "Interrupted", [CELL_STATE.SETTLED]: "Outcome unavailable"};
const PRIORITY: Record<CellState, number> = {running: 4, failed: 3, interrupted: 2, settled: 1, succeeded: 0};

/** Activity reads bounded input fields; expanded details own diff and output parsing. */
export function toolActivity(name: string, args: ToolArgs | undefined, state: CellState): ToolActivity {
  const description = describe(name, args);
  const label = state === CELL_STATE.SUCCEEDED ? description.done : state === CELL_STATE.RUNNING ? description.running
    : `${PREFIX[state]}: ${description.running.toLowerCase()}`;
  return {label, target: description.target, kind: description.kind, state};
}

/** Only this group's indices contribute; an adjacent text part cannot change its outcome. */
export function toolGroupActivity(content: MessageState["content"], indices: readonly number[], running: boolean): ToolActivity {
  const calls = indices.flatMap(index => {const part = content[index]; return part?.type === "tool-call" ? [part] : [];});
  const summaries = new Map<string, number>();
  let state: CellState = CELL_STATE.SUCCEEDED;
  let kind: Kind = "tool";
  let active: ToolActivity | undefined;
  let activeCount = 0;
  for (const call of calls) {
    const result = readToolResult(call.result);
    const current = cellState(result !== undefined, result?.status, running && call.result === undefined);
    const activity = toolActivity(call.toolName, call.args, current);
    if (current === CELL_STATE.RUNNING) {active ??= activity; activeCount++; continue;}
    kind = activity.kind;
    if (PRIORITY[current] > PRIORITY[state]) state = current;
    const summary = current === CELL_STATE.SUCCEEDED ? COMPLETED[kind] : PREFIX[current];
    summaries.set(summary, (summaries.get(summary) ?? 0) + 1);
  }
  if (active) return activeCount === 1 ? active : {...active, target: [active.target, `${activeCount} active`].filter(Boolean).join(" · ")};
  return {label: [...summaries].map(([label, count]) => count === 1 ? label : `${label} (${count})`).join(", ") || "Tool activity",
    target: "", state: calls.length === 0 ? CELL_STATE.SETTLED : state, kind};
}

function describe(name: string, args: ToolArgs | undefined): Description {
  const target = args === undefined ? ARGS_NOT_RECORDED : pathArg(args);
  switch (name) {
    case TOOL_NAME.FILE_READ: case TOOL_NAME.FILE_READ_HASHED:
      return {running: "Reading file", done: "Read file", target, kind: "read"};
    case TOOL_NAME.FILE_WRITE: case TOOL_NAME.FILE_APPEND: case TOOL_NAME.FILE_EDIT: case TOOL_NAME.FILE_EDIT_HASHED: case TOOL_NAME.FILE_DELETE:
      return {...toolCopy(name, undefined).verbs, target, kind: "write"};
    case TOOL_NAME.APPLY_PATCH: return {running: "Editing files", done: "Edited files", target: "", kind: "write"};
    case TOOL_NAME.EXEC_COMMAND: case TOOL_NAME.SHELL:
      return {running: "Running command", done: "Ran command", target: first(stringArg(args, name === TOOL_NAME.SHELL ? "command" : "cmd")), kind: "command"};
    case TOOL_NAME.HTTP_REQUEST: return request(args);
    case TOOL_NAME.MEMORY_LIST: case TOOL_NAME.CRON_LIST: case TOOL_NAME.CRON_RUNS:
      return {running: "Listing", done: "Listed", target: name === TOOL_NAME.MEMORY_LIST ? "memories" : name === TOOL_NAME.CRON_RUNS ? "schedule runs" : "schedules", kind: "list"};
    case TOOL_NAME.MEMORY_RECALL: return {running: "Recalling memory", done: "Recalled memory", target: first(stringArg(args, "query")), kind: "read"};
    case TOOL_NAME.MEMORY_STORE: return {running: "Saving memory", done: "Saved memory", target: first(stringArg(args, "key")), kind: "tool"};
    case TOOL_NAME.GIT: return git(args);
    default: {
      if (!KNOWN.has(name)) return {running: "Using", done: "Used", target: name, kind: "tool"};
      return {...toolCopy(name, undefined).verbs, target: name, kind: "tool"};
    }
  }
}

function request(args: ToolArgs | undefined): Description {
  const method = stringArg(args, "method") ?? "GET";
  const raw = stringArg(args, "url");
  let url: URL;
  try {url = new URL(raw ?? "");} catch {return {running: "Making request", done: "Made request", target: raw === undefined ? ARGS_NOT_RECORDED : "", kind: "request"};}
  const target = `${method} ${url.origin}${url.pathname}`;
  if (method === "GET" && ["/healthz", "/readyz"].includes(url.pathname))
    return {running: "Checking environment", done: "Checked environment", target: url.origin, kind: "check"};
  if (method === "GET" && /^\/v1\/workspaces\/[^/]+\/?$/.test(url.pathname))
    return {running: "Checking workspace access", done: "Checked workspace access", target: url.pathname.slice("/v1/workspaces/".length).replace(/\/$/, ""), kind: "check"};
  if (url.origin === "https://api.github.com" && /^\/repos\/[^/]+\/[^/]+(?:\/|$)/.test(url.pathname)) {
    const remaining = url.pathname.slice("/repos/".length);
    const boundary = remaining.indexOf("/", remaining.indexOf("/") + 1);
    const repo = boundary < 0 ? remaining : remaining.slice(0, boundary);
    const path = boundary < 0 ? "" : remaining.slice(boundary);
    if (method === "POST" && path === "/pulls") return {running: "Opening pull request", done: "Opened pull request", target: repo, kind: "pull-request"};
    if (method === "GET" && (path.startsWith("/git/trees/") || path === "/contents")) return {running: "Listing files", done: "Listed files", target: repo, kind: "list"};
    if (method === "GET" && path.startsWith("/contents/")) return {running: "Reading file", done: "Read file", target: first(path.slice(10)), kind: "read"};
    if (method === "GET" && /\/actions\/jobs\/\d+\/logs$/.test(path)) return {running: "Reading job log", done: "Read job log", target: repo, kind: "read"};
    if (method === "GET" && path === "") return {running: "Checking repository access", done: "Checked repository access", target: repo, kind: "check"};
  }
  return {running: "Making request", done: "Made request", target: first(target), kind: "request"};
}

function git(args: ToolArgs | undefined): Description {
  const words = args?.args;
  if (Array.isArray(words) && words.length === 2 && words[0] === "rev-parse" && words[1] === "--show-toplevel")
    return {running: "Scoping workspace", done: "Scoped workspace", target: "", kind: "check"};
  if (Array.isArray(words) && (words[0] === "status" || words[0] === "diff"))
    return {running: "Checking changes", done: "Checked changes", target: "", kind: "check"};
  return {running: "Running Git command", done: "Ran Git command", target: "", kind: "command"};
}

function first(text: string | undefined): string {
  const bounded = (text ?? "").slice(0, 256);
  const newline = bounded.search(/[\r\n]/);
  return newline < 0 ? bounded : bounded.slice(0, newline);
}
