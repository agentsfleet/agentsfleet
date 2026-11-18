import { Option, Schema } from "effect";

const MAX_FENCE = "9223372036854775807";
const FENCE = Schema.String.check(Schema.isPattern(/^[1-9][0-9]{0,18}$/u),
  Schema.makeFilter((value: string) => value.length < MAX_FENCE.length || value <= MAX_FENCE));
const OUTPUT_KINDS: ReadonlySet<string> = new Set([
  "chunk", "tool_call_started", "tool_call_progress", "tool_call_completed",
  "context_progress", "task_decision_started", "task_decision_completed",
  "task_transfer_progress", "run_restarted",
]);

/** One event's live attempt; a saved completion permanently closes its output. */
export class AttemptOutput {
  #fence: string | null = null;
  #terminal = false;
  readonly #print: (line: string) => void;
  readonly #reset: () => void;

  constructor(print: (line: string) => void, reset: () => void) {
    this.#print = print;
    this.#reset = reset;
  }

  accept(kind: string, payload: Record<string, unknown>): boolean {
    if (this.#terminal) return false;
    if (kind === "event_complete") this.#terminal = true;
    if (!OUTPUT_KINDS.has(kind)) return true;
    const parsed = Schema.decodeUnknownOption(FENCE)(payload["fencing_token"]);
    if (Option.isNone(parsed)) return false;
    const fence = parsed.value;
    const previous = this.#fence;
    if (previous !== null && (fence.length < previous.length
      || (fence.length === previous.length && fence < previous))) return false;
    if (previous !== fence) {
      this.#fence = fence;
      if (previous !== null || kind === "run_restarted") {
        this.#reset();
        this.#print("\nRun restarted\n");
      }
    }
    return kind !== "run_restarted";
  }
}
