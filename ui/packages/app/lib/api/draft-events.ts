// Browser-native framing and reconnects; durable reads own visible text.

import { z } from "zod";
import { ChatConnection } from "../streaming/chat-connection";
import { fastBackoffMs, FAST_RECONNECT_ATTEMPTS, OFFLINE_RETRY_MS } from "../streaming/fleet-stream-reconnect";
import { losesAccess, SetupUnavailable } from "./draft-request";

export const DRAFT_EVENT = {
  accepted: "message.accepted", running: "message.running", completed: "message.completed",
  cancelled: "message.cancelled", failed: "message.failed", timedOut: "message.timed_out",
  candidate: "candidate.changed", closed: "draft.closed", deleted: "draft.deleted",
} as const;
const MAX_HINT_CHARACTERS = 64 * 1024;
const ACCOUNTING_POLL_MS = 5_000;
const ACCOUNTING_POLL_JITTER_MS = 1_000;
const HintSchema = z.object({draft_id: z.uuidv7()});
export const DRAFT_DELTA_EVENT = "message.delta";
export const DRAFT_RESYNC_EVENT = "draft.resync";
type Progress = {readonly delta: (data: string) => boolean; readonly gap: () => void};

export async function followDraft(path: string, draft: string, owner: AbortSignal, reconcile: () => Promise<boolean>, needsPolling: () => boolean = () => false, progress?: Progress): Promise<void> {
  owner.throwIfAborted();
  await new DraftProgress(path, draft, reconcile, needsPolling, progress).follow(owner);
}

class DraftProgress {
  readonly #source: ChatConnection;
  readonly #draft: string;
  readonly #reconcile: () => Promise<boolean>;
  readonly #needsPolling: () => boolean;
  readonly #stopped = Promise.withResolvers<void>();
  #reading = false;
  #queued = false;
  #closed = false;
  #failures = 0;
  #retryTimer: ReturnType<typeof setTimeout> | undefined;
  #pollTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(path: string, draft: string, reconcile: () => Promise<boolean>, needsPolling: () => boolean, progress?: Progress) {
    this.#draft = draft; this.#reconcile = reconcile; this.#needsPolling = needsPolling;
    this.#source = new ChatConnection({
      url: `${path}/events/stream`,
      events: [...Object.values(DRAFT_EVENT), DRAFT_DELTA_EVENT, DRAFT_RESYNC_EVENT],
      onOpen: () => {
        clearTimeout(this.#retryTimer); this.#retryTimer = undefined;
        void this.#refresh();
      },
      onInterrupted: () => { progress?.gap(); void this.#refresh(); },
      onFrame: (name, data) => {
        if (name === DRAFT_DELTA_EVENT) return progress?.delta(data) ?? false;
        if (name === DRAFT_RESYNC_EVENT) { progress?.gap(); void this.#refresh(); return true; }
        return this.#hint(name, data);
      },
    });
    this.#source.start();
  }

  async follow(owner: AbortSignal): Promise<void> {
    const close = (): void => this.#stop();
    owner.addEventListener("abort", close, {once: true});
    try { if (owner.aborted) this.#stop(); await this.#stopped.promise; }
    finally { this.#stop(); owner.removeEventListener("abort", close); }
  }

  #stop(cause?: unknown): void {
    this.#closed = true; this.#source.close(); clearTimeout(this.#retryTimer); clearTimeout(this.#pollTimer);
    if (cause === undefined) this.#stopped.resolve(); else this.#stopped.reject(cause);
  }

  async #refresh(): Promise<void> {
    if (this.#closed || this.#retryTimer !== undefined) return;
    clearTimeout(this.#pollTimer); this.#pollTimer = undefined;
    if (this.#reading) { this.#queued = true; return; }
    this.#reading = true;
    try {
      do {
        this.#queued = false;
        if (!await this.#reconcile()) { this.#stop(); return; }
        this.#failures = 0;
      } while (this.#needsRead());
      this.#pollAccounting();
    } catch (cause) {
      this.#recover(cause);
    } finally { this.#reading = false; }
  }

  #needsRead(): boolean {
    return this.#queued && !this.#closed;
  }

  #pollAccounting(): void {
    if (this.#closed || !this.#needsPolling()) return;
    const delay = ACCOUNTING_POLL_MS + Math.floor(Math.random() * ACCOUNTING_POLL_JITTER_MS);
    this.#pollTimer = setTimeout(() => { this.#pollTimer = undefined; void this.#refresh(); }, delay);
  }

  #recover(cause: unknown): void {
    if (this.#closed) return;
    if (losesAccess(cause)) { this.#stop(cause); return; }
    const delay = this.#failures < FAST_RECONNECT_ATTEMPTS ? fastBackoffMs(this.#failures++) : OFFLINE_RETRY_MS;
    this.#retryTimer = setTimeout(() => { this.#retryTimer = undefined; void this.#refresh(); }, delay);
  }

  #hint(name: string, data: string): boolean {
    if (this.#closed || data.length > MAX_HINT_CHARACTERS || !Object.values<string>(DRAFT_EVENT).includes(name)) return false;
    let value: unknown;
    try { value = JSON.parse(data); } catch { return false; }
    const hint = HintSchema.safeParse(value);
    if (!hint.success || hint.data.draft_id !== this.#draft) return false;
    if (name === DRAFT_EVENT.deleted) this.#stop(new SetupUnavailable("This private draft was deleted", 404));
    else void this.#refresh();
    return true;
  }
}
