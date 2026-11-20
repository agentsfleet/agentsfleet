import { isRecord, isString } from "../../lib/guards.ts";
import { activityText } from "./task-activity.ts";
import { fileActivity } from "./file-activity.ts";

// The daemon's tool-call frame kinds, named once for every reader of them.
export const KIND_TOOL_CALL_STARTED = "tool_call_started" as const;
export const KIND_TOOL_CALL_PROGRESS = "tool_call_progress" as const;
export const KIND_TOOL_CALL_COMPLETED = "tool_call_completed" as const;
export const TOOL_CALL_KINDS: ReadonlySet<string> = new Set([KIND_TOOL_CALL_STARTED, KIND_TOOL_CALL_PROGRESS, KIND_TOOL_CALL_COMPLETED]);
const STATUSES = new Set(["succeeded", "failed", "interrupted"]);

/** One owned turn's command observations and bounded saved-result readback. */
export class ToolActivityPrinter {
  readonly #print: (line: string) => void;
  readonly #seen = new Map<string, string>();

  constructor(print: (line: string) => void) { this.#print = print; }

  reset(): void { this.#seen.clear(); }

  frame(kind: string, payload: Record<string, unknown>): boolean {
    if (!TOOL_CALL_KINDS.has(kind)) return false;
    const name = payload["name"];
    if (!isString(name)) return true;
    const status = kind === KIND_TOOL_CALL_STARTED ? "starting" : kind === KIND_TOOL_CALL_PROGRESS ? "running"
      : isString(payload["status"]) && STATUSES.has(payload["status"]) ? payload["status"] : "outcome unavailable";
    const duration = payload["ms"] ?? payload["elapsed_ms"];
    const lines = [`[tool] ${name} ${status}${typeof duration === "number" && Number.isFinite(duration) && duration >= 0 ? ` ${duration}ms` : ""}`];
    const args = payload["args_redacted"];
    const edited = fileActivity(name, payload["changes"], status);
    if (edited !== undefined) lines.push(edited);
    else if (isRecord(args)) lines.push(JSON.stringify(args, null, 2));
    const head = payload["output_head"];
    const tail = payload["output_tail"];
    if (isString(head) && head) lines.push(head);
    if (isString(tail) && tail && tail !== head) lines.push("…", tail);
    if (typeof payload["exit_code"] === "number" && Number.isSafeInteger(payload["exit_code"])) lines.push(`Exit ${payload["exit_code"]}`);
    const text = activityText(lines.join("\n"));
    const call = payload["call_id"];
    if (isString(call)) {
      const key = `${kind}:${call}`;
      if (this.#seen.get(key) === text) return true;
      this.#seen.set(key, text);
      if (this.#seen.size > 600) {
        const oldest = this.#seen.keys().next().value;
        if (oldest !== undefined) this.#seen.delete(oldest);
      }
    }
    this.#print(text);
    return true;
  }

  history(value: unknown): void {
    if (!isRecord(value) || !Array.isArray(value["calls"])) return;
    for (const call of value["calls"].slice(0, 200)) {
      if (isRecord(call)) this.frame(KIND_TOOL_CALL_COMPLETED, {...call, args_redacted: call["arguments"], ms: call["duration_ms"]});
    }
    const omitted = value["omitted_call_count"];
    if (typeof omitted === "number" && Number.isSafeInteger(omitted) && omitted > 0) this.#print(`${omitted} earlier tool calls are outside the saved trace.`);
  }
}
