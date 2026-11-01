// A stopped watcher cannot clear or report errors for its replacement.
import { followDraft, type DraftProgressCallbacks } from "./draft-events";

const FOLLOW_LIMIT_MS = 330_000;
type Following = {
  readonly id: string; readonly path: string; readonly owner: AbortSignal; readonly view: AbortSignal;
  readonly reconcile: (signal: AbortSignal) => Promise<boolean>; readonly needsPolling: () => boolean;
  readonly progress: DraftProgressCallbacks;
  readonly finished: () => void; readonly failed: (cause: unknown) => void;
};

export class DraftWatch {
  #current: {id: string; controller: AbortController} | undefined;

  follow(options: Following): void {
    this.start(options.id, options.view, async view => {
      const owner = AbortSignal.any([options.owner, view, AbortSignal.timeout(FOLLOW_LIMIT_MS)]);
      await followDraft(options.path, options.id, owner, () => options.reconcile(owner), options.needsPolling, options.progress);
      if (!view.aborted && !options.owner.aborted) options.finished();
    }, options.failed);
  }

  start(id: string, view: AbortSignal, follow: (signal: AbortSignal) => Promise<void>, failed: (cause: unknown) => void): void {
    if (this.#current?.id === id) return;
    this.stop();
    const current = {id, controller: new AbortController()};
    this.#current = current;
    const signal = AbortSignal.any([view, current.controller.signal]);
    void follow(signal).catch((cause: unknown) => {
      if (!signal.aborted) failed(cause);
    }).finally(() => { if (this.#current === current) this.#current = undefined; });
  }

  stop(): void {
    const current = this.#current;
    this.#current = undefined;
    current?.controller.abort();
  }
}
