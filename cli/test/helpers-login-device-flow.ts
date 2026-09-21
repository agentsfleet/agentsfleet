// Shared fixtures and doubles for the device-flow helper suites.
//
// Extracted when login-device-flow.unit.test.ts passed the repository's 350-line cap;
// the suites that read them are unchanged.

import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Input } from "../src/services/input.ts";
import { Output } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { NetworkError, ServerError, type CliError } from "../src/errors/index.ts";

// Service-layer fakes for the interactive / transport branches. The pure
// functions above need no layers; the helpers below drive Credentials,
// Input, Output, CliConfig, HttpClient through Layer.succeed stubs.
export const outputNoop: Layer.Layer<Output> = Layer.succeed(Output, {
...outputDouble(),
});

export const outputRecording = (rec: { warnings: string[] }): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    ...outputDouble(),
    warn: (msg) => Effect.sync(() => rec.warnings.push(msg)),
  });

export const inputReturning = (answer: string): Layer.Layer<Input> =>
  Layer.succeed(Input, { readLine: () => Effect.sync(() => answer) });

// Returns each answer in turn, then null (EOF / canceled) once exhausted —
// lets a test drive the local re-prompt loop deterministically without
// looping forever on a fixed invalid answer.
export const inputSequence = (answers: ReadonlyArray<string | null>): Layer.Layer<Input> => {
  let i = 0;
  return Layer.succeed(Input, {
    readLine: () => Effect.sync(() => (i < answers.length ? (answers[i++] ?? null) : null)),
  });
};

export const credsWith = (
  token: Option.Option<Redacted.Redacted<string>>,
  apiUrl: string | null = null,
): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => token),
    // snapshot mirrors the token so the replace-prompt path, which reads the
    // stored deployment alongside it, sees a coherent record.
    snapshot: Effect.succeed({ accessToken: token, savedAt: null, sessionId: null, apiUrl, credentialId: null }),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

export const configAt = (apiUrl: string): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl,
    dashboardUrl: "https://app.example",
    accessToken: Option.none(),
    jsonMode: false,
    noOpen: true,
    telemetryPosthogKey: "",
    telemetryPosthogHost: "",
  });

// Every request fails with the given status/code — enough to drive the
// terminal-state poll branch and the verify-retry-then-fail path without
// staging a real ECDH round trip (that lives in login.acceptance.spec.ts).
export const failingHttp = (status: number, code: string): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: <T>(_input: HttpRequestInput): Effect.Effect<T, NetworkError | ServerError> =>
      Effect.fail(
        new ServerError({ detail: "fixture", suggestion: "x", code, status, requestId: "req_fix" }),
      ),
  });

// Like failingHttp but walks a fixed list of failures across successive
// requests (clamping at the last) and counts calls — lets a test prove how
// many /verify round-trips a retry path actually made.
export const countingHttp = (
  steps: ReadonlyArray<{ readonly status: number; readonly code: string }>,
): { readonly layer: Layer.Layer<HttpClient>; readonly calls: () => number } => {
  let n = 0;
  const layer = Layer.succeed(HttpClient, {
    request: <T>(_input: HttpRequestInput): Effect.Effect<T, NetworkError | ServerError> => {
      const step = steps[Math.min(n, steps.length - 1)] ?? { status: 500, code: "UZ-FIXTURE-EMPTY" };
      n += 1;
      return Effect.fail(
        new ServerError({ detail: "fixture", suggestion: "x", code: step.code, status: step.status, requestId: "req_fix" }),
      );
    },
  });
  return { layer, calls: () => n };
};

export const failureValue = <T>(exit: Exit.Exit<T, CliError>): CliError | null =>
  Exit.isFailure(exit) ? Option.getOrNull(Cause.findErrorOption(exit.cause)) : null;
