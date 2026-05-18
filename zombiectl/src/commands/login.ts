// Login handler — Effect-shaped. Replaces the pre-Effect commandLogin
// from core.ts. Flow:
//   1. POST /v1/auth/sessions      → session_id + login_url
//   2. printSection / key-value     announce so the operator sees the URL
//   3. Browser service opens the URL (unless --no-open / ctx.noOpen)
//   4. Spinner + poll loop          until status=complete | expired | timeout
//   5. SIGINT closes the AbortController → outcome.status="interrupted"
//   6. On complete: save credentials, hydrate workspaces, capture analytics
//   7. Render outcome → exit 0 on success, AuthError otherwise
//
// All side effects (process listener, spinner timer, stdout writes) sit
// behind services so the handler stays referentially transparent under
// `Effect.runPromise`. Helpers for hydration / sigint / spinner live in
// login-helpers.ts to keep this file under the 350-line cap.

import { Effect, Redacted } from "effect";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { Browser } from "../services/browser.ts";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Spinner } from "../services/spinner.ts";
import { Workspaces } from "../services/workspaces.ts";
import { AUTH_SESSIONS_PATH } from "../lib/api-paths.ts";
import {
  AuthError,
  NetworkError,
  ServerError,
  UnexpectedError,
  type CliError,
} from "../errors/index.ts";
import {
  EVT_LOGIN_COMPLETED,
  EVT_USER_AUTHENTICATED,
} from "../constants/analytics-events.ts";
import { extractDistinctIdFromToken } from "../program/auth-token.ts";
import {
  hydrateWorkspacesAfterLogin,
  startSpinner,
  withSigintAbort,
} from "./login-helpers.ts";

const DEFAULT_TIMEOUT_SEC = 300;
const DEFAULT_POLL_MS = 2000;
const MIN_POLL_MS = 500;

interface AuthSessionCreate {
  readonly session_id: string;
  readonly login_url: string;
}

interface AuthSessionStatus {
  readonly status: string;
  readonly token?: string;
}

type LoginOutcome =
  | { readonly status: "complete"; readonly token: string }
  | { readonly status: "expired" }
  | { readonly status: "interrupted" }
  | { readonly status: "timeout" };

export interface LoginFlags {
  readonly timeoutSec: number;
  readonly pollMs: number;
  readonly noOpen: boolean;
}

const createLoginSession: Effect.Effect<
  AuthSessionCreate,
  NetworkError | ServerError,
  HttpClient
> = Effect.gen(function* () {
  const http = yield* HttpClient;
  return yield* http.request<AuthSessionCreate>({
    path: AUTH_SESSIONS_PATH,
    method: "POST",
    body: {},
  });
});

const pollSessionOnce = (
  sessionId: string,
): Effect.Effect<AuthSessionStatus, NetworkError | ServerError, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    return yield* http.request<AuthSessionStatus>({
      path: `${AUTH_SESSIONS_PATH}/${encodeURIComponent(sessionId)}`,
    });
  });

const announceSession = (
  sessionId: string,
  loginUrl: string,
): Effect.Effect<void, never, CliConfig | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    if (config.jsonMode) return;
    const output = yield* Output;
    yield* output.printSection("Login session");
    yield* output.printKeyValue({ session_id: sessionId, login_url: loginUrl });
  });

const maybeOpenBrowser = (
  loginUrl: string,
  noOpen: boolean,
): Effect.Effect<boolean, never, Browser | CliConfig | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    if (noOpen || config.noOpen) {
      if (!config.jsonMode) {
        const output = yield* Output;
        yield* output.info("browser: not opened (open URL manually)");
      }
      return false;
    }
    const browser = yield* Browser;
    const opened = yield* browser.open(loginUrl);
    if (!config.jsonMode) {
      const output = yield* Output;
      yield* output.info(
        opened ? "browser: opened" : "browser: not opened (open URL manually)",
      );
    }
    return opened;
  });

const pollUntilComplete = (
  sessionId: string,
  flags: { readonly deadline: number; readonly pollMs: number },
  abort: AbortSignal,
): Effect.Effect<LoginOutcome, NetworkError | ServerError, HttpClient> =>
  Effect.gen(function* () {
    const pollIntervalMs = Math.max(MIN_POLL_MS, flags.pollMs);
    while (Date.now() < flags.deadline) {
      if (abort.aborted) return { status: "interrupted" } as LoginOutcome;
      const latest = yield* pollSessionOnce(sessionId);
      if (latest.status === "complete" && typeof latest.token === "string") {
        if (abort.aborted) return { status: "interrupted" } as LoginOutcome;
        return { status: "complete", token: latest.token } as LoginOutcome;
      }
      if (latest.status === "expired") return { status: "expired" } as LoginOutcome;
      yield* Effect.sleep(`${pollIntervalMs} millis`);
    }
    return (abort.aborted ? { status: "interrupted" } : { status: "timeout" }) as LoginOutcome;
  });

const renderOutcome = (
  outcome: LoginOutcome,
  sessionId: string,
): Effect.Effect<number, never, CliConfig | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    if (outcome.status === "complete") {
      if (config.jsonMode) {
        yield* output.printJson({
          status: "complete",
          session_id: sessionId,
          token_saved: true,
          api_url: config.apiUrl,
        });
      } else {
        yield* output.success("login complete");
      }
      return 0;
    }
    if (outcome.status === "expired") {
      if (config.jsonMode) {
        yield* output.printJson({ status: "expired", session_id: sessionId });
      } else {
        yield* output.error("login session expired");
      }
      return 1;
    }
    if (outcome.status === "interrupted") {
      if (config.jsonMode) {
        yield* output.printJson({ status: "interrupted", session_id: sessionId });
      } else {
        yield* output.error("login interrupted");
      }
      return 130;
    }
    if (config.jsonMode) {
      yield* output.printJson({ status: "timeout", session_id: sessionId });
    } else {
      yield* output.error("login timed out");
    }
    return 1;
  });

const persistSuccess = (
  sessionId: string,
  token: string,
): Effect.Effect<
  Redacted.Redacted<string>,
  UnexpectedError,
  CliConfig | Credentials
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const credentials = yield* Credentials;
    const redacted = Redacted.make(token);
    yield* credentials.saveAccessToken({
      token: redacted,
      sessionId,
      apiUrl: config.apiUrl,
    });
    return redacted;
  });

// Identify under the post-login distinct id so subsequent emits in
// the same fiber attribute correctly, then fire user_authenticated +
// login_completed in-band. The Analytics service auto-merges
// cli_session_id / cli_device_id via TelemetryRuntime; the layer
// short-circuits these emits when DISABLE_TELEMETRY isn't opted in
// (the default).
const captureLoginCompleted = (
  sessionId: string,
  token: string,
): Effect.Effect<void, never, Analytics> =>
  Effect.gen(function* () {
    const analytics = yield* Analytics;
    const distinctId = extractDistinctIdFromToken(token);
    if (distinctId) yield* analytics.identify(distinctId);
    yield* analytics.capture(EVT_USER_AUTHENTICATED, { command: "login" });
    yield* analytics.capture(EVT_LOGIN_COMPLETED, { session_id: sessionId });
  });

const failedOutcomeError = (outcome: LoginOutcome): AuthError => {
  const detail =
    outcome.status === "expired"
      ? "login session expired"
      : outcome.status === "interrupted"
        ? "login interrupted"
        : outcome.status === "timeout"
          ? "login timed out"
          : "login failed";
  return new AuthError({
    detail,
    suggestion: "retry `zombiectl login`",
    code: outcome.status.toUpperCase(),
  });
};

// Login surface contract: every failure exits 1. The ServerError /
// NetworkError exit codes (3 / 2) used elsewhere don't apply during
// the OAuth handshake because the operator can't differentiate
// "auth-service down" from "auth flow expired" — both are "try login
// again". Network/server errors are re-rendered as AuthError before
// they reach the dispatcher.
const loginCore = (
  flags: LoginFlags,
): Effect.Effect<
  void,
  CliError,
  | Analytics
  | Browser
  | CliConfig
  | Credentials
  | HttpClient
  | Output
  | Spinner
  | Workspaces
> =>
  Effect.gen(function* () {
    const { session_id: sessionId, login_url: loginUrl } = yield* createLoginSession;
    yield* announceSession(sessionId, loginUrl);
    yield* maybeOpenBrowser(loginUrl, flags.noOpen);

    const handles = yield* startSpinner("waiting for browser login");
    const deadline = Date.now() + Math.max(1, flags.timeoutSec) * 1000;
    const outcome = yield* withSigintAbort((signal) =>
      pollUntilComplete(sessionId, { deadline, pollMs: flags.pollMs }, signal),
    ).pipe(
      Effect.tap((res) =>
        res.status === "complete" ? handles.succeed : handles.fail,
      ),
      Effect.tapError(() => handles.fail),
    );

    if (outcome.status === "complete") {
      const token = yield* persistSuccess(sessionId, outcome.token);
      yield* hydrateWorkspacesAfterLogin(token);
      yield* captureLoginCompleted(sessionId, outcome.token);
    }

    const exitCode = yield* renderOutcome(outcome, sessionId);
    if (exitCode !== 0) return yield* Effect.fail(failedOutcomeError(outcome));
  });

// Login is a transient flow — every transport hiccup is "retry the
// login command". Re-wrap ServerError + NetworkError as AuthError so
// every failure exits 1 (EXIT_CODE.AuthError). The optional requestId
// rides along so the dispatcher's renderError still prints
// `request_id:` for support workflows.
const remapTransportErrors = (err: CliError): CliError => {
  if (err._tag === "ServerError") {
    return new AuthError({
      detail: err.detail,
      suggestion: "retry `zombiectl login`",
      code: err.code,
      requestId: err.requestId,
    });
  }
  if (err._tag === "NetworkError") {
    return new AuthError({
      detail: err.detail,
      suggestion: "check network, then retry `zombiectl login`",
      code: "NETWORK_UNREACHABLE",
    });
  }
  return err;
};

export const loginEffect = (
  flags: LoginFlags,
): Effect.Effect<
  void,
  CliError,
  | Analytics
  | Browser
  | CliConfig
  | Credentials
  | HttpClient
  | Output
  | Spinner
  | Workspaces
> =>
  loginCore(flags).pipe(Effect.mapError(remapTransportErrors));

export const loginEffectFromFlags = (
  rawTimeoutSec: number | undefined,
  rawPollMs: number | undefined,
  rawNoOpen: boolean | undefined,
): ReturnType<typeof loginEffect> =>
  loginEffect({
    timeoutSec: rawTimeoutSec ?? DEFAULT_TIMEOUT_SEC,
    pollMs: rawPollMs ?? DEFAULT_POLL_MS,
    noOpen: rawNoOpen ?? false,
  });
