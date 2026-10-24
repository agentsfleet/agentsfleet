import { describe, expect, spyOn, test } from "bun:test";
import { EventEmitter } from "node:events";
import { Effect, Exit, Layer, Option, Redacted } from "effect";
import { loginEffect } from "../src/commands/login.ts";
import { browserLogin, TerminalLoginControls } from "../src/commands/login-browser-flow.ts";
import { LoginCallback } from "../src/commands/login-callback.ts";
import { loginComparisonCode, trustedLoginUrl } from "../src/commands/login-callback-protocol.ts";
import { deriveSharedKey, encryptJwtForTest, generateCliKeypair } from "../src/lib/cli-flow.ts";
import { Browser } from "../src/services/browser.service.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Input } from "../src/services/input.ts";
import { NetworkError, ServerError, UnexpectedError, type CliError } from "../src/errors/index.ts";
import { CLI_CREDENTIAL_PREFIX } from "../src/constants/cli-credential.ts";
import { SIGINT } from "../src/constants/signals.ts";
import { HTTP_METHOD } from "../src/constants/http-method.ts";
import { analyticsLayer, makeConfig, makeRec, outputLayer, stdinTty, telemetryLayer, workspacesLayer } from "./helpers-login-effect.ts";

const SESSION_ID = "local-browser-session";
const DASHBOARD_ORIGIN = "https://dash.test.local";
const LOGIN_URL = `${DASHBOARD_ORIGIN}/cli-auth/${SESSION_ID}`;
const VERIFICATION_CODE = "654321";
const SESSION_TOKEN = "ephemeral-test-session";
const PREVIOUS_CREDENTIAL = "previous-credential";
const INPUT_EVENT = "data";
const FAILURE = { none: "none", verify: "verify", decrypt: "decrypt", mint: "mint", mintLost: "mint-lost", persist: "persist", identity: "identity" } as const;
type Failure = (typeof FAILURE)[keyof typeof FAILURE];
const COMMITTED_FAILURES = new Set<Failure>([FAILURE.mintLost, FAILURE.persist, FAILURE.identity]);

class LoginBoundary {
  readonly #failure: Failure;
  readonly #deny: boolean;
  readonly #opened: boolean;
  readonly #url: string;
  readonly #credential = `${CLI_CREDENTIAL_PREFIX}${Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString("hex")}`;
  #publicKey = "";
  #stored: string | null = PREVIOUS_CREDENTIAL;
  #minted = 0;
  #verifies = 0;
  #opens = 0;
  #reply: Promise<Response> | null = null;
  #replay: (() => Promise<Response>) | null = null;
  readonly #replyAbort = new AbortController();
  readonly rec = makeRec();
  readonly verifyEntered = Promise.withResolvers<void>();
  readonly releaseVerify = Promise.withResolvers<void>();
  readonly #hold: boolean;

  constructor(options: { failure?: Failure; deny?: boolean; opened?: boolean; loginUrl?: string; hold?: boolean } = {}) {
    this.#failure = options.failure ?? FAILURE.none;
    this.#deny = options.deny ?? false;
    this.#opened = options.opened ?? true;
    this.#url = options.loginUrl ?? LOGIN_URL;
    this.#hold = options.hold ?? false;
  }
  get stored(): string | null { return this.#stored; }
  get minted(): number { return this.#minted; }
  get verifies(): number { return this.#verifies; }
  get opens(): number { return this.#opens; }
  get credential(): string { return this.#credential; }

  async responseText(): Promise<string> {
    if (!this.#reply) throw new Error("browser callback was not sent");
    return (await this.#reply).text();
  }

  async loseReplyAndReplay(): Promise<number> {
    this.#replyAbort.abort();
    if (!this.#reply || !this.#replay) throw new Error("no callback to replay");
    await this.#reply.catch(() => null);
    return (await this.#replay()).status;
  }

  readonly browser = Layer.succeed(Browser, {
    open: (value) => Effect.sync(() => {
      this.#opens += 1;
      const url = new URL(value);
      expect(url.searchParams.get("flow")).toBe("loopback");
      const fields = new URLSearchParams(url.hash.slice(1));
      expect(fields.get("cli_public_key")).toBe(this.#publicKey);
      expect(Buffer.from(this.#publicKey, "base64url").byteLength).toBe(91);
      const target = fields.get("callback");
      if (!target) throw new Error("missing callback");
      const body = new URLSearchParams({ state: fields.get("state") ?? "", session_id: SESSION_ID, status: this.#deny ? "denied" : "approved" });
      if (!this.#deny) body.set("verification_code", VERIFICATION_CODE);
      this.#replay = () => fetch(target, { method: HTTP_METHOD.post, headers: { Origin: url.origin }, body });
      this.#reply = fetch(target, { method: HTTP_METHOD.post, headers: { Origin: url.origin }, body, signal: this.#replyAbort.signal });
      return this.#opened;
    }),
  });

  readonly credentials = Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => this.#stored === null ? Option.none() : Option.some(Redacted.make(this.#stored))),
    snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
    saveAccessToken: (input) => this.#failure === FAILURE.persist
      ? Effect.fail(new UnexpectedError({ detail: "injected disk refusal", suggestion: "retry" }))
      : Effect.sync(() => { this.#stored = Redacted.value(input.token); }),
    clearAccessToken: Effect.sync(() => { this.#stored = null; }),
  });

  readonly http = Layer.succeed(HttpClient, {
    request: <T>(input: HttpRequestInput) => this.#request(input) as Effect.Effect<T, NetworkError | ServerError>,
  });

  #request(input: HttpRequestInput): Effect.Effect<unknown, NetworkError | ServerError> {
    if (input.path === "/v1/auth/sessions") {
      const body = input.body as { public_key: string };
      this.#publicKey = body.public_key;
      return Effect.succeed({ session_id: SESSION_ID, login_url: this.#url });
    }
    if (input.path.endsWith("/verify")) return this.#verify();
    if (input.path === "/v1/cli-credentials") return this.#mint();
    if (input.path === "/v1/users/me") {
      return this.#failure === FAILURE.identity ? this.#refuse() : Effect.succeed({
        user_id: "person", email: "person@example.test", display_name: "Person", tenant_id: "account", tenant_name: "Team", credential: "cli", scopes: [],
      });
    }
    if (input.path.startsWith("/v1/tenants/me/workspaces?")) return Effect.succeed({ items: [], tenant_id: "account", next_cursor: null });
    return Effect.die("unexpected login boundary request");
  }

  #refuse(): Effect.Effect<never, ServerError> {
    return Effect.fail(new ServerError({ detail: "injected refusal", suggestion: "retry", code: "UZ-AUTH-002", status: 401, requestId: null }));
  }

  #verify(): Effect.Effect<unknown, ServerError> {
    this.#verifies += 1;
    this.verifyEntered.resolve();
    if (this.#failure === FAILURE.verify) return this.#refuse();
    return Effect.promise(async () => {
      if (this.#hold) await this.releaseVerify.promise;
      const dashboard = await generateCliKeypair();
      const key = await deriveSharedKey(dashboard.privateKey, this.#publicKey);
      const encrypted = await encryptJwtForTest(key, SESSION_TOKEN);
      return { dashboard_public_key: dashboard.publicKeyBase64Url, nonce: encrypted.nonceBase64Url, ciphertext: this.#failure === FAILURE.decrypt ? "invalid" : encrypted.ciphertextBase64Url };
    });
  }

  #mint(): Effect.Effect<unknown, ServerError | NetworkError> {
    if (this.#failure === FAILURE.mint) return this.#refuse();
    this.#minted += 1;
    if (this.#failure === FAILURE.mintLost) return Effect.fail(new NetworkError({ detail: "mint reply lost after commit", suggestion: "retry", url: "/v1/cli-credentials" }));
    return Effect.succeed({ id: "credential-record", credential: this.#credential });
  }

  run(): Promise<Exit.Exit<void, CliError>> {
    const effect = loginEffect({ force: true, noOpen: false, noInput: true, tokenName: undefined }).pipe(
      Effect.provide(this.http), Effect.provide(this.browser), Effect.provide(this.credentials),
      Effect.provide(Layer.succeed(Input, { readLine: () => Effect.die("automatic login prompted for a code") })),
      Effect.provide(outputLayer(this.rec)), Effect.provide(stdinTty), Effect.provide(makeConfig({ noOpen: false })),
      Effect.provide(analyticsLayer), Effect.provide(telemetryLayer), Effect.provide(workspacesLayer),
    );
    return Effect.runPromiseExit(effect);
  }
}

describe("automatic terminal authorization", () => {
  test("real callback and crypto complete the existing mint, persistence and named identity path", async () => {
    const boundary = new LoginBoundary();
    expect(Exit.isSuccess(await boundary.run())).toBe(true);
    expect(boundary.verifies).toBe(1);
    expect(boundary.minted).toBe(1);
    expect(boundary.stored).toBe(boundary.credential);
    expect(await boundary.responseText()).toContain("Terminal authorized");
    expect(boundary.rec.stdout.some((line) => line.includes("signed in as Person — Team"))).toBe(true);
  });

  test.each([FAILURE.verify, FAILURE.decrypt, FAILURE.mint, FAILURE.mintLost, FAILURE.persist, FAILURE.identity])("%s failure refuses the browser and preserves documented residual state", async (failure) => {
    const boundary = new LoginBoundary({ failure });
    expect(Exit.isFailure(await boundary.run())).toBe(true);
    expect(await boundary.responseText()).toContain("Sign-in did not complete");
    const mintCommitted = COMMITTED_FAILURES.has(failure);
    expect(boundary.minted).toBe(mintCommitted ? 1 : 0);
    expect(boundary.stored).toBe(failure === FAILURE.identity ? null : PREVIOUS_CREDENTIAL);
    expect(boundary.verifies).toBe(1);
    expect(boundary.rec.stdout.some((line) => line.includes("signed in as"))).toBe(false);
  });

  test("Deny never verifies or mints and preserves the existing credential", async () => {
    const boundary = new LoginBoundary({ deny: true });
    expect(Exit.isFailure(await boundary.run())).toBe(true);
    expect(boundary.verifies).toBe(0);
    expect(boundary.minted).toBe(0);
    expect(boundary.stored).toBe(PREVIOUS_CREDENTIAL);
    expect(await boundary.responseText()).toContain("Authorization denied");
  });

  test("opener refusal still completes using the same-computer URL without a manual prompt", async () => {
    const boundary = new LoginBoundary({ opened: false });
    expect(Exit.isSuccess(await boundary.run())).toBe(true);
    expect(boundary.rec.stdout.some((line) => line.includes("restart with --no-open"))).toBe(true);
  });

  test("interrupt after approval cannot cancel credential finalization and its handler is released", async () => {
    const before = process.listenerCount(SIGINT);
    const boundary = new LoginBoundary({ hold: true });
    const running = boundary.run();
    await boundary.verifyEntered.promise;
    expect(process.listenerCount(SIGINT)).toBe(before + 1);
    process.emit(SIGINT);
    boundary.releaseVerify.resolve();
    expect(Exit.isSuccess(await running)).toBe(true);
    expect(process.listenerCount(SIGINT)).toBe(before);
    expect(boundary.minted).toBe(1);
  });

  test("losing the held browser response and replaying approval still mints exactly once", async () => {
    const boundary = new LoginBoundary({ hold: true });
    const running = boundary.run();
    await boundary.verifyEntered.promise;
    expect(await boundary.loseReplyAndReplay()).toBe(409);
    boundary.releaseVerify.resolve();
    expect(Exit.isSuccess(await running)).toBe(true);
    expect(boundary.verifies).toBe(1);
    expect(boundary.minted).toBe(1);
    expect(boundary.stored).toBe(boundary.credential);
  });

  test.each(["https://attacker.test/cli-auth/local-browser-session", `${LOGIN_URL}?other=1`, `${LOGIN_URL}#other=1`, `${DASHBOARD_ORIGIN}/cli-auth/other`, "https://person:secret@dash.test.local/cli-auth/local-browser-session"])("rejects a substituted API login URL before opening a browser: %s", async (loginUrl) => {
    const boundary = new LoginBoundary({ loginUrl });
    expect(Exit.isFailure(await boundary.run())).toBe(true);
    expect(boundary.opens).toBe(0);
    expect(boundary.verifies).toBe(0);
    expect(boundary.stored).toBe(PREVIOUS_CREDENTIAL);
  });
});

class TerminalDouble extends EventEmitter {
  readonly isTTY = true;
  isRaw = false;
  paused = true;
  setRawMode(raw: boolean): this { this.isRaw = raw; return this; }
  isPaused(): boolean { return this.paused; }
  pause(): this { this.paused = true; return this; }
}

test.each([
  ["canceled", "InterruptedError"], ["expired", "ExpiredSessionError"],
  ["listener refused", "SessionAbortedError"], ["terminal refused", "SessionAbortedError"],
] as const)("%s browser startup refuses credential exchange and releases terminal resources", async (failure, tag) => {
  const keypair = await generateCliKeypair();
  const before = process.listenerCount(SIGINT);
  const rec = makeRec();
  let completed = 0;
  let opened = 0;
  let callbackUrl: string | null = null;
  const observedListener = failure === "terminal refused" ? spyOn(Bun, "serve") : null;
  const fault = failure === "listener refused"
    ? spyOn(Bun, "serve").mockImplementation(() => { throw new Error("listener refused"); })
    : failure === "terminal refused"
      ? spyOn(process.stdin, "isPaused").mockImplementation(() => { throw new Error("terminal refused"); }) : null;
  try {
    const effect = browserLogin({ sessionId: SESSION_ID, keypair, dashboardUrl: DASHBOARD_ORIGIN,
      loginUrl: LOGIN_URL, deadlineMs: failure === "expired" ? Date.now() - 1 : Date.now() + 2_000 },
    () => Effect.sync(() => { completed += 1; })).pipe(
      Effect.provide(outputLayer(rec)),
      Effect.provide(Layer.succeed(HttpClient, { request: () => Effect.die("refused login reached credential verification") })),
      Effect.provide(Layer.succeed(Browser, { open: (url) => Effect.sync(() => {
        opened += 1;
        callbackUrl = new URLSearchParams(new URL(url).hash.slice(1)).get("callback");
        if (failure === "canceled") process.emit(SIGINT);
        return false;
      }) })),
    );
    const error = await Effect.runPromise(Effect.flip(effect));
    expect(error._tag).toBe(tag);
    expect(error.detail).toContain(failure === "listener refused" ? "local authorization listener"
      : failure === "terminal refused" ? "terminal cancellation keys" : failure);
    expect(error.suggestion).toContain("retry");
    expect(completed).toBe(0);
    expect(opened).toBe(failure === "canceled" || failure === "expired" ? 1 : 0);
    expect(rec.stdout.some((line) => line.includes("Finalizing sign-in"))).toBe(false);
    if (callbackUrl) await expect(fetch(callbackUrl)).rejects.toThrow();
    if (observedListener) {
      expect(observedListener.mock.results).toHaveLength(1);
      const server = observedListener.mock.results[0]!.value;
      if (typeof server !== "object" || server === null || !("port" in server)) throw new Error("listener was not created");
      await expect(fetch(`http://127.0.0.1:${server.port}/cli-auth/callback`)).rejects.toThrow();
    }
  } finally { fault?.mockRestore(); observedListener?.mockRestore(); }
  expect(process.listenerCount(SIGINT)).toBe(before);
});

test.each(["\u001b", "\u0003", SIGINT])("%j cancels waiting and controls restore raw mode and signal listeners", async (cancel) => {
  const callback = new LoginCallback({ sessionId: SESSION_ID, dashboardOrigin: DASHBOARD_ORIGIN });
  const input = new TerminalDouble();
  const signals = new EventEmitter();
  const controls = new TerminalLoginControls(callback, input, signals);
  try {
    expect(input.isRaw).toBe(true);
    if (cancel === SIGINT) signals.emit(SIGINT);
    else input.emit(INPUT_EVENT, cancel);
    expect(await callback.waitForDecision()).toEqual({ status: "canceled" });
  } finally { controls.close(); await callback.close(); }
  expect(input.isRaw).toBe(false);
  expect(input.listenerCount(INPUT_EVENT)).toBe(0);
  expect(signals.listenerCount(SIGINT)).toBe(0);
});

test("terminal setup failure restores raw mode and removes the signal handler", async () => {
  const callback = new LoginCallback({ sessionId: SESSION_ID, dashboardOrigin: DASHBOARD_ORIGIN });
  const input = new TerminalDouble();
  const signals = new EventEmitter();
  input.on = () => { throw new Error("input listener refused"); };
  try {
    expect(() => new TerminalLoginControls(callback, input, signals)).toThrow("input listener refused");
    expect(input.isRaw).toBe(false);
    expect(signals.listenerCount(SIGINT)).toBe(0);
    expect(callback.waiting).toBe(true);
  } finally { await callback.close(); }
});

test("comparison code is stable and bound to both session and the complete existing public key", async () => {
  const keypair = await generateCliKeypair();
  const code = await loginComparisonCode(SESSION_ID, keypair.publicKeyBase64Url);
  expect(code).toMatch(/^[A-F0-9]{4}-[A-F0-9]{4}$/);
  expect(await loginComparisonCode(SESSION_ID, keypair.publicKeyBase64Url)).toBe(code);
  expect(await loginComparisonCode("other", keypair.publicKeyBase64Url)).not.toBe(code);
  expect(await loginComparisonCode(SESSION_ID, `${keypair.publicKeyBase64Url}a`)).not.toBe(code);
  expect(trustedLoginUrl(DASHBOARD_ORIGIN, LOGIN_URL, SESSION_ID).href).toBe(LOGIN_URL);
});

test("comparison code matches the published transcript and complete SPKI vector", async () => {
  // pin test: these public inputs and output are the cross-runtime wire contract.
  const session = "018f1f4d-6d7b-7e1f-8c7a-123456789abc";
  const publicKey = "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEaxfR8uEsQkf4vOblY6RA8ncDfYEt6zOg9KE5RdiYwpZP40Li_hp_m47n60p8D54WK84zV2sxXs7LtkBoN79R9Q";
  expect(await loginComparisonCode(session, publicKey)).toBe("765D-06BF");
});
