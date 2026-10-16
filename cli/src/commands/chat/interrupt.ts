import { Effect } from "effect";
import type { CliError } from "../../errors/index.ts";
import type { Output } from "../../services/output.ts";
import { SIGINT } from "../../constants/signals.ts";

type InterruptSource = {
  on(event: typeof SIGINT, listener: () => void): unknown;
  off(event: typeof SIGINT, listener: () => void): unknown;
};

export function stopOnInterrupt<T>(action: Effect.Effect<T, CliError>, stop: Effect.Effect<void, CliError>, output: Output,
  source: InterruptSource = process): Effect.Effect<T, CliError> {
  return Effect.acquireUseRelease(Effect.sync(() => {
    const controller = new AbortController();
    let pending: Promise<void> | undefined;
    const listener = (): void => {
      if (pending) return;
      const attempt = Effect.runPromise(stop.pipe(Effect.catch((error) => output.warn(`${error.detail}. ${error.suggestion}`))),
        {signal: controller.signal}).catch(() => undefined);
      pending = attempt;
      void attempt.finally(() => { if (pending === attempt) pending = undefined; });
    };
    source.on(SIGINT, listener);
    return Object.freeze({listener, controller, settle: () => pending});
  }), () => action, (owned) => Effect.promise(async () => {
    source.off(SIGINT, owned.listener);
    owned.controller.abort();
    await owned.settle();
  }));
}
