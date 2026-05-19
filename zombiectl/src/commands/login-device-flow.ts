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
//     201 { session_id, request_id }
//   GET  /v1/auth/sessions/{id}
//     200 { status: "pending"|"verification_pending",
//           cli_public_key, token_name, expires_at_ms }
//     404 | 410 — terminal states (consumed/expired/aborted)
//   POST /v1/auth/sessions/{id}/verify  { verification_code }
//     200 { dashboard_public_key, ciphertext, nonce }    (success | replay)
//     400 — wrong code (ServerError.code === "UZ-AUTH-...")
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
import { CliConfig } from "../services/config.ts";
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

const MIN_POLL_MS = 500;

export type PollOutcome =
  | { readonly status: "verification_pending" }
  | { readonly status: "expired" }
  | { readonly status: "interrupted" }
  | { readonly status: "timeout" };

export interface SessionCreatedResponse {
  readonly session_id: string;
  readonly request_id?: string;
}

export interface SessionStatusResponse {
  readonly status: "pending" | "verification_pending";
  readonly cli_public_key: string;
  readonly token_name: string;
  readonly expires_at_ms: number;
}

export interface VerifySuccessResponse {
  readonly dashboard_public_key: string;
  readonly ciphertext: string;
  readonly nonce: string;
}

// Spec-pinned platform default (no hostname leak). Falls back to a
// generic label for unknown platforms.
export const defaultTokenName = (
  platform: NodeJS.Platform = process.platform,
): string => {
  if (platform === "darwin") return "macos-cli";
  if (platform === "linux") return "linux-cli";
  if (platform === "win32") return "windows-cli";
  if (platform === "freebsd") return "freebsd-cli";
  return "cli";
};

const ZMB_TOKEN_ENV_KEYS = ["ZMB_TOKEN", "ZOMBIE_TOKEN"] as const;

const noInputAbort = (detail: string): InterruptedError =>
  new InterruptedError({
    detail,
    suggestion: "re-run interactively (without --no-input) or pass --force",
  });

// Reads stdin via Input.readLine and returns true on y/Y/yes/<empty>.
// The empty-string default biases toward "Yes" because the calling
// prompts (D20 replace-existing, D26b env-var notice) treat continuing
// as the safe choice — the user has to type "n" to abort.
const promptYesNo = (
  question: string,
): Effect.Effect<boolean, never, Input | Output> =>
  Effect.gen(function* () {
    const input = yield* Input;
    const raw = yield* input.readLine(`${question} [Y/n] `);
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

// D26b — surface a notice when ZMB_TOKEN / ZOMBIE_TOKEN is set in the
// environment. The login flow only writes credentials.json; env-var
// tokens are out-of-band and take precedence on interactive shells, so
// the operator may run `zombiectl login` expecting it to "fix"
// authentication and be confused when the env-var token keeps winning.
// Read process.env directly — CliConfig.accessToken is the *resolved*
// token (could be from creds.json or env), we want to know specifically
// whether the env variant was set.
const envTokenKeysSet = (): readonly string[] =>
  ZMB_TOKEN_ENV_KEYS.filter((k) => typeof process.env[k] === "string" && process.env[k] !== "");

export const envTokenAwareness = (
  opts: { readonly force: boolean; readonly noInput: boolean },
  envKeysSetFn: () => readonly string[] = envTokenKeysSet,
): Effect.Effect<void, CliError, CliConfig | Input | Output> =>
  Effect.gen(function* () {
    const keysSet = envKeysSetFn();
    if (keysSet.length === 0) return;
    const config = yield* CliConfig;
    if (config.jsonMode) return;
    const output = yield* Output;
    const list = keysSet.join(" / ");
    yield* output.warn(
      `${list} is set in your environment — on interactive shells it takes precedence over credentials.json.\n  ` +
        "`zombiectl login` only replaces credentials.json; your env-var token is unaffected.",
    );
    if (opts.force) return;
    if (opts.noInput) {
      return yield* Effect.fail(
        noInputAbort(`${list} is set; login would not change which token wins on this shell`),
      );
    }
    const proceed = yield* promptYesNo("Continue with login anyway?");
    if (!proceed) {
      return yield* Effect.fail(
        new InterruptedError({
          detail: "login aborted — env-var token left in place",
          suggestion: `unset ${list} or re-run with --force`,
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
      method: "POST",
      body: { public_key: publicKeyBase64Url, token_name: tokenName },
    });
  });

export const pollSessionStatus = (
  sessionId: string,
): Effect.Effect<SessionStatusResponse, NetworkError | ServerError, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    return yield* http.request<SessionStatusResponse>({
      path: `${AUTH_SESSIONS_PATH}/${encodeURIComponent(sessionId)}`,
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
      method: "POST",
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
        suggestion: "retry `zombiectl login`",
      }),
  });

// Map a wrong-code 400 from /verify to a typed VerificationFailedError
// so callers can detect it and offer one retry without coupling to the
// transport-layer error shape.
export const mapVerifyFailure = (err: CliError): CliError => {
  if (err._tag !== "ServerError") return err;
  if (err.status !== 400) return err;
  return new VerificationFailedError({
    detail: "verification code didn't match",
    suggestion: "check the 6-digit code shown in your browser and try again",
    requestId: err.requestId,
  });
};

// Poll loop. Returns when status flips to verification_pending, when the
// server-side TTL expires, or when the local deadline is reached. SIGINT
// surfaces as `interrupted`.
export const pollUntilVerificationPending = (
  sessionId: string,
  flags: { readonly deadline: number; readonly pollMs: number },
  abort: AbortSignal,
): Effect.Effect<PollOutcome, CliError, HttpClient> =>
  Effect.gen(function* () {
    const pollIntervalMs = Math.max(MIN_POLL_MS, flags.pollMs);
    while (Date.now() < flags.deadline) {
      if (abort.aborted) return { status: "interrupted" } as PollOutcome;
      const latest = yield* pollSessionStatus(sessionId).pipe(
        Effect.map((r) => ({ kind: "ok" as const, value: r })),
        Effect.catchTag("ServerError", (err) => {
          if (err.code === "UZ-AUTH-EXPIRED" || err.status === 410) {
            return Effect.succeed({ kind: "expired" as const });
          }
          return Effect.fail(err);
        }),
      );
      if (latest.kind === "expired") return { status: "expired" } as PollOutcome;
      if (latest.value.status === "verification_pending") {
        return { status: "verification_pending" } as PollOutcome;
      }
      yield* Effect.sleep(`${pollIntervalMs} millis`);
    }
    return (abort.aborted ? { status: "interrupted" } : { status: "timeout" }) as PollOutcome;
  });

const promptVerificationCode: Effect.Effect<string, never, Input | Output> =
  Effect.gen(function* () {
    const input = yield* Input;
    const output = yield* Output;
    yield* output.info("");
    const raw = yield* input.readLine("Enter the 6-digit verification code shown in your browser: ");
    return raw.trim();
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

// Interactive code submission. On a wrong-code first attempt the operator
// gets one retry; a second failure surfaces VerificationFailedError.
// DecryptError and terminal-state ServerErrors propagate without retry —
// those signal protocol or session-level breakage, not human typo.
// --no-input aborts before any prompt — the verification code is a
// human-readable per-flow secret typed into the terminal; non-interactive
// shells have no way to supply it.
export const verifyAndDecryptWithRetry = (
  sessionId: string,
  keypair: CliKeypair,
  opts: { readonly noInput: boolean } = { noInput: false },
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
    const firstCode = yield* promptVerificationCode;
    const firstAttempt = yield* verifyAndDecrypt(sessionId, keypair, firstCode).pipe(
      Effect.map((token) => ({ ok: true as const, token })),
      Effect.catchTag("VerificationFailedError", (err) =>
        Effect.succeed({ ok: false as const, err }),
      ),
    );
    if (firstAttempt.ok) return firstAttempt.token;
    const output = yield* Output;
    yield* output.warn("verification code didn't match — one more try");
    const secondCode = yield* promptVerificationCode;
    return yield* verifyAndDecrypt(sessionId, keypair, secondCode);
  });
