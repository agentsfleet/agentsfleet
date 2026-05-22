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

import { Effect, Option, Redacted } from "effect";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { TelemetryRuntime } from "../services/telemetry/runtime.service.ts";
import { Browser } from "../services/browser.service.ts";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Input } from "../services/input.ts";
import { Output } from "../services/output.ts";
import { Stdin } from "../services/stdin.ts";
import { Workspaces } from "../services/workspaces.ts";
import {
  AuthError,
  type CliError,
  type MeValidationError,
} from "../errors/index.ts";
import {
  buildLoginUrl,
  createSession,
  defaultTokenName,
  generateKeypair,
  idempotencyCheck,
  verifyAndDecryptWithRetry,
} from "./login-device-flow.ts";
import {
  captureLoginCompleted,
  hydrateWorkspacesAfterLogin,
  resolveDirectToken,
  saveDirectToken,
  withSigintAbort,
} from "./login-helpers.ts";
import { pingMe } from "../lib/me-ping.ts";

export interface LoginFlags {
  readonly noOpen: boolean;
  readonly noInput: boolean;
  readonly force: boolean;
  readonly tokenName: string | undefined;
  // --token <pat>; non-interactive direct-token source (highest priority).
  readonly tokenFlag: string | undefined;
  // Raw ZOMBIE_TOKEN env value (not the file-merged CliConfig.accessToken)
  // so an existing credentials.json is never mistaken for a direct token.
  readonly envToken: string | undefined;
}

export interface LoginFlagsRaw {
  readonly noOpen: boolean | undefined;
  readonly noInput: boolean | undefined;
  readonly force: boolean | undefined;
  readonly tokenName: string | undefined;
  readonly tokenFlag: string | undefined;
  readonly envToken: string | undefined;
}

const announceSession = Effect.fnUntraced(function* (
  sessionId: string,
  loginUrl: string,
) {
  const config = yield* CliConfig;
  if (config.jsonMode) return;
  const output = yield* Output;
  yield* output.printSection("Login session");
  yield* output.printKeyValue({ session_id: sessionId, login_url: loginUrl });
});

const maybeOpenBrowser = Effect.fnUntraced(function* (
  loginUrl: string,
  noOpen: boolean,
) {
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

// Success rendering for both paths (direct token + device flow). Every
// non-success path is an Effect failure routed through the dispatcher's
// exit-code map, so this only handles "complete". `sessionId` is null for
// the direct-token path (no device-flow session to report).
const renderSuccess = Effect.fnUntraced(function* (sessionId: string | null) {
  const config = yield* CliConfig;
  const output = yield* Output;
  if (config.jsonMode) {
    yield* output.printJson({
      status: "complete",
      session_id: sessionId ?? "",
      token_saved: true,
      api_url: config.apiUrl,
    });
  } else {
    yield* output.success("login complete");
  }
});

const persistSuccess = Effect.fnUntraced(function* (
  sessionId: string,
  token: string,
) {
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

// /me ping failure → wipe credentials.json before propagating the error.
// The token was persisted moments ago but failed validation; leaving it
// on disk would route subsequent commands to the same dead-on-arrival
// token. Swallow the clear's own UnexpectedError — the validation
// failure is the load-bearing signal the operator needs to see.
const rollbackOnMeFailure = Effect.fnUntraced(function* (err: MeValidationError) {
  const credentials = yield* Credentials;
  yield* credentials.clearAccessToken.pipe(Effect.ignore);
  return yield* Effect.fail(err);
});

// Verify branch: prompt → /verify → decrypt → persist → /me ping → hydrate
// → telemetry. SIGINT/EOF at the prompt aborts via `signal` before any
// persist. Lives outside loginCore so the orchestrator stays linear (no
// nested generators) under the 350-line cap.
const completeVerificationBranch = Effect.fnUntraced(function* (
  sessionId: string,
  keypair: import("../lib/cli-flow.ts").CliKeypair,
  noInput: boolean,
  signal: AbortSignal,
) {
  const token = yield* verifyAndDecryptWithRetry(sessionId, keypair, { noInput, signal });
  const redacted = yield* persistSuccess(sessionId, token);
  yield* pingMe(redacted).pipe(Effect.catchTag("MeValidationError", rollbackOnMeFailure));
  yield* hydrateWorkspacesAfterLogin(redacted);
  yield* captureLoginCompleted(sessionId, token, "browser");
});

// Login surface contract: every failure exits 1. Transport / server
// errors during create-session get re-wrapped as AuthError so the
// dispatcher's exit-code map still routes them as AuthError(1) rather than
// ServerError(3) / NetworkError(2). VerificationFailed, DecryptError, and
// the rate-limit-exceeded SessionAborted all come through already-typed as
// AuthError from the device-flow helpers.
const loginCore = Effect.fnUntraced(function* (flags: LoginFlags) {
  const config = yield* CliConfig;
  const stdin = yield* Stdin;

  // Pre-flight (D20). idempotencyCheck refuses to overwrite an existing
  // credential without --force or a Y/yes prompt; --no-input aborts loudly
  // instead of prompting so scripts don't silently clobber a token. A
  // non-TTY stdin is a pipe carrying a token (resolveDirectToken's
  // lowest-priority source), never a Y/n answer — treat it like --no-input
  // so the piped token is never consumed as the replace-prompt response.
  const noInput = flags.noInput || !stdin.isTTY;
  yield* idempotencyCheck({ force: flags.force, noInput });

  // Non-interactive resolve (--token > ZOMBIE_TOKEN env > piped stdin)
  // ahead of the browser device flow. A directly-supplied token is
  // validated + persisted with no browser; `none` falls through to the
  // device flow below.
  const direct = yield* resolveDirectToken({
    tokenFlag: flags.tokenFlag,
    envToken: flags.envToken,
  });
  if (Option.isSome(direct)) {
    if (flags.tokenName !== undefined && !config.jsonMode) {
      const output = yield* Output;
      yield* output.info(
        "--token-name is ignored with a direct token (no browser session to label)",
      );
    }
    yield* saveDirectToken(direct.value);
    yield* renderSuccess(null);
    return;
  }

  const keypair = yield* generateKeypair;
  const tokenName = flags.tokenName ?? defaultTokenName();
  const created = yield* createSession(keypair.publicKeyBase64Url, tokenName);
  const loginUrl = buildLoginUrl(config.dashboardUrl, created.session_id);

  yield* announceSession(created.session_id, loginUrl);
  yield* maybeOpenBrowser(loginUrl, flags.noOpen);

  // Prompt for the code immediately — possessing it implies the dashboard
  // approved, so there's no poll-gate to wait through. SIGINT/EOF at the
  // prompt aborts cleanly (exit 130) with nothing persisted.
  yield* withSigintAbort((signal) =>
    completeVerificationBranch(created.session_id, keypair, flags.noInput, signal),
  );
  yield* renderSuccess(created.session_id);
});

// Re-map transport errors during create-session so every login failure
// exits 1 (AuthError). VerificationFailed / DecryptError / SessionAborted /
// SessionConsumed are already AuthError-typed.
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
  | Stdin
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
    noOpen: raw.noOpen ?? false,
    noInput: raw.noInput ?? false,
    force: raw.force ?? false,
    tokenName: raw.tokenName,
    tokenFlag: raw.tokenFlag,
    envToken: raw.envToken,
  });
