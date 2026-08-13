// Function-coverage backfill for login-helpers.ts. The hydration branches
// live in login-helpers-hydration.unit.test.ts and the distinct-id wiring in
// login-logout-identity.unit.test.ts; withSigintAbort is reached only through
// login.ts in those suites, so its inner closures never fire as callable
// units. These tests invoke it directly with in-memory layers.
//
// resolveDirectToken and saveDirectToken were covered here until
// direct-token seeding was retired; both exports are gone, so their blocks went
// with them rather than being kept as tests of nothing.

import { describe, expect, spyOn, test } from "bun:test";
import { Effect } from "effect";
import { withSigintAbort } from "../src/commands/login-helpers.ts";
import { SIGINT } from "../src/constants/signals.ts";

describe("withSigintAbort", () => {
  // Stub process.on/removeListener into a local registry so the registered
  // handler never touches the global `process` listener table. The earlier
  // `process.emit(SIGINT)` + `process.listenerCount` assertions raced other
  // suites under the full --coverage run (shared global signal state); driving
  // a captured handler directly is deterministic and still exercises every
  // line of withSigintAbort's acquire/use/release scope.
  type SigHandler = NodeJS.SignalsListener;

  function withStubbedProcessSignals<T>(
    run: (registry: Set<SigHandler>) => Promise<T>,
  ): Promise<T> {
    const handlers = new Set<SigHandler>();
    const onSpy = spyOn(process, "on").mockImplementation(((event: string | symbol, listener: SigHandler) => {
      if (event === SIGINT) handlers.add(listener);
      return process;
    }) as typeof process.on);
    const offSpy = spyOn(process, "removeListener").mockImplementation(((event: string | symbol, listener: SigHandler) => {
      if (event === SIGINT) handlers.delete(listener);
      return process;
    }) as typeof process.removeListener);
    return run(handlers).finally(() => {
      onSpy.mockRestore();
      offSpy.mockRestore();
    });
  }

  test("registers a SIGINT listener for the body and removes it after", async () => {
    await withStubbedProcessSignals(async (handlers) => {
      let signalledAborted = false;
      let liveDuringBody = -1;
      const result = await Effect.runPromise(
        withSigintAbort((signal) =>
          Effect.sync(() => {
            // The body sees a live, un-aborted controller signal + a live listener.
            signalledAborted = signal.aborted;
            liveDuringBody = handlers.size;
            return "done";
          }),
        ),
      );
      expect(result).toBe("done");
      expect(signalledAborted).toBe(false);
      expect(liveDuringBody).toBe(1);
      // Release removed the listener — registry is empty again.
      expect(handlers.size).toBe(0);
    });
  });

  test("a SIGINT during the body aborts the controller signal", async () => {
    await withStubbedProcessSignals(async (handlers) => {
      let aborted = false;
      await Effect.runPromise(
        withSigintAbort((signal) =>
          Effect.promise(
            () =>
              new Promise<void>((resolve) => {
                signal.addEventListener(
                  "abort",
                  () => {
                    aborted = true;
                    resolve();
                  },
                  { once: true },
                );
                // Fire the captured handler directly — no global signal broadcast.
                queueMicrotask(() => {
                  for (const h of handlers) h(SIGINT);
                });
              }),
          ),
        ),
      );
      expect(aborted).toBe(true);
      expect(handlers.size).toBe(0);
    });
  });
});

test("withSigintAbort aborts the signal on SIGINT and removes its listener after", async () => {
  const { withSigintAbort } = await import("../src/commands/login-helpers.ts");
  const before = process.listenerCount("SIGINT");
  let observed: AbortSignal | null = null;
  await Effect.runPromise(
    withSigintAbort((signal) =>
      Effect.sync(() => {
        observed = signal;
        // Fire the handler exactly as an OS SIGINT would, without killing the
        // test process: emit on the event, not via process.kill.
        process.emit("SIGINT" as never);
      }),
    ),
  );
  expect(observed).not.toBeNull();
  expect((observed as unknown as AbortSignal).aborted).toBe(true);
  // The release arm removed the listener — no leak across invocations.
  expect(process.listenerCount("SIGINT")).toBe(before);
});
