import { z } from "zod";
import { ReplyStream } from "../streaming/reply-stream";
import { isRunning, MESSAGE_ROLE, type Message } from "./draft-types";

const MAX_FRAME_BYTES = 8192;
const DELTA_BYTES = 4096;
const MAX_ANSWER_CHARACTERS = 128 * 1024;
const MAX_STREAMS_PER_MESSAGE = 32;
const encoder = new TextEncoder();
const DeltaSchema = z.object({
  draft_id: z.uuidv7(), sequence: z.string().regex(/^[1-9][0-9]{0,18}$/), stream_id: z.uuidv7(),
  stream_seq: z.number().int().positive().safe(), stream_start: z.boolean(), text_kind: z.literal("answer"),
  text: z.string().min(1).refine(text => encoder.encode(text).byteLength <= DELTA_BYTES),
});
export type DraftReply = {readonly sequence: string; readonly stream: string; readonly text: string; readonly gapped: boolean};
type Current = {reply: DraftReply; readonly decoder: ReplyStream};

/** Bounded provisional text is owned by its private client, separate from durable messages. */
export class DraftReplies {
  #current: Current | null = null;
  readonly #seen = new Set<string>();

  constructor(private readonly changed: (reply: DraftReply | null) => void) {}

  clear(): void { this.#current?.decoder.dispose(); this.#current = null; this.#seen.clear(); this.changed(null); }

  gap(): void {
    const current = this.#current;
    if (current) {current.decoder.markGap(); this.#publish(current, {...current.reply, gapped: true});}
  }

  reconcile(messages: readonly Message[], cutoff: string): void {
    const reply = this.#current?.reply;
    if (!reply) return;
    if (BigInt(reply.sequence) <= BigInt(cutoff) || messages.some(message => message.sequence === reply.sequence
      && message.role === MESSAGE_ROLE.assistant && !isRunning(message))) this.clear();
  }

  accept(data: string, draft: string, cutoff: string, messages: readonly Message[]): boolean {
    if (data.length > MAX_FRAME_BYTES || encoder.encode(data).byteLength > MAX_FRAME_BYTES) return false;
    let value: unknown;
    try { value = JSON.parse(data); } catch { return false; }
    const parsed = DeltaSchema.safeParse(value);
    if (!parsed.success) return false;
    const frame = parsed.data;
    if (frame.draft_id !== draft || BigInt(frame.sequence) <= BigInt(cutoff)
      || messages.some(message => message.sequence === frame.sequence && message.role === MESSAGE_ROLE.assistant && !isRunning(message))) return false;
    let current = this.#current;
    if (current && BigInt(frame.sequence) < BigInt(current.reply.sequence)) return false;
    if (current?.reply.stream !== frame.stream_id) {
      if (current?.reply.sequence !== frame.sequence) this.#seen.clear();
      if (this.#seen.has(frame.stream_id)) return false;
      if (this.#seen.size >= MAX_STREAMS_PER_MESSAGE) {this.gap(); return false;}
      current = this.#begin(frame.sequence, frame.stream_id);
    }
    const accepted = current.decoder.write({sequence: frame.stream_seq - 1, start: frame.stream_start,
      contiguous: true, kind: frame.text_kind, text: frame.text});
    if (!accepted) this.gap();
    return true;
  }

  #begin(sequence: string, stream: string): Current {
    this.#current?.decoder.dispose();
    this.#seen.add(stream);
    const current: Current = {reply: {sequence, stream, text: "", gapped: false}, decoder: new ReplyStream(delta => {
      const reply = current.reply;
      if (reply.text.length + delta.answer.length > MAX_ANSWER_CHARACTERS) { this.gap(); return; }
      this.#publish(current, {...reply, text: reply.text + delta.answer});
    })};
    this.#current = current;
    return current;
  }

  #publish(current: Current, reply: DraftReply): void { current.reply = reply; this.changed(reply); }
}
