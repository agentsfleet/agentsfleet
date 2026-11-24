// A stopped watcher cannot clear or report errors for its replacement.
import { followDraft, type DraftProgressCallbacks } from "./draft-events";

type Following = {
  readonly id: string; readonly path: string; readonly owner: AbortSignal; readonly view: AbortSignal;
  readonly reconcile: (signal: AbortSignal) => Promise<boolean>;
  readonly progress: DraftProgressCallbacks;
  readonly finished: () => void; readonly failed: (cause: unknown) => void;
};

export class DraftWatch {
  #current: {id: string; controller: AbortController} | undefined;

  follow(options: Following): void {
    this.start(options.id, options.view, async view => {
      // The follow ends when the turn settles or its owner or view stops it.
      const owner = AbortSignal.any([options.owner, view]);
      await followDraft(options.path, options.id, owner, () => options.reconcile(owner), options.progress);
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
