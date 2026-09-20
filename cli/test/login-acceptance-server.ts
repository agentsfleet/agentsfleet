// The device-flow server the login acceptance suite talks to.
//
// Its own file because it IS the far side of the round trip: every path the
// login flow calls, answered the way the daemon answers it, including the
// identity read that reports who was signed in. Split from the client-side
// layers under the file-length cap — the seam is which end of the wire each
// half stands on.

import { Effect, Layer, Redacted } from "effect";
import { webcrypto } from "node:crypto";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import {
  deriveSharedKey,
  encryptJwtForTest,
  type EncryptedJwt,
} from "../src/lib/cli-flow.ts";
import { CLI_CREDENTIALS_PATH, USERS_ME_PATH } from "../src/lib/api-paths.ts";
import { NetworkError, ServerError } from "../src/errors/index.ts";
import {
  IDENTITY,
  MINTED_CREDENTIAL,
  MINTED_CREDENTIAL_ID,
  SESSION_ID,
  TEST_JWT,
} from "./login-acceptance-fixtures.ts";

export const importSpkiPublicKey = async (
  publicKeyBase64Url: string,
): Promise<CryptoKey> => {
  const pad = "=".repeat((4 - (publicKeyBase64Url.length % 4)) % 4);
  const b64 = publicKeyBase64Url.replaceAll("-", "+").replaceAll("_", "/") + pad;
  const binary = atob(b64);
  const buf = new ArrayBuffer(binary.length);
  const bytes = new Uint8Array(buf);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return webcrypto.subtle.importKey(
    "spki",
    buf,
    { name: "ECDH", namedCurve: "P-256" },
    true,
    [],
  );
};

export const exportSpkiBase64Url = async (publicKey: CryptoKey): Promise<string> => {
  const spki = await webcrypto.subtle.exportKey("spki", publicKey);
  const bytes = new Uint8Array(spki);
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replaceAll("+", "-").replaceAll("_", "/").replace(/=+$/, "");
};

export interface DeviceFlowFixture {
  readonly capturedCliPubKey: { value: string | null };
  readonly verifyCalls: { count: number };
  // The exchange login makes with the recovered session token. Counted so a
  // test can prove it happened exactly once, and that its authorization was
  // the session token rather than anything read from disk.
  readonly mintCalls: { count: number; authorization: string | null; machineName: string | null };
}

export const httpLayer = (
  fixture: DeviceFlowFixture,
  opts: {
    identityFails?: boolean;
    identityAbsent?: boolean;
    identityUnreadable?: boolean;
    identity?: Record<string, unknown>;
    firstVerifyFails?: boolean;
    mintFails?: boolean;
  } = {},
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: <T>(input: HttpRequestInput): Effect.Effect<T, NetworkError | ServerError> => {
      const { path, method = "GET" } = input;
      if (method === "POST" && path === "/v1/auth/sessions") {
        const body = input.body as { public_key: string; token_name: string };
        fixture.capturedCliPubKey.value = body.public_key;
        return Effect.succeed({ session_id: SESSION_ID, request_id: "req_create" } as T);
      }
      if (method === "GET" && path === `/v1/auth/sessions/${SESSION_ID}`) {
        return Effect.succeed({
          status: "verification_pending",
          cli_public_key: fixture.capturedCliPubKey.value ?? "",
          token_name: "macos-cli",
          expires_at_ms: Date.now() + 60_000,
        } as T);
      }
      if (method === "POST" && path === `/v1/auth/sessions/${SESSION_ID}/verify`) {
        fixture.verifyCalls.count += 1;
        if (opts.firstVerifyFails && fixture.verifyCalls.count === 1) {
          // Wrong code on the first attempt → 400, which mapVerifyFailure
          // turns into VerificationFailedError so the retry kicks in.
          return Effect.fail(
            new ServerError({
              detail: "verification code didn't match",
              suggestion: "try again",
              code: "UZ-AUTH-010",
              status: 400,
              requestId: "req_verify_1",
            }),
          );
        }
        return Effect.promise(async () => {
          const cliPub = fixture.capturedCliPubKey.value;
          if (!cliPub) throw new Error("verify called before create");
          const dashboardKeypair = await webcrypto.subtle.generateKey(
            { name: "ECDH", namedCurve: "P-256" },
            true,
            ["deriveBits"],
          );
          const dashboardSpkiB64Url = await exportSpkiBase64Url(dashboardKeypair.publicKey);
          await importSpkiPublicKey(cliPub); // validate shape; throws on bad bytes
          const sharedKey = await deriveSharedKey(dashboardKeypair.privateKey, cliPub);
          const enc: EncryptedJwt = await encryptJwtForTest(sharedKey, TEST_JWT);
          return {
            dashboard_public_key: dashboardSpkiB64Url,
            ciphertext: enc.ciphertextBase64Url,
            nonce: enc.nonceBase64Url,
          } as T;
        });
      }
      if (method === "POST" && path === CLI_CREDENTIALS_PATH) {
        const body = input.body as { machine_name: string };
        fixture.mintCalls.count += 1;
        fixture.mintCalls.machineName = body.machine_name;
        fixture.mintCalls.authorization = input.token
          ? Redacted.value(input.token)
          : null;
        if (opts.mintFails) {
          return Effect.fail(
            new ServerError({
              detail: "session expired before the exchange",
              suggestion: "sign in again",
              code: "UZ-AUTH-006",
              status: 401,
              requestId: "req_mint_1",
            }),
          );
        }
        return Effect.succeed({
          id: MINTED_CREDENTIAL_ID,
          credential: MINTED_CREDENTIAL,
          machine_name: body.machine_name,
          deployment: "https://api.test.local",
        } as T);
      }
      if (
        method === "GET" &&
        path.startsWith("/v1/tenants/me/workspaces?")
      ) {
        return Effect.succeed({
          items: [],
          tenant_id: "tenant_login_fixture",
          total: null,
          next_cursor: null,
        } as T);
      }
      if (method === "GET" && path === USERS_ME_PATH) {
        // The post-login identity read (`readIdentity` in
        // `src/lib/me-ping.ts`). Unlike the billing probe it replaced, the
        // BODY matters: login reports the person it signed in, so the shape
        // has to decode or the success line falls back.
        // What a deployment older than this client answers: the credential is
        // fine and the ROUTE is not there.
        if (opts.identityAbsent) {
          return Effect.fail(
            new ServerError({
              detail: "",
              suggestion: "verify the request payload and retry",
              code: "HTTP_404",
              status: 404,
              requestId: null,
            }),
          );
        }
        // A deployment that HAS the route and answers 200 in a shape this
        // client cannot decode — the version-skew case that looks like success
        // to the transport and fails at the parse boundary.
        if (opts.identityUnreadable) {
          return Effect.succeed({ unexpected: "shape" } as T);
        }
        if (opts.identityFails) {
          return Effect.fail(
            new ServerError({
              detail: "token rejected",
              suggestion: "retry",
              code: "UZ-AUTH-401",
              status: 401,
              requestId: null,
            }),
          );
        }
        return Effect.succeed((opts.identity ?? IDENTITY) as T);
      }
      return Effect.fail(
        new ServerError({
          detail: `unexpected ${method} ${path}`,
          suggestion: "fix the test fixture",
          code: "UZ-TEST",
          status: 500,
          requestId: null,
        }),
      );
    },
  });
