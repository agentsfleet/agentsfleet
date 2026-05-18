// Spinner service — Effect-shaped wrapper around the imperative
// createSpinner from ui-progress. Login wraps its poll loop in
// `withSpinner` so the terminal carries the "waiting for browser
// login" frames while the dispatcher's analytics + output Effects
// continue to compose normally.
//
// All side effects (interval timer, ANSI writes) live inside
// `Effect.sync` callbacks; the surface is `Effect<void>` so commands
// don't have to track a fiber for the spinner timer.

import { Effect, Layer, Context } from "effect";
import { createSpinner as createSpinnerRaw } from "../ui-progress.ts";
import type { SpinnerOptions } from "../commands/types.ts";

export interface SpinnerHandleShape {
  readonly succeed: (message?: string) => Effect.Effect<void>;
  readonly fail: (message?: string) => Effect.Effect<void>;
  readonly stop: Effect.Effect<void>;
}

export interface SpinnerShape {
  readonly start: (opts: SpinnerOptions) => Effect.Effect<SpinnerHandleShape>;
}

export class Spinner extends Context.Service<Spinner, SpinnerShape>()(
  "zombiectl/runtime/Spinner",
) {}

export const spinnerLayer: Layer.Layer<Spinner> = Layer.succeed(
  Spinner,
  Spinner.of({
    start: (opts) =>
      Effect.sync(() => {
        const handle = createSpinnerRaw(opts);
        handle.start();
        return {
          succeed: (message) => Effect.sync(() => handle.succeed?.(message)),
          fail: (message) => Effect.sync(() => handle.fail?.(message)),
          stop: Effect.sync(() => handle.stop?.()),
        };
      }),
  }),
);
