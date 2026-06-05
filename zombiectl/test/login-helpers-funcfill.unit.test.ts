// Function-coverage backfill for login-helpers.ts. The hydration branches
// live in login-helpers-hydration.unit.test.ts and the distinct-id wiring in
// login-logout-identity.unit.test.ts; the three exports below
// (resolveDirectToken / saveDirectToken / withSigintAbort) are reached only
// through login.ts in those suites, so their inner closures never fire as
// callable units. These tests invoke each directly with in-memory layers.

import { afterEach, beforeEach, describe, expect, spyOn, test } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  resolveDirectToken,
  saveDirectToken,
  withSigintAbort,
} from "../src/commands/login-helpers.ts";
import { Stdin } from "../src/services/stdin.ts";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import {
  TelemetryRuntime,
  telemetryRuntimeFromValuesLayer,
} from "../src/services/telemetry/runtime.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { SIGINT } from "../src/constants/signals.ts";
import {
  InterruptedError,
  ServerError,
} from "../src/errors/index.ts";

const stdinLayer = (
  isTTY: boolean,
  piped = "",
): Layer.Layer<Stdin> =>
  Layer.succeed(Stdin, Stdin.of({ isTTY, readToEnd: Effect.succeed(piped) }));

describe("resolveDirectToken", () => {
  test("--token flag wins, trimmed", async () => {
    const result = await Effect.runPromise(
      resolveDirectToken({ tokenFlag: "  pat_flag  ", envToken: "pat_env" }).pipe(
        Effect.provide(stdinLayer(true)),
      ),
    );
    expect(Option.getOrNull(result)).toBe("pat_flag");
  });

  test("falls back to ZOMBIE_TOKEN env when flag absent", async () => {
    const result = await Effect.runPromise(
      resolveDirectToken({ tokenFlag: undefined, envToken: " pat_env " }).pipe(
        Effect.provide(stdinLayer(true)),
      ),
    );
    expect(Option.getOrNull(result)).toBe("pat_env");
  });

  test("interactive TTY with no token → Option.none (falls through to browser)", async () => {
    const result = await Effect.runPromise(
      resolveDirectToken({ tokenFlag: undefined, envToken: undefined }).pipe(
        Effect.provide(stdinLayer(true)),
      ),
    );
    expect(Option.isNone(result)).toBe(true);
  });

  test("non-TTY pipe carrying a token → Option.some of the trimmed piped value", async () => {
    const result = await Effect.runPromise(
      resolveDirectToken({ tokenFlag: undefined, envToken: undefined }).pipe(
        Effect.provide(stdinLayer(false, "  pat_piped\n")),
      ),
    );
    expect(Option.getOrNull(result)).toBe("pat_piped");
  });

  test("non-TTY with empty stdin → InterruptedError (no token, no terminal)", async () => {
    const exit = await Effect.runPromiseExit(
      resolveDirectToken({ tokenFlag: "   ", envToken: "  " }).pipe(
        Effect.provide(stdinLayer(false, "   \n")),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = exit.cause;
      // The typed failure is InterruptedError; assert via squash.
      const squashed = Exit.isFailure(exit) ? err : null;
      expect(String(squashed)).toContain("InterruptedError");
    }
  });

  test("InterruptedError carries the no-token suggestion", () => {
    const e = new InterruptedError({
      detail: "no token provided and stdin is not a terminal",
      suggestion: "pass --token or set ZOMBIE_TOKEN",
    });
    expect(e.message).toContain("no token provided");
    expect(e.suggestion).toContain("--token");
  });
});

describe("saveDirectToken", () => {
  const okHttpLayer: Layer.Layer<HttpClient> = Layer.succeed(HttpClient, {
    request: () => Effect.succeed({} as never),
  });

  const failHttpLayer: Layer.Layer<HttpClient> = Layer.succeed(HttpClient, {
    request: () =>
      Effect.fail(
        new ServerError({
          detail: "unauthorized",
          suggestion: "re-login",
          code: "UZ-AUTH-401",
          status: 401,
          requestId: "req_x",
        }),
      ) as Effect.Effect<never, ServerError>,
  });

  const outputLayer: Layer.Layer<Output> = Layer.succeed(Output, {
    intro: () => Effect.void,
    info: () => Effect.void,
    success: () => Effect.void,
    warn: () => Effect.void,
    error: () => Effect.void,
    outro: () => Effect.void,
    printJson: () => Effect.void,
    printJsonErr: () => Effect.void,
    printKeyValue: () => Effect.void,
    printSection: () => Effect.void,
    printTable: () => Effect.void,
  });

  const analyticsLayer: Layer.Layer<Analytics> = Layer.succeed(Analytics, {
    capture: () => Effect.void,
    identify: () => Effect.void,
    alias: () => Effect.void,
    groupIdentify: () => Effect.void,
  });

  const configLayer: Layer.Layer<CliConfig> = Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode: false,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

  const credsLayer = (saved: { token: string | null }): Layer.Layer<Credentials> =>
    Layer.succeed(Credentials, {
      getAccessToken: Effect.succeed(Option.none()),
      getSavedAt: Effect.succeed(null),
      getSessionId: Effect.succeed(null),
      getApiUrl: Effect.succeed(null),
      saveAccessToken: (input) =>
        Effect.sync(() => {
          saved.token = Redacted.value(input.token);
        }),
      clearAccessToken: Effect.void,
    });

  const workspacesLayer: Layer.Layer<Workspaces> = Layer.succeed(Workspaces, {
    load: Effect.succeed({ current_workspace_id: null, items: [] }),
    save: () => Effect.void,
  });

  const telemetryRuntime: Layer.Layer<TelemetryRuntime> =
    telemetryRuntimeFromValuesLayer({
      configDir: "/tmp/zombiectl-funcfill-test",
      tracesDir: "/tmp/zombiectl-funcfill-test/traces",
      consent: "granted",
      showDebug: false,
      deviceId: "device-fixture-7",
      sessionId: "session-fixture-7",
      isFirstRun: false,
      isTty: false,
      isCi: true,
      os: "linux",
      arch: "x64",
      cliVersion: "0.0.0-test",
    });

  let tempStateDir: string | null = null;
  let prevStateDir: string | undefined;
  beforeEach(() => {
    prevStateDir = process.env.ZOMBIE_STATE_DIR;
    tempStateDir = fs.mkdtempSync(path.join(os.tmpdir(), "zombiectl-funcfill-"));
    process.env.ZOMBIE_STATE_DIR = tempStateDir;
  });
  afterEach(() => {
    if (tempStateDir) fs.rmSync(tempStateDir, { recursive: true, force: true });
    tempStateDir = null;
    if (prevStateDir === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = prevStateDir;
  });

  test("validates then persists the token on a successful ping", async () => {
    const saved = { token: null as string | null };
    const exit = await Effect.runPromiseExit(
      saveDirectToken("pat_live").pipe(
        Effect.provide(okHttpLayer),
        Effect.provide(outputLayer),
        Effect.provide(analyticsLayer),
        Effect.provide(configLayer),
        Effect.provide(credsLayer(saved)),
        Effect.provide(workspacesLayer),
        Effect.provide(telemetryRuntime),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(saved.token).toBe("pat_live");
  });

  test("leaves credentials untouched when the ping fails", async () => {
    const saved = { token: null as string | null };
    const exit = await Effect.runPromiseExit(
      saveDirectToken("pat_bad").pipe(
        Effect.provide(failHttpLayer),
        Effect.provide(outputLayer),
        Effect.provide(analyticsLayer),
        Effect.provide(configLayer),
        Effect.provide(credsLayer(saved)),
        Effect.provide(workspacesLayer),
        Effect.provide(telemetryRuntime),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(saved.token).toBeNull();
  });
});

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
