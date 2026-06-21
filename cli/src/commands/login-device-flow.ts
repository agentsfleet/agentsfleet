// Device-flow helpers for the new login handshake. Pure-Effect wrappers
// around the cli-flow crypto primitives + the three CLI-facing endpoints:
// POST /v1/auth/sessions, GET /v1/auth/sessions/{id}, POST /v1/auth/
// sessions/{id}/verify. The dashboard side approves out-of-band via PATCH
// /approve which the CLI never touches.
//
// Server response shapes (confirmed against src/http/handlers/auth/
// sessions.zig + session_helpers.zig):
//
//   POST /v1/auth/sessions
//     201 { session_id, login_url, request_id }
//   GET  /v1/auth/sessions/{id}
//     200 { status: "pending"|"verification_pending",
//           cli_public_key, token_name, expires_at_ms }
//     404 | 410 — terminal states (consumed/expired/aborted)
//   POST /v1/auth/sessions/{id}/verify  { verification_code }
//     200 { dashboard_public_key, ciphertext, nonce }    (success | replay)
//     400 — UZ-AUTH-011 wrong code (retryable) | UZ-AUTH-018 bad shape (re-enter)
//     410 — aborted/consumed/expired
//
// Fingerprint is computed server-side from request_addr || user_agent ||
// session_id; the CLI never sends one.

import { Effect, Option } from "effect";
import {
  decryptJwt,
  deriveSharedKey,
  generateCliKeypair,
  type CliKeypair,
} from "../lib/cli-flow.ts";
import { AUTH_SESSIONS_PATH } from "../lib/api-paths.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Input } from "../services/input.ts";
import { Output } from "../services/output.ts";
import {
  DecryptError,
  InterruptedError,
  VerificationFailedError,
  type CliError,
  type NetworkError,
  type ServerError,
} from "../errors/index.ts";
import {
  defaultTokenName,
  type SessionCreatedResponse,
  type VerifySuccessResponse,
} from "./login-device-flow-types.ts";

// Wrong-code budget for the interactive verify prompt. Only a wrong code
// (UZ-AUTH-011 → VerificationFailedError) spends a strike; the 6-digit shape
// is validated client-side before submit, so a malformed code never reaches
// the server. Separate from the server's session-level MAX_VERIFY_ATTEMPTS (5).
const MAX_CLI_VERIFY_ATTEMPTS = 2;

// Server code for a malformed verification code (not 6 digits). Identifier
// matches ERR_INVALID_VERIFICATION_CODE in src/errors/error_registry.zig.
const ERR_INVALID_VERIFICATION_CODE = "UZ-AUTH-018";

// Re-export the device-flow wire shapes + platform default from their sibling
// module so existing consumers (login.ts, tests) keep importing them from here.
export type {
  SessionCreatedResponse,
  VerifySuccessResponse,
};
export { defaultTokenName };

const noInputAbort = (detail: string): InstanceType<typeof InterruptedError> =>
  new InterruptedError({
    detail,
    suggestion: "re-run interactively (without --no-input) or pass --force",
  });

// Reads stdin via Input.readLine and returns true on y/Y/yes/<empty>.
// The empty-string default biases toward "Yes" because the calling
// prompt (D20 replace-existing) treats continuing as the safe choice —
// the user has to type "n" to abort.
const promptYesNo = (
  question: string,
): Effect.Effect<boolean, never, Input | Output> =>
  Effect.gen(function* () {
    const input = yield* Input;
    const raw = yield* input.readLine(`${question} [Y/n] `);
    if (raw === null) return false; // EOF / canceled → don't proceed
    const trimmed = raw.trim().toLowerCase();
    return trimmed === "" || trimmed === "y" || trimmed === "yes";
  });

// D20 — abort if an existing credential is present and the operator
// hasn't passed --force. --no-input + no --force aborts loudly so
// scripts don't silently overwrite tokens. Interactive mode prompts.
export const idempotencyCheck = (
  opts: { readonly force: boolean; readonly noInput: boolean },
): Effect.Effect<void, CliError, Credentials | Input | Output> =>
  Effect.gen(function* () {
    if (opts.force) return;
    const credentials = yield* Credentials;
    const existing = yield* credentials.getAccessToken;
    if (Option.isNone(existing)) return;
    if (opts.noInput) {
      return yield* Effect.fail(
        noInputAbort("an existing credential is already saved"),
      );
    }
    const output = yield* Output;
    yield* output.warn(
      "an existing credential is already saved on this machine",
    );
    const proceed = yield* promptYesNo("Replace it?");
    if (!proceed) {
      return yield* Effect.fail(
        new InterruptedError({
          detail: "login aborted — existing credential kept",
          suggestion: "re-run with --force to overwrite without prompting",
        }),
      );
    }
  });

export const buildLoginUrl = (dashboardUrl: string, sessionId: string): string =>
  `${dashboardUrl.replace(/\/$/, "")}/cli-auth/${encodeURIComponent(sessionId)}`;

export const generateKeypair: Effect.Effect<CliKeypair> = Effect.promise(() => generateCliKeypair());

export const createSession = (
  publicKeyBase64Url: string,
  tokenName: string,
): Effect.Effect<SessionCreatedResponse, NetworkError | ServerError, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    return yield* http.request<SessionCreatedResponse>({
      path: AUTH_SESSIONS_PATH,
      method: HTTP_METHOD_POST,
      body: { public_key: publicKeyBase64Url, token_name: tokenName },
    });
  });

export const submitVerificationCode = (
  sessionId: string,
  verificationCode: string,
): Effect.Effect<VerifySuccessResponse, NetworkError | ServerError, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    return yield* http.request<VerifySuccessResponse>({
      path: `${AUTH_SESSIONS_PATH}/${encodeURIComponent(sessionId)}/verify`,
      method: HTTP_METHOD_POST,
      body: { verification_code: verificationCode },
    });
  });

// ECDH derive + HKDF + AES-GCM decrypt. Any throw → DecryptError —
// the channel is opaque on this side, no value in surfacing the raw
// WebCrypto failure to the operator.
export const decryptIssuedToken = (
  keypair: CliKeypair,
  response: VerifySuccessResponse,
): Effect.Effect<string, CliError> =>
  Effect.tryPromise({
    try: async () => {
      const key = await deriveSharedKey(keypair.privateKey, response.dashboard_public_key);
      return await decryptJwt(key, response.ciphertext, response.nonce);
    },
    catch: () =>
      new DecryptError({
        detail: "session integrity check failed",
        suggestion: "retry `agentsfleet login`",
      }),
  });

// Map a wrong-code 400 from /verify to a typed VerificationFailedError so
// callers can offer a retry without coupling to the transport-layer shape.
// /verify returns two distinct 400s: UZ-AUTH-011 (wrong HMAC) and
// UZ-AUTH-018 (bad shape — not 6 digits). Only the former is a "didn't
// match" worth a wrong-code strike; a shape error is a typo the caller
// re-enters, so it passes through for verifyAndDecryptWithRetry to handle.
export const mapVerifyFailure = (err: CliError): CliError => {
  if (err._tag !== "ServerError") return err;
  if (err.status !== 400) return err;
  if (err.code === ERR_INVALID_VERIFICATION_CODE) return err;
  return new VerificationFailedError({
    detail: "verification code didn't match",
    suggestion: "check the 6-digit code shown in your browser and try again",
    requestId: err.requestId,
  });
};

// Prompt for the 6-digit code, validating its shape client-side so an
// empty Enter or a non-6-digit typo re-prompts locally — no wasted /verify
// round-trip. A null read (EOF / Ctrl-D, or SIGINT via `signal`) is a clean
// cancel, not a re-prompt loop.
const VERIFICATION_CODE_RE = /^\d{6}$/;

const promptVerificationCode = (
  signal?: AbortSignal,
): Effect.Effect<string, InstanceType<typeof InterruptedError>, Input | Output> =>
  Effect.gen(function* () {
    const input = yield* Input;
    const output = yield* Output;
    yield* output.info("");
    let code = "";
    while (!VERIFICATION_CODE_RE.test(code)) {
      const raw = yield* input.readLine(
        "Enter the 6-digit verification code shown in your browser: ",
        signal,
      );
      if (raw === null) {
        return yield* Effect.fail(
          new InterruptedError({
            detail: "login canceled",
            suggestion: "re-run `agentsfleet login` and enter the code shown in your browser",
          }),
        );
      }
      code = raw.trim();
      if (!VERIFICATION_CODE_RE.test(code)) {
        yield* output.warn("that isn't a 6-digit code — enter the 6 digits shown in your browser");
      }
    }
    return code;
  });

const verifyAndDecrypt = (
  sessionId: string,
  keypair: CliKeypair,
  code: string,
): Effect.Effect<string, CliError, HttpClient> =>
  Effect.gen(function* () {
    const response = yield* submitVerificationCode(sessionId, code).pipe(
      Effect.mapError(mapVerifyFailure),
    );
    return yield* decryptIssuedToken(keypair, response);
  });

// Interactive code submission. The 6-digit shape is validated client-side
// (promptVerificationCode), so only a wrong code (UZ-AUTH-011 →
// VerificationFailedError) reaches here and spends one of
// MAX_CLI_VERIFY_ATTEMPTS strikes. DecryptError and terminal-state
// ServerErrors (410 expired/aborted) propagate without retry — protocol or
// session breakage, not a human typo. SIGINT/EOF at the prompt surfaces as
// InterruptedError. --no-input aborts before any prompt — the verification
// code is a human-typed per-flow secret with no non-interactive source.
export const verifyAndDecryptWithRetry = (
  sessionId: string,
  keypair: CliKeypair,
  opts: { readonly noInput: boolean; readonly signal?: AbortSignal } = { noInput: false },
): Effect.Effect<string, CliError, HttpClient | Input | Output> =>
  Effect.gen(function* () {
    if (opts.noInput) {
      return yield* Effect.fail(
        new InterruptedError({
          detail: "verification code required but --no-input prevents prompting",
          suggestion: "re-run interactively (without --no-input)",
        }),
      );
    }
    const output = yield* Output;
    const submit = (strikesLeft: number): Effect.Effect<string, CliError, HttpClient | Input | Output> =>
      Effect.gen(function* () {
      const code = yield* promptVerificationCode(opts.signal);
      const attempt = yield* verifyAndDecrypt(sessionId, keypair, code).pipe(
        Effect.map((token) => ({ kind: STATUS_OK, token })),
        Effect.catchTag("VerificationFailedError", (err) =>
          Effect.succeed({ kind: "wrong" as const, err }),
        ),
      );
      if (attempt.kind === STATUS_OK) return attempt.token;
      const remaining = strikesLeft - 1;
      if (remaining <= 0) return yield* Effect.fail(attempt.err);
      yield* output.warn("verification code didn't match — one more try");
        return yield* submit(remaining);
      });
    return yield* submit(MAX_CLI_VERIFY_ATTEMPTS);
  });
const HTTP_METHOD_POST = "POST" as const;
const STATUS_OK = "ok" as const;
