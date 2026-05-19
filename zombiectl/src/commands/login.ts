// Login handler — Effect-shaped device flow. Replaces the legacy
// plaintext-token poll-and-grab with the ECDH-encrypted handshake:
//
//   1. CLI generates an ephemeral P-256 keypair.
//   2. POST /v1/auth/sessions { public_key, token_name } → session_id.
//   3. CLI prints + opens https://app.usezombie.com/cli-auth/{session_id}.
//   4. Dashboard mints the JWT, AES-GCM-encrypts it to the CLI's public
//      key, PATCHes the ciphertext + nonce + dashboard_public_key + a
//      6-digit verification code to /v1/auth/sessions/{id}/approve.
//   5. CLI polls GET /v1/auth/sessions/{id} until status is
//      verification_pending; prompts the operator for the displayed
//      code; POSTs to /verify; receives the ciphertext; decrypts.
//   6. CLI persists the recovered JWT to credentials.json, then hydrates
//      workspaces + captures the login-completed analytics event.
//
// SIGINT during the poll or prompt aborts cleanly via the existing
// AbortController pattern. Failures route through one AuthError taxonomy
// on the error channel; the dispatcher's exit-code map keys all of them
// to 1 (130 for interrupted).

import { Effect, Redacted } from "effect";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { TelemetryRuntime } from "../services/telemetry/runtime.service.ts";
import { Browser } from "../services/browser.service.ts";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Input } from "../services/input.ts";
import { Output } from "../services/output.ts";
import { Spinner } from "../services/spinner.ts";
import { Workspaces } from "../services/workspaces.ts";
import {
  AuthError,
  ExpiredSessionError,
  InterruptedError,
  TimeoutError,
  type CliError,
} from "../errors/index.ts";
import {
  buildLoginUrl,
  createSession,
  defaultTokenName,
  envTokenAwareness,
  generateKeypair,
  idempotencyCheck,
  pollUntilVerificationPending,
  verifyAndDecryptWithRetry,
} from "./login-device-flow.ts";
import {
  captureLoginCompleted,
  hydrateWorkspacesAfterLogin,
  startSpinner,
  withSigintAbort,
} from "./login-helpers.ts";

const DEFAULT_TIMEOUT_SEC = 300;
const DEFAULT_POLL_MS = 2000;

type FinalOutcome =
  | { readonly status: "complete"; readonly token: string }
  | { readonly status: "expired" }
  | { readonly status: "interrupted" }
  | { readonly status: "timeout" };

export interface LoginFlags {
  readonly timeoutSec: number;
  readonly pollMs: number;
  readonly noOpen: boolean;
  readonly noInput: boolean;
  readonly force: boolean;
  readonly tokenName: string | undefined;
}

export interface LoginFlagsRaw {
  readonly timeoutSec: number | undefined;
  readonly pollMs: number | undefined;
  readonly noOpen: boolean | undefined;
  readonly noInput: boolean | undefined;
  readonly force: boolean | undefined;
  readonly tokenName: string | undefined;
}

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

const renderOutcome = (
  outcome: FinalOutcome,
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
    const detailMap: Record<Exclude<FinalOutcome["status"], "complete">, string> = {
      expired: "login session expired",
      interrupted: "login interrupted",
      timeout: "login timed out",
    };
    const detail = detailMap[outcome.status];
    if (config.jsonMode) {
      yield* output.printJson({ status: outcome.status, session_id: sessionId });
    } else {
      yield* output.error(detail);
    }
    return outcome.status === "interrupted" ? 130 : 1;
  });

const persistSuccess = (
  sessionId: string,
  token: string,
): Effect.Effect<Redacted.Redacted<string>, CliError, CliConfig | Credentials> =>
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

const failedOutcomeError = (
  outcome: FinalOutcome,
): ExpiredSessionError | InterruptedError | TimeoutError => {
  if (outcome.status === "expired") {
    return new ExpiredSessionError({
      detail: "login session expired",
      suggestion: "retry `zombiectl login`",
    });
  }
  if (outcome.status === "interrupted") {
    return new InterruptedError({
      detail: "login interrupted",
      suggestion: "retry `zombiectl login`",
    });
  }
  return new TimeoutError({
    detail: "login timed out",
    suggestion: "retry `zombiectl login`",
  });
};

// Verification-pending branch: prompt → /verify → decrypt → persist →
// hydrate → telemetry. Lives outside loginCore so the orchestrator stays
// linear (no nested generators) under the 350-line cap.
const completeVerificationBranch = (
  sessionId: string,
  keypair: import("../lib/cli-flow.ts").CliKeypair,
  noInput: boolean,
): Effect.Effect<
  FinalOutcome,
  CliError,
  Analytics | CliConfig | Credentials | HttpClient | Input | Output | TelemetryRuntime | Workspaces
> =>
  Effect.gen(function* () {
    const token = yield* verifyAndDecryptWithRetry(sessionId, keypair, { noInput });
    const redacted = yield* persistSuccess(sessionId, token);
    yield* hydrateWorkspacesAfterLogin(redacted);
    yield* captureLoginCompleted(sessionId, token);
    return { status: "complete", token } as FinalOutcome;
  });

// Login surface contract: every failure exits 1. Transport / server
// errors during create-session or poll get re-wrapped as AuthError so
// the dispatcher's exit-code map still routes them as AuthError(1)
// rather than ServerError(3) / NetworkError(2). VerificationFailed,
// DecryptError, and the rate-limit-exceeded SessionAborted all come
// through already-typed as AuthError from the device-flow helpers.
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
  | Input
  | Output
  | Spinner
  | TelemetryRuntime
  | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;

    // Pre-flight (D20 + D26b). idempotencyCheck refuses to overwrite an
    // existing credential without --force or a Y/yes prompt; envTokenAwareness
    // surfaces the precedence gotcha when ZMB_TOKEN/ZOMBIE_TOKEN is set.
    // Both honor --no-input by aborting with NoInputAbort instead of prompting.
    const preflightGuards = { force: flags.force, noInput: flags.noInput };
    yield* idempotencyCheck(preflightGuards);
    yield* envTokenAwareness(preflightGuards);

    const keypair = yield* generateKeypair;
    const tokenName = flags.tokenName ?? defaultTokenName();
    const created = yield* createSession(keypair.publicKeyBase64Url, tokenName);
    const loginUrl = buildLoginUrl(config.dashboardUrl, created.session_id);

    yield* announceSession(created.session_id, loginUrl);
    yield* maybeOpenBrowser(loginUrl, flags.noOpen);

    const handles = yield* startSpinner("waiting for browser approval");
    const deadline = Date.now() + Math.max(1, flags.timeoutSec) * 1000;
    const pollOutcome = yield* withSigintAbort((signal) =>
      pollUntilVerificationPending(
        created.session_id,
        { deadline, pollMs: flags.pollMs },
        signal,
      ),
    ).pipe(
      Effect.tap((res) =>
        res.status === "verification_pending" ? handles.succeed : handles.fail,
      ),
      Effect.tapError(() => handles.fail),
    );

    const final: FinalOutcome =
      pollOutcome.status === "verification_pending"
        ? yield* completeVerificationBranch(created.session_id, keypair, flags.noInput)
        : pollOutcome;

    const exitCode = yield* renderOutcome(final, created.session_id);
    if (exitCode !== 0) return yield* Effect.fail(failedOutcomeError(final));
  });

// Re-map transport errors during create-session / poll so every login
// failure exits 1 (AuthError). VerificationFailed / DecryptError /
// SessionAborted / SessionConsumed are already AuthError-typed.
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

type MainLayerCarry =
  | Analytics
  | Browser
  | CliConfig
  | Credentials
  | HttpClient
  | Input
  | Output
  | Spinner
  | TelemetryRuntime
  | Workspaces;

export const loginEffect = (
  flags: LoginFlags,
): Effect.Effect<void, CliError, MainLayerCarry> =>
  loginCore(flags).pipe(Effect.mapError(remapTransportErrors));

export const loginEffectFromFlags = (
  raw: LoginFlagsRaw,
): ReturnType<typeof loginEffect> =>
  loginEffect({
    timeoutSec: raw.timeoutSec ?? DEFAULT_TIMEOUT_SEC,
    pollMs: raw.pollMs ?? DEFAULT_POLL_MS,
    noOpen: raw.noOpen ?? false,
    noInput: raw.noInput ?? false,
    force: raw.force ?? false,
    tokenName: raw.tokenName,
  });
