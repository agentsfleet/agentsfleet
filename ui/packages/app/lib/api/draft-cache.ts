import type { DraftView } from "./draft-state";

export type CachedDraft = Pick<DraftView, "draft" | "messages" | "model" | "truncated">;
const MAX_CHATS = 5;
const MAX_BYTES = 1024 * 1024;
const RETAIN_MS = 120_000;
const encoder = new TextEncoder();
type Entry = {snapshot: CachedDraft; expires: number; bytes: number};

/** One mounted account/workspace/creator owns this bounded memory cache. */
export class DraftCache {
  readonly #entries = new Map<string, Entry>();
  #bytes = 0;

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
