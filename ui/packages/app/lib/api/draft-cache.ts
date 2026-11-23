import type { DraftIdentity, DraftView } from "./draft-state";

export type CachedDraft = Pick<DraftView, "draft" | "messages" | "model" | "truncated">;
export type DraftSession = {readonly cache: DraftCache; readonly titleWrites: DraftTitleWrites};
// An in-memory cache of recently opened chats, sized to the tab, not to any
// server rule; a miss reads the chat from the server again.
const MAX_CHATS = 5;
const MAX_BYTES = 1024 * 1024;
const RETAIN_MS = 120_000;
const encoder = new TextEncoder();
type Entry = {snapshot: CachedDraft; expires: number; bytes: number};

/** One mounted account/workspace/creator owns this bounded memory cache. */
export class DraftCache {
  readonly #entries = new Map<string, Entry>();
  #bytes = 0;

  invalidate(id: string): void { this.#remove(id); }

  clear(): void {this.#entries.clear(); this.#bytes = 0;}

  read(id: string): CachedDraft | undefined {
    const entry = this.#entries.get(id);
    if (!entry) return undefined;
    if (entry.expires <= Date.now()) {this.#remove(id); return undefined;}
    return entry.snapshot;
  }

  remember(view: DraftView): void {
    if (view.inaccessible) {this.clear(); return;}
    const id = view.draft?.id;
    if (!id) return;
    this.#remove(id);
    const snapshot = {draft: view.draft, messages: view.messages, model: view.model, truncated: view.truncated};
    const json = JSON.stringify(snapshot);
    if (json.length > MAX_BYTES) return;
    const bytes = encoder.encode(json).byteLength;
    if (bytes > MAX_BYTES) return;
    for (const oldest of this.#entries.keys()) {
      if (this.#entries.size < MAX_CHATS && this.#bytes + bytes <= MAX_BYTES) break;
      this.#remove(oldest);
    }
    this.#entries.set(id, {snapshot, bytes, expires: Date.now() + RETAIN_MS});
    this.#bytes += bytes;
  }

  #remove(id: string): void {
    this.#bytes -= this.#entries.get(id)?.bytes ?? 0;
    this.#entries.delete(id);
  }
}

/** The signed-in dashboard owns confirmed title writes across its child routes. */
export class DraftTitleWrites {
  #owner = new AbortController();
  #key = "";
  #workspace: string | null = null;
  readonly #pending = new Map<string, Promise<void>>();

  select({account, workspace, subject}: DraftIdentity): void {
    const key = JSON.stringify([account, workspace, subject]);
    if (key === this.#key) return;
    this.clear();
    this.#key = key;
    this.#workspace = workspace;
  }

  navigate(workspace: string | null): void {
    if (workspace && workspace !== this.#workspace) this.clear();
  }

  async run(id: string, write: (signal: AbortSignal) => Promise<void>): Promise<void> {
    const pending = write(this.#owner.signal);
    this.#pending.set(id, pending);
    try { await pending; }
    finally { if (this.#pending.get(id) === pending) this.#pending.delete(id); }
  }

  async waitFor(id: string, signal: AbortSignal): Promise<void> {
    const pending = this.#pending.get(id);
    if (!pending) return;
    signal.throwIfAborted();
    const stopped = Promise.withResolvers<void>();
    const stop = () => stopped.resolve();
    signal.addEventListener("abort", stop, {once: true});
    try { await Promise.race([pending.catch(() => undefined), stopped.promise]); }
    finally { signal.removeEventListener("abort", stop); }
  }

  clear(): void {
    this.#owner.abort();
    this.#owner = new AbortController();
    this.#key = "";
    this.#workspace = null;
    this.#pending.clear();
  }
}
