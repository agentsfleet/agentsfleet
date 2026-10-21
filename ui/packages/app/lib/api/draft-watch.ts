// A stopped watcher cannot clear or report errors for its replacement.

export class DraftWatch {
  #current: {id: string; controller: AbortController} | undefined;

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
