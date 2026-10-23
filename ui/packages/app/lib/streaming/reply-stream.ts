import { ReplyStreamDecoder, type ReplyDelta } from "./reply-stream-decoder";

type Frame = {
  readonly sequence: number | undefined;
  readonly start: boolean;
  readonly contiguous: boolean;
  readonly kind: unknown;
  readonly text: string;
};

/** A gap ends provisional assembly. Only the purpose's durable reader can repair it. */
export class ReplyStream {
  readonly #decoder: ReplyStreamDecoder;
  #next: number | null = null;
  #closed = false;

  constructor(onDelta: (delta: ReplyDelta) => void) { this.#decoder = new ReplyStreamDecoder(onDelta); }

  get needsFinalReply(): boolean { return this.#decoder.needsFinalReply; }

  write(frame: Frame): boolean {
    if (this.#closed || this.needsFinalReply) return false;
    const {sequence, start, contiguous, kind, text} = frame;
    if (!Number.isSafeInteger(sequence) || sequence === undefined || sequence < 0
      || !contiguous || (kind !== "answer" && kind !== "reasoning")
      || (this.#next === null ? !start || sequence !== 0 : start || sequence !== this.#next)) {
      this.markGap(); return false;
    }
    this.#next = sequence + 1;
    this.#decoder.write(text, kind);
    return true;
  }

  markGap(): void { this.#decoder.markGap(); }
  finish(): Promise<void> { this.#closed = true; return this.#decoder.finish(); }
  dispose(): void { this.#closed = true; this.#decoder.dispose(); }
}
