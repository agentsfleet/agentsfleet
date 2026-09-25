import type { StreamTextKind } from "@/lib/api/events";

const RENDER_INTERVAL_MS = 50;
export const MAX_PENDING_CHARS = 64 * 1024;

export type ReplyDelta = {
  answer: string;
  reasoning: string;
  thinking: boolean;
};

/** Folds provider-typed text. No model markup is interpreted in the browser. */
export class ReplyStreamDecoder {
  #closed = false;
  #needsFinalReply = false;
  #publishedFirst = false;
  #pendingKind: StreamTextKind | null = null;
  #pending: string[] = [];
  #pendingChars = 0;
  #renderTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(private readonly onDelta: (delta: ReplyDelta) => void) {}

  get needsFinalReply(): boolean {
    return this.#needsFinalReply;
  }

  markGap(): void {
    this.#needsFinalReply = true;
    this.#discardPending();
  }

  write(chunk: string, kind: StreamTextKind): void {
    if (this.#closed || this.#needsFinalReply || chunk.length === 0) return;
    if (!this.#publishedFirst) {
      this.#publishedFirst = true;
      this.#emit(chunk, kind);
      return;
    }
    if (this.#pendingKind !== null && this.#pendingKind !== kind) this.#flushPending();
    this.#pendingKind = kind;
    this.#pending.push(chunk);
    this.#pendingChars += chunk.length;
    if (this.#pendingChars >= MAX_PENDING_CHARS) {
      this.#flushPending();
      return;
    }
    if (this.#renderTimer === null) {
      this.#renderTimer = setTimeout(() => this.#flushPending(), RENDER_INTERVAL_MS);
    }
  }

  #emit(chunk: string, kind: StreamTextKind): void {
    this.onDelta({
      answer: kind === "answer" ? chunk : "",
      reasoning: kind === "reasoning" ? chunk : "",
      thinking: kind === "reasoning",
    });
  }

  async finish(): Promise<void> {
    if (this.#closed) return;
    this.#flushPending();
    this.#closed = true;
  }

  dispose(): void {
    this.#closed = true;
    this.#discardPending();
  }

  #flushPending(): void {
    if (this.#renderTimer !== null) clearTimeout(this.#renderTimer);
    this.#renderTimer = null;
    const kind = this.#pendingKind;
    if (kind === null) return;
    const chunk = this.#pending.join("");
    this.#pendingKind = null;
    this.#pending = [];
    this.#pendingChars = 0;
    this.#emit(chunk, kind);
  }

  #discardPending(): void {
    if (this.#renderTimer !== null) clearTimeout(this.#renderTimer);
    this.#renderTimer = null;
    this.#pendingKind = null;
    this.#pending = [];
    this.#pendingChars = 0;
  }
}
