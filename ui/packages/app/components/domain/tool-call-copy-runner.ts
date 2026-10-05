import type { JsonValue, ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { DIFF_ROW, type DiffRow, type LineDiff } from "./tool-call-diff";
import { PLAN_STATUS, TOOL_BODY, TOOL_NAME, plainCopy, type PlanStatus, type PlanStep, type ToolCopy, type Verbs } from "./tool-call-shape";
import { CLIP_MARK, clipMarked, compactArgs, firstLine, firstString, linesOf, mayBeClipped, pathArg, scalarArg, stringArg, workspacePath } from "./tool-call-text";

// The runner's tools beyond files, requests, memory and commands, as their
// cells read. Argument names come from the runner: `afr_tools` for the tools
// it hosts (pushover, update_plan), and the specs that will host the rest
// (the sandbox tools, nested loops and runner verbs). Where a
// spec names only a verb's body, the cell tries each spelling it knows and
// otherwise shows the arguments as they came, never a guess.

const VERBS = {
  NOTIFY: { running: "Notifying", done: "Notified" },
  PLAN: { running: "Updating plan", done: "Updated plan" },
  EDIT: { running: "Editing", done: "Edited" },
  VIEW_IMAGE: { running: "Viewing image", done: "Viewed image" },
  OPEN: { running: "Opening", done: "Opened" },
  CLICK: { running: "Clicking", done: "Clicked" },
  TYPE: { running: "Typing into", done: "Typed into" },
  READ_PAGE: { running: "Reading", done: "Read" },
  WAIT_FOR: { running: "Waiting for", done: "Waited for" },
  BROWSE: { running: "Browsing", done: "Browsed" },
  SCREENSHOT: { running: "Taking screenshot", done: "Took screenshot" },
  RUN: { running: "Running", done: "Ran" },
  WRITE_TO: { running: "Writing to", done: "Wrote to" },
  DELEGATE: { running: "Delegating", done: "Delegated" },
  SPAWN: { running: "Starting agent", done: "Started agent" },
  SCHEDULE: { running: "Scheduling", done: "Scheduled" },
  ADD_SCHEDULE: { running: "Adding schedule", done: "Added schedule" },
  UPDATE_SCHEDULE: { running: "Updating schedule", done: "Updated schedule" },
  REMOVE_SCHEDULE: { running: "Removing schedule", done: "Removed schedule" },
  RUN_SCHEDULE: { running: "Running schedule", done: "Ran schedule" },
  MESSAGE: { running: "Sending message", done: "Sent message" },
} as const satisfies Record<string, Verbs>;

const ARG = {
  MESSAGE: "message",
  TITLE: "title",
  PLAN: "plan",
  EXPLANATION: "explanation",
  STEP: "step",
  STATUS: "status",
  PATCH: "patch",
  URL: "url",
  ACTION: "action",
  SELECTOR: "selector",
  ARGS: "args",
  SESSION: "session_id",
  CHARS: "chars",
  TASK: "task",
  AT: "at",
} as const;

// The spellings a schedule's id, expression and message go by: the runner
// verbs' bodies, and the names the earlier runner's tools used.
const SCHEDULE_ID = ["schedule_id", "id", "job_id"] as const;
const CRON_EXPRESSION = ["cron", "expression"] as const;
const SCHEDULE_MESSAGE = ["message", "command", "prompt"] as const;
const MESSAGE_TEXT = ["text", "content", "message"] as const;

const BROWSER_ACTION = { CLICK: "click", TYPE: "type", TEXT: "text", WAIT: "wait" } as const;
const PAGE = "page";
const TERMINAL = "terminal";
const GIT = "git";
const SEPARATOR = " ";
const PATCH_FILE = /^\*\*\* (?:(?:Add|Update|Delete) File|Move to): /;
const PATCH_FRAME = /^\*\*\* |^@@/;
const PLAN_STATUSES: ReadonlySet<string> = new Set(Object.values(PLAN_STATUS));

// A Map, not an object literal: a tool named `toString` must read as unknown.
export const RUNNER_COPY: ReadonlyMap<string, (args: ToolArgs) => ToolCopy> = new Map([
  [TOOL_NAME.PUSHOVER, (args) => plainCopy(VERBS.NOTIFY, firstLine(firstString(args, [ARG.TITLE, ARG.MESSAGE]) ?? ""))],
  [TOOL_NAME.UPDATE_PLAN, planCopy],
  [TOOL_NAME.APPLY_PATCH, patchCopy],
  [TOOL_NAME.IMAGE, (args) => plainCopy(VERBS.VIEW_IMAGE, pathArg(args))],
  [TOOL_NAME.BROWSER_OPEN, (args) => plainCopy(VERBS.OPEN, clipMarked(stringArg(args, ARG.URL) ?? ""))],
  [TOOL_NAME.BROWSER, browserCopy],
  [TOOL_NAME.SCREENSHOT, () => plainCopy(VERBS.SCREENSHOT, "")],
  [TOOL_NAME.GIT, gitCopy],
  [TOOL_NAME.WRITE_STDIN, stdinCopy],
  [TOOL_NAME.DELEGATE, (args) => plainCopy(VERBS.DELEGATE, firstLine(stringArg(args, ARG.TASK) ?? ""))],
  [TOOL_NAME.SPAWN, (args) => plainCopy(VERBS.SPAWN, firstLine(stringArg(args, ARG.TASK) ?? ""))],
  [TOOL_NAME.SCHEDULE, scheduleCopy],
  [TOOL_NAME.CRON_ADD, (args) => named(VERBS.ADD_SCHEDULE, args, [firstString(args, CRON_EXPRESSION), messageOf(args)])],
  [TOOL_NAME.CRON_UPDATE, (args) => named(VERBS.UPDATE_SCHEDULE, args, [scheduleId(args)])],
  [TOOL_NAME.CRON_REMOVE, (args) => named(VERBS.REMOVE_SCHEDULE, args, [scheduleId(args)])],
  [TOOL_NAME.CRON_RUN, (args) => named(VERBS.RUN_SCHEDULE, args, [scheduleId(args)])],
  [TOOL_NAME.MESSAGE, (args) => named(VERBS.MESSAGE, args, [textOf(args)])],
]);

/** Tools a call makes with no arguments at all: no arguments is no loss. */
export const TAKES_NO_ARGS: ReadonlySet<string> = new Set([TOOL_NAME.SCREENSHOT]);

/** A patch's files and the lines it removes and adds, read from Codex's
 * patch grammar (`*** Update File:`, `@@`, then `-`, `+` and ` ` lines). */
export function patchDiff(patch: string): { files: string[]; diff: LineDiff } {
  const files: string[] = [];
  const rows: DiffRow[] = [];
  for (const line of linesOf(patch)) {
    if (PATCH_FILE.test(line)) files.push(workspacePath(line.replace(PATCH_FILE, "")));
    else if (!PATCH_FRAME.test(line)) rows.push(patchRow(line));
  }
  return {
    files,
    diff: {
      rows,
      added: rows.filter((row) => row.kind === DIFF_ROW.ADDED).length,
      removed: rows.filter((row) => row.kind === DIFF_ROW.REMOVED).length,
    },
  };
}

function patchRow(line: string): DiffRow {
  if (line.startsWith("+")) return { kind: DIFF_ROW.ADDED, text: line.slice(1) };
  if (line.startsWith("-")) return { kind: DIFF_ROW.REMOVED, text: line.slice(1) };
  return { kind: DIFF_ROW.CONTEXT, text: line.startsWith(SEPARATOR) ? line.slice(1) : line };
}

function patchCopy(args: ToolArgs): ToolCopy {
  const patch = stringArg(args, ARG.PATCH);
  // No patch text, or none in the patch grammar: no counts to claim, so the
  // arguments as they came.
  if (patch === undefined) return plainCopy(VERBS.EDIT, compactArgs(args));
  const { files, diff } = patchDiff(patch);
  const target = files.join(", ");
  // Cut at the leaf cap, a patch's counts and lines would be a part shown as the whole.
  if (mayBeClipped(patch)) return { verbs: VERBS.EDIT, target: target.length > 0 ? `${target}${CLIP_MARK}` : "", body: { kind: TOOL_BODY.CLIPPED_EDIT } };
  if (files.length === 0) return plainCopy(VERBS.EDIT, compactArgs(args));
  return { verbs: VERBS.EDIT, target, body: { kind: TOOL_BODY.PATCH, diff } };
}

function planCopy(args: ToolArgs): ToolCopy {
  const raw: JsonValue | undefined = args[ARG.PLAN];
  const steps = Array.isArray(raw) ? raw.flatMap(planStep) : [];
  const explanation = stringArg(args, ARG.EXPLANATION);
  return { verbs: VERBS.PLAN, target: "", body: { kind: TOOL_BODY.PLAN, explanation: explanation === undefined ? null : clipMarked(explanation), steps } };
}

function planStep(value: JsonValue): PlanStep[] {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return [];
  const step = (value as { readonly [key: string]: JsonValue })[ARG.STEP];
  const status = (value as { readonly [key: string]: JsonValue })[ARG.STATUS];
  if (typeof step !== "string" || typeof status !== "string" || !PLAN_STATUSES.has(status)) return [];
  return [{ step: clipMarked(step), status: status as PlanStatus }];
}

function browserCopy(args: ToolArgs): ToolCopy {
  const action = stringArg(args, ARG.ACTION);
  const selector = clipMarked(stringArg(args, ARG.SELECTOR) ?? "");
  switch (action) {
    case BROWSER_ACTION.CLICK:
      return plainCopy(VERBS.CLICK, selector);
    case BROWSER_ACTION.TYPE:
      return plainCopy(VERBS.TYPE, selector);
    case BROWSER_ACTION.TEXT:
      return plainCopy(VERBS.READ_PAGE, selector.length > 0 ? selector : PAGE);
    case BROWSER_ACTION.WAIT:
      return plainCopy(VERBS.WAIT_FOR, selector);
    default:
      return plainCopy(VERBS.BROWSE, `${action ?? ""} ${selector}`.trim());
  }
}

// The runner caps each word on its own, so each carries its own mark.
function gitCopy(args: ToolArgs): ToolCopy {
  const raw: JsonValue | undefined = args[ARG.ARGS];
  if (!Array.isArray(raw)) return { verbs: VERBS.RUN, target: compactArgs(args), body: { kind: TOOL_BODY.OUTPUT } };
  const words = raw.map((word) => (typeof word === "string" ? clipMarked(word) : JSON.stringify(word)));
  return { verbs: VERBS.RUN, target: [GIT, ...words].join(SEPARATOR), body: { kind: TOOL_BODY.OUTPUT } };
}

// Codex's background terminal: writing keys to a running command, or, with
// none, waiting on it.
function stdinCopy(args: ToolArgs): ToolCopy {
  const session = scalarArg(args, ARG.SESSION);
  const chars = stringArg(args, ARG.CHARS) ?? "";
  const terminal = session === undefined ? TERMINAL : `${TERMINAL} ${session}`;
  if (chars.length === 0) return plainCopy(VERBS.WAIT_FOR, terminal);
  return plainCopy(VERBS.WRITE_TO, `${terminal}: ${keysOf(chars)}`);
}

// Keys as they were sent: Enter, Ctrl-C and their kind are what a terminal
// session mostly receives, and drawn raw they are blank or invisible.
function keysOf(chars: string): string {
  const quoted = JSON.stringify(chars);
  return mayBeClipped(chars) ? `${quoted}${CLIP_MARK}` : quoted;
}

function scheduleCopy(args: ToolArgs): ToolCopy {
  const message = messageOf(args);
  const at = scalarArg(args, ARG.AT);
  return named(VERBS.SCHEDULE, args, [message, at === undefined ? undefined : `at ${at}`]);
}

/** Verbs and the parts a cell knows how to name; with none of them present,
 * the arguments as they came, so nothing is guessed. */
function named(verbs: Verbs, args: ToolArgs, parts: readonly (string | undefined)[]): ToolCopy {
  const known = parts.filter((part) => part !== undefined && part.length > 0);
  return plainCopy(verbs, known.length > 0 ? known.join(SEPARATOR) : compactArgs(args));
}

function scheduleId(args: ToolArgs): string | undefined {
  for (const key of SCHEDULE_ID) {
    const id = scalarArg(args, key);
    if (id !== undefined) return id;
  }
  return undefined;
}

function messageOf(args: ToolArgs): string | undefined {
  const message = firstString(args, SCHEDULE_MESSAGE);
  return message === undefined ? undefined : firstLine(message);
}

function textOf(args: ToolArgs): string | undefined {
  const text = firstString(args, MESSAGE_TEXT);
  return text === undefined ? undefined : firstLine(text);
}
