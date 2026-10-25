import type { DraftReply } from "./draft-replies";
import type { Message } from "./draft-types";
import type { ContextProgress } from "../context-progress";
import { z } from "zod";
import { CONTEXT_STATE, ContextProgressSchema, advancesContext } from "../context-progress";
import { DraftReplies } from "./draft-replies";
import { isRunning, MESSAGE_ROLE } from "./draft-types";

const MAX_STATUS_CHARACTERS = 2_048;
const MAX_STREAMS_PER_MESSAGE = 32;
const NoticeSchema = z.object({
  draft_id: z.uuidv7(), sequence: z.string().regex(/^[1-9][0-9]{0,18}$/), stream_id: z.uuidv7(),
  progress: ContextProgressSchema,
}).strict();
export type DraftCompaction = { readonly sequence: string; readonly progress: ContextProgress };
type Update = { liveReply?: DraftReply | null; contextProgress?: DraftCompaction | null };

/** Private provisional state shares its owner's reset, gap and durable reconciliation. */
export class DraftLive {
  readonly #replies: DraftReplies;
  readonly #seen = new Map<string, ContextProgress>();
  #sequence = "0";
  #generation = 0;
  #completedGeneration = 0;
  #visible = false;

  constructor(private readonly changed: (patch: Update) => void) {
    this.#replies = new DraftReplies(liveReply => {
      if (liveReply?.text) this.#hide();
      this.changed({ liveReply });
    });
  }

  clear(): void {
    this.#replies.clear(); this.#seen.clear(); this.#sequence = "0"; this.#generation = 0; this.#completedGeneration = 0;
    this.#hide();
  }

  gap(): void { this.#replies.gap(); this.#hide(); }

  reconcile(messages: readonly Message[], cutoff: string): void {
    this.#replies.reconcile(messages, cutoff);
    if (BigInt(this.#sequence) <= BigInt(cutoff) || !messages.some(message => message.sequence === this.#sequence && isRunning(message))) {
      this.#seen.clear(); this.#hide();
    }
  }

  accept(data: string, draft: string, cutoff: string, messages: readonly Message[]): boolean {
    return this.#replies.accept(data, draft, cutoff, messages);
  }

  context(data: string, draft: string, cutoff: string, messages: readonly Message[]): boolean {
    if (data.length > MAX_STATUS_CHARACTERS) return false;
    let value: unknown;
    try { value = JSON.parse(data); } catch { return false; }
    const parsed = NoticeSchema.safeParse(value);
    if (!parsed.success) return false;
    const frame = parsed.data;
    if (frame.draft_id !== draft || BigInt(frame.sequence) <= BigInt(cutoff) || BigInt(frame.sequence) < BigInt(this.#sequence)
      || !messages.some(message => message.sequence === frame.sequence && message.role === MESSAGE_ROLE.user && isRunning(message))) return false;
    if (frame.sequence !== this.#sequence) { this.#seen.clear(); this.#sequence = frame.sequence; this.#generation = 0; this.#completedGeneration = 0; }
    const previous = this.#seen.get(frame.stream_id);
    if (frame.progress.generation <= this.#completedGeneration || frame.progress.generation < this.#generation || !advancesContext(previous, frame.progress)
      || (!previous && this.#seen.size >= MAX_STREAMS_PER_MESSAGE)) return false;
    this.#generation = frame.progress.generation;
    if (frame.progress.state === CONTEXT_STATE.resuming) this.#completedGeneration = frame.progress.generation;
    this.#seen.set(frame.stream_id, frame.progress);
    this.#visible = true;
    this.changed({ contextProgress: { sequence: frame.sequence, progress: frame.progress } });
    return true;
  }

  #hide(): void { if (this.#visible) { this.#visible = false; this.changed({ contextProgress: null }); } }
}
