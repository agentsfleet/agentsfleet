// Keep one exact operation in memory until its outcome is known.

import { Effect } from "effect";
import { v7 as uuidv7 } from "uuid";
import { CLI_ERROR_TAG, ValidationError, type NetworkError, type ServerError } from "../../errors/index.ts";

type RequestError = NetworkError | ServerError;
type Pending<T> = {
  readonly identity: string;
  readonly key: string;
  readonly run: Effect.Effect<T, RequestError>;
};

export class RetainedRequest<T> {
  #pending: Pending<T> | undefined;
  #running: Pending<T> | undefined;
  #initialKey: string | undefined;

  constructor(initialKey?: string) { this.#initialKey = initialKey; }
  get operationId(): string | undefined { return this.#pending?.key; }

  get pending(): boolean { return this.#pending !== undefined; }

  clear(): void { this.#pending = undefined; this.#initialKey = undefined; }

  run(identity: unknown, execute: (key: string) => Effect.Effect<T, RequestError>) {
    return Effect.suspend<T, RequestError | ValidationError, never>(() => {
      const encoded = JSON.stringify(identity);
      if (this.#pending && this.#pending.identity !== encoded) return busy();
      if (!this.#pending) {
        const key = this.#initialKey ?? uuidv7();
        this.#initialKey = undefined;
        this.#pending = {identity: encoded, key, run: execute(key)};
      }
      return this.retry();
    });
  }

  retry(): Effect.Effect<T, RequestError | ValidationError> {
    return Effect.suspend<T, RequestError | ValidationError, never>(() => {
      if (!this.#pending) return Effect.fail(new ValidationError({
        detail: "There is no uncertain operation to retry", suggestion: "continue chatting or review the current setup",
      }));
      const pending = this.#pending;
      if (this.#running === pending) return Effect.fail(new ValidationError({
        detail: "This operation is still in progress", suggestion: "wait for its answer before using /retry",
      }));
      this.#running = pending;
      return pending.run.pipe(
        Effect.tap(() => Effect.sync(() => { if (this.#pending === pending) this.clear(); })),
        Effect.tapError((error) => Effect.sync(() => {
          if (this.#pending === pending && error._tag === CLI_ERROR_TAG.server && error.status >= 400 && error.status < 500 && error.status !== 408) this.clear();
        })),
        Effect.ensuring(Effect.sync(() => { if (this.#running === pending) this.#running = undefined; })),
      );
    });
  }
}

function busy(): Effect.Effect<never, ValidationError> {
  return Effect.fail(new ValidationError({
    detail: "The previous operation has an unknown outcome",
    suggestion: "use /retry with its retained identity before changing its text or setup",
  }));
}
