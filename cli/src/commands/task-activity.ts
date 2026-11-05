import { Option, Schema } from "effect";
import { isRecord, isString } from "../lib/guards.ts";

const TEXT_LIMIT = 8192;
const label = Schema.String.check(Schema.isMaxLength(256));
const count = Schema.Number.check(Schema.makeFilter((value: number) => Number.isSafeInteger(value) && value >= 0 && value <= 200_000));
const Progress = Schema.Struct({
  activity_id: Schema.String.check(Schema.isPattern(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u)),
  operation: Schema.Literals(["intent", "goal", "capture", "restore"]),
  phase: Schema.Literals(["starting", "recorded", "unavailable", "preparing", "uploading", "verifying", "saved", "restoring", "restored", "failed"]),
  provider: Schema.NullOr(label), model: Schema.NullOr(label), completed: count, total: Schema.NullOr(count),
});
const PHASE = {starting: "Assessing", recorded: "Assessment recorded", unavailable: "Assessment unavailable",
  preparing: "Preparing recovery files", uploading: "Saving encrypted files", verifying: "Verifying recovery data",
  saved: "Recovery point saved", restoring: "Restoring saved progress", restored: "Saved progress restored", failed: "Recovery interrupted"} as const;
const KINDS: ReadonlySet<string> = new Set(["task_decision_started", "task_decision_completed", "task_transfer_progress"]);

/** Untrusted output is plain terminal text, with an explicit bounded preview. */
export function activityText(text: string): string {
  const safe = text.replace(/[\u0000-\u0008\u000b-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/gu, "");
  return safe.length > TEXT_LIMIT ? `${safe.slice(0, TEXT_LIMIT)}\n… output truncated` : safe;
}

/** One event's replay ownership; duplicate polls do not repeat confirmed stages. */
export class ActivityPrinter {
  readonly #seen = new Map<string, string>();
  readonly #print: (line: string) => void;

  constructor(print: (line: string) => void) { this.#print = print; }

  frame(kind: string, payload: Record<string, unknown>): boolean {
    if (!KINDS.has(kind)) return false;
    const parsed = Schema.decodeUnknownOption(Progress)(payload["progress"]);
    if (Option.isNone(parsed)) return true;
    const p = parsed.value;
    const key = `${kind}:${p.activity_id}:${p.operation}:${p.phase}`;
    const line = `${PHASE[p.phase]} · ${p.operation}${p.model ? ` · ${p.provider ?? ""} / ${p.model}` : ""}${p.total === null ? "" : ` · ${p.completed}/${p.total} objects`}`;
    if (this.#seen.get(key) === line) return true;
    this.#seen.set(key, line);
    if (this.#seen.size > 256) {
      const first = this.#seen.keys().next().value;
      if (first !== undefined) this.#seen.delete(first);
    }
    this.#print(activityText(line));
    return true;
  }

  history(value: unknown): void {
    if (!isRecord(value) || !Array.isArray(value["items"])) return;
    for (const item of value["items"].slice(-128)) {
      if (isRecord(item) && isString(item["kind"])) this.frame(item["kind"], item);
    }
    if (value["truncated"] === true && !this.#seen.has("truncated")) {
      this.#seen.set("truncated", "yes");
      this.#print("Earlier progress stages are outside the retained activity window.");
    }
  }
}
