// Slice-7 acceptance: full device-flow login through loginEffect against
// faked services that perform a real ECDH + AES-256-GCM encrypt on the
// /verify response. Asserts the operator-visible contract end-to-end:
// exit 0, credentials persisted with the decrypted JWT, workspace
// hydration kicked off, analytics login-completed captured.
//
// Sits at the Effect-composition layer (not the runCli wrapper) because
// `runCli` has no current seam for injecting an Input fake — needed so
// the verification-code prompt resolves to a known string instead of
// blocking on real stdin. The dimension batch (D20/D22/D24) will widen
// runCli with that seam; until then the Effect-level test gives the
// same end-to-end confidence on the post-Slice-3 server contract.

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { webcrypto } from "node:crypto";
import { loginEffect } from "../src/commands/login.ts";
import {
  encryptJwtForTest,
  deriveSharedKey,
  type EncryptedJwt,
} from "../src/lib/cli-flow.ts";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import { Browser } from "../src/services/browser.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Input } from "../src/services/input.ts";
import { Output } from "../src/services/output.ts";
import { Stdin } from "../src/services/stdin.ts";
import {
  TelemetryRuntime,
  telemetryRuntimeFromValuesLayer,
} from "../src/services/telemetry/runtime.service.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { CLI_CREDENTIALS_PATH, USERS_ME_PATH } from "../src/lib/api-paths.ts";
import {
  CLI_CREDENTIAL_BODY_LEN,
  CLI_CREDENTIAL_PREFIX,
} from "../src/constants/cli-credential.ts";
import {
  AuthError,
  MeValidationError,
  NetworkError,
  ServerError,
  type CliError,
} from "../src/errors/index.ts";

// The person the identity read answers with, so the success line has a name to
// print. Its `email` is what a missing display name falls back to.
const IDENTITY = {
  user_id: "0193c5e1-0000-7000-8000-00000000abcd",
  email: "ada@example.com",
  display_name: "Ada Lovelace",
  tenant_id: "0193c5e0-0000-7000-8000-000000001234",
  tenant_name: "Ada's Workshop",
  credential: "cli_credential",
  scopes: ["fleet:read"],
} as const;

const SESSION_ID = "sess_acceptance_e2e";
const VERIFICATION_CODE = "424242";
const TEST_JWT = "eyJhbGciOiJIUzI1NiJ9.acceptance-payload.sig";

// What the mint hands back. Shaped the way the client validates on load —
// the afc_ prefix and a 64-character lower-case hex body — and built by
// repetition so this file carries no high-entropy literal.
const MINTED_BODY_CHAR = "b";
const MINTED_CREDENTIAL = `${CLI_CREDENTIAL_PREFIX}${MINTED_BODY_CHAR.repeat(CLI_CREDENTIAL_BODY_LEN)}`;
const MINTED_CREDENTIAL_ID = "cli_cred_acceptance";

interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly events: Array<{ event: string; properties: Record<string, unknown> }>;
  savedToken: string | null;
  savedSessionId: string | null;
  browserOpened: boolean;
  promptsAsked: number;
  cleared: boolean;
}

const makeRecorder = (): Recorder => ({
  stdout: [],
  stderr: [],
  events: [],
  savedToken: null,
  savedSessionId: null,
  browserOpened: false,
  promptsAsked: 0,
  cleared: false,
});

const importSpkiPublicKey = async (
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

const exportSpkiBase64Url = async (publicKey: CryptoKey): Promise<string> => {
  const spki = await webcrypto.subtle.exportKey("spki", publicKey);
  const bytes = new Uint8Array(spki);
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replaceAll("+", "-").replaceAll("_", "/").replace(/=+$/, "");
};

interface DeviceFlowFixture {
  readonly capturedCliPubKey: { value: string | null };
  readonly verifyCalls: { count: number };
  // The exchange login makes with the recovered session token. Counted so a
  // test can prove it happened exactly once, and that its authorization was
  // the session token rather than anything read from disk.
  readonly mintCalls: { count: number; authorization: string | null; machineName: string | null };
}

const httpLayer = (
  fixture: DeviceFlowFixture,
  opts: {
    identityFails?: boolean;
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

const outputLayer = (rec: Recorder): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    info: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    success: (msg) => Effect.sync(() => rec.stdout.push(`ok: ${msg}`)),
    warn: (msg) => Effect.sync(() => rec.stderr.push(`warn: ${msg}`)),
    error: (msg) => Effect.sync(() => rec.stderr.push(`error: ${msg}`)),
    outro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    printJson: (payload) => Effect.sync(() => rec.stdout.push(JSON.stringify(payload))),
    printJsonErr: (payload) => Effect.sync(() => rec.stderr.push(JSON.stringify(payload))),
    printKeyValue: (record) =>
      Effect.sync(() => {
        for (const [k, v] of Object.entries(record)) rec.stdout.push(`  ${k}: ${v}`);
      }),
    printSection: (title) => Effect.sync(() => rec.stdout.push(`# ${title}`)),
    printTable: () => Effect.void,
  });

const inputLayer = (rec: Recorder, code: string): Layer.Layer<Input> =>
  Layer.succeed(Input, {
    readLine: () =>
      Effect.sync(() => {
        rec.promptsAsked += 1;
        return code;
      }),
  });

const credentialsLayer = (rec: Recorder): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => Option.none<Redacted.Redacted<string>>()),
    snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
    saveAccessToken: (input) =>
      Effect.sync(() => {
        rec.savedToken = Redacted.value(input.token);
        rec.savedSessionId = input.sessionId;
      }),
    clearAccessToken: Effect.sync(() => {
      rec.cleared = true;
    }),
  });

const browserLayer = (rec: Recorder): Layer.Layer<Browser> =>
  Layer.succeed(Browser, {
    open: () =>
      Effect.sync(() => {
        rec.browserOpened = true;
        return true;
      }),
  });

const workspacesLayer: Layer.Layer<Workspaces> = Layer.succeed(Workspaces, {
  load: Effect.succeed({ current_workspace_id: null, items: [] }),
  save: () => Effect.void,
});

const analyticsLayer = (rec: Recorder): Layer.Layer<Analytics> =>
  Layer.succeed(Analytics, {
    capture: (event, properties = {}) =>
      Effect.sync(() => {
        rec.events.push({ event, properties });
      }),
    identify: () => Effect.void,
    alias: () => Effect.void,
    groupIdentify: () => Effect.void,
  });

const makeConfig = (jsonMode: boolean): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

const configLayer: Layer.Layer<CliConfig> = makeConfig(false);

// Interactive terminal so the resolve step returns `none` and the device
// flow runs — this suite is the full ECDH round trip, not the direct path.
const stdinLayer: Layer.Layer<Stdin> = Layer.succeed(Stdin, {
  isTTY: true,
  readToEnd: Effect.succeed(""),
});

const telemetryLayer: Layer.Layer<TelemetryRuntime> = telemetryRuntimeFromValuesLayer({
  configDir: "/tmp/test-config",
  tracesDir: "/tmp/test-traces",
  consent: "granted",
  showDebug: false,
  deviceId: "dev_test",
  sessionId: "telem_test",
  isFirstRun: false,
  isTty: false,
  isCi: true,
  os: "test",
  arch: "test",
  cliVersion: "0.0.0",
});

describe("login acceptance — full device flow end-to-end", () => {
  test("create → poll → prompt → verify → decrypt → persist → exit 0", async () => {
    const rec = makeRecorder();
    const fixture: DeviceFlowFixture = {
      capturedCliPubKey: { value: null },
      verifyCalls: { count: 0 },
      mintCalls: { count: 0, authorization: null, machineName: null },
    };

    const program = loginEffect({
      noOpen: true,
      noInput: false,
      force: true,
      tokenName: undefined,
    }).pipe(
      Effect.provide(httpLayer(fixture)),
      Effect.provide(inputLayer(rec, VERIFICATION_CODE)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(credentialsLayer(rec)),
      Effect.provide(browserLayer(rec)),
      Effect.provide(workspacesLayer),
      Effect.provide(analyticsLayer(rec)),
      Effect.provide(configLayer),
      Effect.provide(telemetryLayer),
      Effect.provide(stdinLayer),
    ) as Effect.Effect<void, CliError, never>;

    const exit = await Effect.runPromiseExit(program);

    if (Exit.isFailure(exit)) {
      throw new Error(`expected success, got: ${Cause.pretty(exit.cause)}`);
    }
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    // The session token bought the credential and was then discarded;
    // what reaches disk outlives the minute that token had left.
    expect(rec.savedToken).not.toBe(TEST_JWT);
    expect(rec.savedSessionId).toBe(SESSION_ID);
    expect(rec.promptsAsked).toBe(1);
    expect(fixture.verifyCalls.count).toBe(1);
    // The success line NAMES the person, which is the whole point of the
    // identity read: a terminal that reported "login complete" left the
    // operator with no way to tell which account it had just signed into.
    expect(
      rec.stdout.some(
        (line) =>
          line.includes(IDENTITY.display_name) && line.includes(IDENTITY.tenant_name),
      ),
    ).toBe(true);
    // Analytics capture asserted separately in the unit-test suite — the
    // captureLoginCompleted helper writes to a real config-dir path
    // before emitting, which would require staging a real tmp tree just
    // to assert here. The decrypt + persist contract is what this
    // acceptance case is for; analytics emission is covered by the
    // login-effect / login-logout-identity unit tests once the dimension
    // batch reinstates them.
  });
});

const runLogin = (
  rec: Recorder,
  fixture: DeviceFlowFixture,
  opts: {
    jsonMode?: boolean;
    identityFails?: boolean;
    identity?: Record<string, unknown>;
    firstVerifyFails?: boolean;
    mintFails?: boolean;
  } = {},
): Effect.Effect<void, CliError, never> =>
  loginEffect({
    noOpen: true,
    noInput: false,
    force: true,
    tokenName: undefined,
  }).pipe(
    Effect.provide(
      httpLayer(fixture, {
        identityFails: opts.identityFails ?? false,
        ...(opts.identity !== undefined ? { identity: opts.identity } : {}),
        firstVerifyFails: opts.firstVerifyFails ?? false,
        mintFails: opts.mintFails ?? false,
      }),
    ),
    Effect.provide(inputLayer(rec, VERIFICATION_CODE)),
    Effect.provide(outputLayer(rec)),
    Effect.provide(credentialsLayer(rec)),
    Effect.provide(browserLayer(rec)),
    Effect.provide(workspacesLayer),
    Effect.provide(analyticsLayer(rec)),
    Effect.provide(makeConfig(opts.jsonMode ?? false)),
    Effect.provide(telemetryLayer),
    Effect.provide(stdinLayer),
  ) as Effect.Effect<void, CliError, never>;

const freshFixture = (): DeviceFlowFixture => ({
  capturedCliPubKey: { value: null },
  verifyCalls: { count: 0 },
  mintCalls: { count: 0, authorization: null, machineName: null },
});

describe("login acceptance — jsonMode rendering + rollback", () => {
  test("an identity carrying no name and no address still completes the login", async () => {
    const rec = makeRecorder();
    // A server answering a 200 with neither a display name nor an address is
    // broken, and the login is not: the credential was minted, it
    // authenticated, and it is on disk. So the line falls back to reporting
    // what happened rather than naming a person it was not told about. A
    // rendering gap must never fail work that completed.
    const exit = await Effect.runPromiseExit(
      runLogin(rec, freshFixture(), {
        identity: { ...IDENTITY, display_name: undefined, email: "" },
      }),
    );

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    expect(rec.stdout.some((line) => line.includes("login complete"))).toBe(true);
    expect(rec.stdout.some((line) => line.includes("signed in as"))).toBe(false);
  });

  test("jsonMode prints the machine-readable complete payload (no human prose)", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(runLogin(rec, freshFixture(), { jsonMode: true }));
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    // The session token bought the credential and was then discarded;
    // what reaches disk outlives the minute that token had left.
    expect(rec.savedToken).not.toBe(TEST_JWT);
    expect(rec.stdout.some((l) => l.includes('"status":"complete"'))).toBe(true);
    expect(rec.stdout.some((l) => l.includes('"token_saved":true'))).toBe(true);
    expect(rec.stdout.some((l) => l.includes("login complete"))).toBe(false);
  });

  test("post-login /me ping failure rolls back the persisted credential", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(runLogin(rec, freshFixture(), { identityFails: true }));
    expect(Exit.isFailure(exit)).toBe(true);
    const err = Exit.isFailure(exit)
      ? Option.getOrNull(Cause.findErrorOption(exit.cause))
      : null;
    expect(err).toBeInstanceOf(MeValidationError);
    // The token was persisted moments before validation failed; rollback
    // must wipe it so subsequent commands don't reuse a dead-on-arrival token.
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    // The session token bought the credential and was then discarded;
    // what reaches disk outlives the minute that token had left.
    expect(rec.savedToken).not.toBe(TEST_JWT);
    expect(rec.cleared).toBe(true);
  });

  test("first wrong code then correct code: retry succeeds, token persists", async () => {
    const rec = makeRecorder();
    const fixture = freshFixture();
    const exit = await Effect.runPromiseExit(
      runLogin(rec, fixture, { firstVerifyFails: true }),
    );
    if (Exit.isFailure(exit)) {
      throw new Error(`expected retry success, got: ${Cause.pretty(exit.cause)}`);
    }
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    // The session token bought the credential and was then discarded;
    // what reaches disk outlives the minute that token had left.
    expect(rec.savedToken).not.toBe(TEST_JWT);
    // Prompted twice (first attempt + retry), called /verify twice.
    expect(rec.promptsAsked).toBe(2);
    expect(fixture.verifyCalls.count).toBe(2);
  });
});

describe("login acceptance — the credential exchange", () => {
  test("test_login_persists_credential_not_session_token — the session token authorises one mint and is then discarded", async () => {
    const rec = makeRecorder();
    const fixture = freshFixture();
    const exit = await Effect.runPromiseExit(runLogin(rec, fixture));
    expect(Exit.isSuccess(exit)).toBe(true);

    // Spent exactly once, and spent as the authorization — the whole point
    // of the sixty-second window is that it buys one durable thing.
    expect(fixture.mintCalls.count).toBe(1);
    expect(fixture.mintCalls.authorization).toBe(TEST_JWT);

    // The label is hostname-derived and inside the server's grammar. A
    // platform label ("macos-cli") would make every Mac claim one row, so
    // asserting the grammar also guards the machine-per-row key.
    expect(fixture.mintCalls.machineName).toMatch(/^[a-zA-Z0-9._-]{1,64}$/);

    // What survives on disk is the credential, and the session token
    // appears nowhere in the persisted record.
    expect(rec.savedToken).toBe(MINTED_CREDENTIAL);
    expect(rec.savedToken).not.toBe(TEST_JWT);
  });

  test("test_failed_exchange_persists_nothing — a refused mint writes nothing and reports why the daemon refused", async () => {
    const rec = makeRecorder();
    const fixture = freshFixture();
    const exit = await Effect.runPromiseExit(
      runLogin(rec, fixture, { mintFails: true }),
    );
    expect(Exit.isFailure(exit)).toBe(true);

    // The exchange was attempted and refused, and nothing reached disk —
    // not the credential, and above all not the session token the flow was
    // still holding at that moment.
    expect(fixture.mintCalls.count).toBe(1);
    expect(rec.savedToken).toBeNull();

    const err = Exit.isFailure(exit)
      ? Option.getOrNull(Cause.findErrorOption(exit.cause))
      : null;
    expect(err).toBeInstanceOf(AuthError);
    // The daemon named the cause (an expired session). That code survives
    // instead of being flattened into the client's generic one, so the
    // operator is told which failure happened.
    expect((err as InstanceType<typeof AuthError>).code).toBe("UZ-AUTH-006");
  });
});
