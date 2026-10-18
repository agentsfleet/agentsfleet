// Rotate before the hosting cutoff; the client reconnects and refreshes durable state.

const HANDSHAKE_TIMEOUT_MS = 30_000;
const STREAM_ROTATION_MS = 240_000;
type Failure = {error: unknown};

export class StreamLifetime {
  readonly #upstream = new AbortController();
  readonly signal: AbortSignal;
  readonly #handshake: ReturnType<typeof setTimeout>;
  readonly #rotation: ReturnType<typeof setTimeout>;
  #reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
  #downstream: ReadableStreamDefaultController<Uint8Array> | null = null;
  #closed = false;
  readonly #onAbort = () => { void this.#finish(); };

  constructor(owner: AbortSignal) {
    this.signal = AbortSignal.any([owner, this.#upstream.signal]);
    this.#handshake = setTimeout(this.#onAbort, HANDSHAKE_TIMEOUT_MS);
    this.#rotation = setTimeout(this.#onAbort, STREAM_ROTATION_MS);
    this.signal.addEventListener("abort", this.#onAbort, {once: true});
    if (this.signal.aborted) this.dispose();
  }

  wrap(body: ReadableStream<Uint8Array> | null): ReadableStream<Uint8Array> | null {
    clearTimeout(this.#handshake);
    if (!body) { this.dispose(); return null; }
    const reader = body.getReader();
    this.#reader = reader;
    return new ReadableStream<Uint8Array>({
      start: (controller) => {
        this.#downstream = controller;
        if (this.#closed) { controller.close(); void this.#cancelReader(); }
      },
      pull: async (controller) => {
        try {
          const chunk = await reader.read();
          if (this.#closed) return;
          if (chunk.done) await this.#finish();
          else controller.enqueue(chunk.value);
        } catch (error) {
          await this.#finish({error});
        }
      },
      cancel: () => { this.#downstream = null; return this.#finish(); },
    }, {highWaterMark: 0});
  }

  dispose(): void { void this.#finish(); }

  async #finish(failure: Failure | null = null): Promise<void> {
    if (this.#closed) return;
    this.#closed = true;
    clearTimeout(this.#handshake);
    clearTimeout(this.#rotation);
    this.signal.removeEventListener("abort", this.#onAbort);
    if (failure) this.#downstream?.error(failure.error);
    else this.#downstream?.close();
    this.#upstream.abort();
    await this.#cancelReader();
  }

  async #cancelReader(): Promise<void> {
    const reader = this.#reader;
    if (!reader) return;
    this.#reader = null;
    try { await reader.cancel(); }
    catch { /* An aborted network body can already be errored. */ }
    finally { reader.releaseLock(); }
  }
}
