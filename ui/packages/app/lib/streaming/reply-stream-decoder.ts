import type { LanguageModelV4StreamPart } from "@ai-sdk/provider";
import { hermesProtocol } from "@ai-sdk-tool/parser";
import { Parser } from "htmlparser2";

const STREAM_PART_ID = "fleet-reply";
const RENDER_INTERVAL_MS = 50;
// htmlparser2 omits unmatched end tags. Check the text Hermes has classified
// before passing it on, so a quoted tool argument cannot trip this guard.
const UNEXPECTED_CLOSE = /<\/(?!think(?:\s|>))[^>]*>/i;

export type ReplyDelta = {
  answer: string;
  reasoning: string;
  thinking: boolean;
};

/** One model pass. Tool calls become structured parser events, never text. */
export class ReplyStreamDecoder {
  #controller: ReadableStreamDefaultController<LanguageModelV4StreamPart> | null = null;
  #parser: Parser;
  #thinking = false;
  #needsFinalReply = false;
  #closed = false;
  #disposed = false;
  #closeTail = "";
  #publishedFirst = false;
  #pending: ReplyDelta | null = null;
  #renderTimer: ReturnType<typeof setTimeout> | null = null;
  #done: Promise<void>;

  constructor(private readonly onDelta: (delta: ReplyDelta) => void) {
    this.#parser = new Parser({
      onopentag: (name) => {
        if (name === "think" && !this.#needsFinalReply && !this.#thinking) this.#thinking = true;
        else this.#needsFinalReply = true;
      },
      ontext: (text) => {
        if (this.#needsFinalReply) return;
        this.#publish(text);
      },
      onclosetag: (name, implied) => {
        if (this.#needsFinalReply) return;
        if (name !== "think" || implied) {
          this.#needsFinalReply = true;
          return;
        }
        this.#thinking = false;
        this.#emit({ answer: "", reasoning: "", thinking: false });
      },
      oncomment: () => { this.#needsFinalReply = true; },
      onprocessinginstruction: () => { this.#needsFinalReply = true; },
    }, { xmlMode: true, decodeEntities: false });
    const source = new ReadableStream<LanguageModelV4StreamPart>({
      start: (controller) => { this.#controller = controller; },
    });
    const parsed = source.pipeThrough(hermesProtocol().createStreamParser({ tools: [] }));
    this.#done = this.#consume(parsed);
  }

  get needsFinalReply(): boolean {
    return this.#needsFinalReply;
  }

  markGap(): void {
    this.#needsFinalReply = true;
    this.#discardPending();
  }

  write(chunk: string): void {
    if (this.#closed || this.#needsFinalReply || chunk.length === 0) return;
    this.#controller?.enqueue({ type: "text-delta", id: STREAM_PART_ID, delta: chunk });
  }

  async finish(): Promise<void> {
    if (!this.#closed) {
      this.#closed = true;
      this.#controller?.close();
    }
    await this.#done;
    this.#flushPending();
  }

  dispose(): void {
    this.#disposed = true;
    this.#discardPending();
    void this.finish();
  }

  async #consume(stream: ReadableStream<LanguageModelV4StreamPart>): Promise<void> {
    try {
      for await (const part of stream) {
        if (part.type !== "text-delta") continue;
        const closingWindow = this.#closeTail + part.delta;
        this.#closeTail = closingWindow.slice(-64);
        if (UNEXPECTED_CLOSE.test(closingWindow)) {
          this.#needsFinalReply = true;
          break;
        }
        this.#parser.write(part.delta);
      }
      this.#parser.end();
    } catch {
      // A parser failure closes this live pass. The durable event remains the
      // source of truth and raw protocol bytes never become display text.
      this.#needsFinalReply = true;
    }
  }

  #publish(text: string): void {
    if (this.#disposed || this.#needsFinalReply || text.length === 0) return;
    this.#emit({
      answer: this.#thinking ? "" : text,
      reasoning: this.#thinking ? text : "",
      thinking: this.#thinking,
    });
  }

  #emit(delta: ReplyDelta): void {
    if (!this.#publishedFirst) {
      this.#publishedFirst = true;
      this.onDelta(delta);
      return;
    }
    const pending = this.#pending;
    this.#pending = pending === null ? delta : {
      answer: pending.answer + delta.answer,
      reasoning: pending.reasoning + delta.reasoning,
      thinking: delta.thinking,
    };
    if (this.#renderTimer === null) {
      this.#renderTimer = setTimeout(() => {
        this.#renderTimer = null;
        this.#flushPending();
      }, RENDER_INTERVAL_MS);
    }
  }

  #flushPending(): void {
    const pending = this.#pending;
    this.#discardPending();
    if (pending !== null && !this.#disposed && !this.#needsFinalReply) this.onDelta(pending);
  }

  #discardPending(): void {
    if (this.#renderTimer !== null) clearTimeout(this.#renderTimer);
    this.#renderTimer = null;
    this.#pending = null;
  }
}
