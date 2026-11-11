import { stripVTControlCharacters } from "node:util";
import { Option, Schema } from "effect";
import { isRecord, isString } from "../../lib/guards.ts";

const TEXT_LIMIT = 8192;
const label = Schema.String.check(Schema.isMaxLength(256));
const count = Schema.Number.check(Schema.makeFilter((value: number) => Number.isSafeInteger(value) && value >= 0 && value <= 200_000));
const byteCount = Schema.Number.check(Schema.makeFilter((value: number) => Number.isSafeInteger(value) && value >= 0 && value <= 68_719_476_736));
const Progress = Schema.Struct({
  sequence: Schema.optionalKey(Schema.Number.check(Schema.makeFilter((value: number) => Number.isSafeInteger(value) && value >= 0))),
  activity_id: Schema.String.check(Schema.isPattern(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u)),
  operation: Schema.Literals(["intent", "goal", "capture", "restore"]),
  phase: Schema.Literals(["starting", "recorded", "unavailable", "preparing", "scanning", "uploading", "verifying", "saved", "restoring", "restored", "failed"]),
  provider: Schema.NullOr(label), model: Schema.NullOr(label), completed: count, total: Schema.NullOr(count),
  bytes: Schema.optionalKey(Schema.Struct({acknowledged: byteCount, missing: byteCount, reused: byteCount})),
});
const PHASE = {starting: "Assessing", recorded: "Assessment recorded", unavailable: "Assessment unavailable",
  preparing: "Preparing recovery files", scanning: "Scanning frozen recovery files", uploading: "Saving encrypted files", verifying: "Verifying recovery data",
  saved: "Recovery point saved", restoring: "Restoring saved progress", restored: "Saved progress restored", failed: "Recovery interrupted"} as const;
const KINDS: ReadonlySet<string> = new Set(["task_decision_started", "task_decision_completed", "task_transfer_progress"]);
const RANK = {starting: 0, preparing: 0, scanning: 1, uploading: 2, verifying: 3, restoring: 4,
  unavailable: 5, failed: 5, recorded: 6, saved: 6, restored: 6} as const;

/** Untrusted output is plain terminal text, with an explicit bounded preview. */
export function activityText(text: string): string {
  const safe = stripVTControlCharacters(text).replace(/[\u0000-\u0008\u000b-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/gu, "");
  return safe.length > TEXT_LIMIT ? `${safe.slice(0, TEXT_LIMIT)}\n… output truncated` : safe;
}

/** One event's replay ownership; duplicate polls do not repeat confirmed stages. */
export class ActivityPrinter {
  readonly #seen = new Map<string, string>();
  readonly #latest = new Map<string, typeof Progress.Type>();
  readonly #print: (line: string) => void;

  constructor(print: (line: string) => void) { this.#print = print; }

  frame(kind: string, payload: Record<string, unknown>): boolean {
    if (!KINDS.has(kind)) return false;
    const parsed = Schema.decodeUnknownOption(Progress)(payload["progress"]);
    if (Option.isNone(parsed)) return true;
    const p = parsed.value;
    const operation = `${p.activity_id}:${p.operation}`;
    const previous = this.#latest.get(operation);
    if (previous && ((previous.sequence !== undefined && (p.sequence === undefined || previous.sequence > p.sequence))
      || RANK[previous.phase] > RANK[p.phase]
      || previous.completed > p.completed || (previous.bytes?.acknowledged ?? 0) > (p.bytes?.acknowledged ?? 0)
      || (previous.bytes?.reused ?? 0) > (p.bytes?.reused ?? 0))) return true;
    this.#latest.set(operation, p);
    if (this.#latest.size > 128) {
      const oldest = this.#latest.keys().next().value;
      if (oldest !== undefined) this.#latest.delete(oldest);
    }
    const key = `${kind}:${p.activity_id}:${p.operation}:${p.phase}`;
    const bytes = p.bytes ? ` · ${p.bytes.acknowledged} bytes acknowledged · ${p.bytes.missing} known missing bytes · ${p.bytes.reused} bytes reused` : "";
    const line = `${PHASE[p.phase]} · ${p.operation}${p.model ? ` · ${p.provider ?? ""} / ${p.model}` : ""}${p.total === null ? "" : ` · ${p.completed}/${p.total} objects`}${bytes}`;
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
