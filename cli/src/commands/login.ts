// Browser login delivers the encrypted session through a bound local callback.
// Explicit --no-open retains human code entry for a browser on another device.
// Both paths share verification, durable credential minting and identity checks.

import { Effect, Redacted } from "effect";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { TelemetryRuntime } from "../services/telemetry/runtime.service.ts";
import { Browser } from "../services/browser.service.ts";
import { CliConfig } from "../services/config.ts";
import { API_KEY_ENV } from "../constants/env.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Input } from "../services/input.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { Stdin } from "../services/stdin.ts";
import { Workspaces } from "../services/workspaces.ts";
import {
  AuthError,
  CLI_ERROR_TAG,
  InterruptedError,
  type CliError,
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
  withSigintAbort,
} from "./login-helpers.ts";
import {
  exchangeForCredential,
  type MintedCredential,
} from "./login-exchange.ts";
import { rollbackOnIdentityFailure } from "./login-identity-rollback.ts";
import { browserLogin } from "./login-browser-flow.ts";
import { MAX_LOGIN_WAIT_MS } from "./login-callback-protocol.ts";
import {
  readIdentity,
  type CallerIdentity,
} from "../lib/me-ping.ts";

export interface LoginFlags {
  readonly noOpen: boolean;
  readonly noInput: boolean;
  readonly force: boolean;
  readonly tokenName: string | undefined;
}

export interface LoginFlagsRaw {
  readonly noOpen: boolean | undefined;
  readonly noInput: boolean | undefined;
  readonly force: boolean | undefined;
  readonly tokenName: string | undefined;
}

const announceSession = Effect.fnUntraced(function* (
  sessionId: string,
  loginUrl: string,
) {
  const output = yield* Output;
  // The session block is scaffolding a person reads while waiting; a script
  // gets the whole outcome in one record when the flow completes.
  if (output.format !== OUTPUT_FORMAT.text) return;
  yield* output.printSection("Login session");
  yield* output.printKeyValue({ session_id: sessionId, login_url: loginUrl });
});

// Success rendering. Every non-success path is an Effect failure routed
// through the dispatcher's exit-code map, so this only handles "complete".
// The device flow is the only login path, so a session id always exists.
//
// The line NAMES the person, because the complaint this answers is that a
// terminal reported "login complete" and left the operator with no way to tell
// which account it had just signed into. The identity costs no extra request:
// the validation probe that has always run here now answers with it.
//
// Display name first, email second, and the bare word if a server answered
// neither — a rendering gap must never fail a login that completed.
const renderSuccess = Effect.fnUntraced(function* (
  sessionId: string,
  identity: CallerIdentity,
) {
  const config = yield* CliConfig;
  const output = yield* Output;
  const who = identity.displayName ?? identity.email;
  yield* output.success(
    who.length > 0
      ? `signed in as ${who} — ${identity.tenantName}`
      : LOGIN_COMPLETE,
    {
      status: "complete",
      session_id: sessionId,
      token_saved: true,
      api_url: config.apiUrl,
      email: identity.email,
      tenant_name: identity.tenantName,
    },
  );
});

const persistSuccess = Effect.fnUntraced(function* (
  sessionId: string,
  minted: MintedCredential,
) {
  const config = yield* CliConfig;
  const credentials = yield* Credentials;
  yield* credentials.saveAccessToken({
    token: minted.credential,
    sessionId,
    apiUrl: config.apiUrl,
    credentialId: minted.id,
  });
  return minted.credential;
});

// Only a verified, decrypted session reaches the shared persistence path.
const completeLogin = Effect.fnUntraced(function* (
  sessionId: string,
  token: string,
) {
  // The exchange completes before anything is persisted. A failure here
  // leaves credentials.json untouched and reports a registered code, so a
  // failed login never leaves a dead-on-arrival value on disk.
  const minted = yield* exchangeForCredential(Redacted.make(token));
  const redacted = yield* persistSuccess(sessionId, minted);
  // The one call that proves the credential authenticates, and the one that
  // knows whose it is. A refusal here still clears the file just written.
  const identity = yield* readIdentity(redacted).pipe(
    Effect.catch(rollbackOnIdentityFailure),
  );
  yield* hydrateWorkspacesAfterLogin(redacted);
  yield* captureLoginCompleted(token);
  return identity;
});

// Login surface rule: every failure exits 1. Transport / server
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
  // instead of prompting so scripts don't silently clobber a credential. A
  // non-TTY stdin carries no answer worth reading, so it is treated like
  // --no-input rather than consuming whatever was piped as a Y/n response.
  const noInput = flags.noInput || !stdin.isTTY;
  yield* idempotencyCheck({ force: flags.force, noInput });

  // A person must start login in a terminal and deliberately authorize it.
  if (!stdin.isTTY) {
    return yield* Effect.fail(
      new InterruptedError({
        detail:
          "`agentsfleet login` needs an interactive terminal and browser authorization",
        suggestion: `set ${API_KEY_ENV} to a tenant API key for unattended use`,
      }),
    );
  }

  const keypair = yield* generateKeypair;
  const tokenName = flags.tokenName ?? defaultTokenName();
  const deadlineMs = Date.now() + MAX_LOGIN_WAIT_MS;
  const created = yield* createSession(keypair.publicKeyBase64Url, tokenName);
  const serverLoginUrl = created.login_url?.trim();
  const loginUrl = serverLoginUrl ? serverLoginUrl : buildLoginUrl(config.dashboardUrl, created.session_id);

  const identity = yield* flags.noOpen || config.noOpen
    ? manualLogin(created.session_id, keypair, loginUrl, flags.noInput)
    : browserLogin({ sessionId: created.session_id, keypair, dashboardUrl: config.dashboardUrl, loginUrl, deadlineMs },
      (token) => completeLogin(created.session_id, token));
  yield* renderSuccess(created.session_id, identity);
});

const manualLogin = Effect.fnUntraced(function* (
  sessionId: string, keypair: import("../lib/cli-flow.ts").CliKeypair, loginUrl: string, noInput: boolean,
) {
  const output = yield* Output;
  yield* announceSession(sessionId, loginUrl);
  if (output.format === OUTPUT_FORMAT.text) yield* output.info(BROWSER_NOT_OPENED_MESSAGE);
  return yield* withSigintAbort((signal) => verifyAndDecryptWithRetry(sessionId, keypair, { noInput, signal }).pipe(
    Effect.flatMap((token) => completeLogin(sessionId, token)),
  ));
});

// Re-map transport errors during create-session so every login failure
// exits 1 (AuthError). VerificationFailed / DecryptError / SessionAborted /
// SessionConsumed are already AuthError-typed.
const remapTransportErrors = (err: CliError): CliError => {
  if (err._tag === CLI_ERROR_TAG.server) {
    return new AuthError({
      detail: err.detail,
      suggestion: "retry `agentsfleet login`",
      code: err.code,
      requestId: err.requestId,
    });
  }
  if (err._tag === CLI_ERROR_TAG.network) {
    return new AuthError({
      detail: err.detail,
      suggestion: "check network, then retry `agentsfleet login`",
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
  });
const BROWSER_NOT_OPENED_MESSAGE = "browser: not opened (open URL manually)" as const;
// The fallback line, for a server that answered an identity with neither a
// display name nor an email. It reports what happened and claims no more.
const LOGIN_COMPLETE = "login complete" as const;
