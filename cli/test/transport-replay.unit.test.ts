// A request whose socket dropped is sent again — every method, no opt-in —
// and what each command then reports.
//
// The parity table (`tests/fixtures/retry-policy/cases.json`) proves the
// attempt counts inside the retry loop. This file proves the half the table
// cannot see: the command's own outcome. Every case runs the real
// `HttpClient` and the real loop, so the only double is the fetch at the
// boundary; a recorded config field would prove a value was set, not that
// the loop acted on it.
//
// The error shapes here are Bun's, taken from a probe of the shipped
// runtime rather than assumed: a dropped socket carries `ECONNRESET` on the
// error itself, a rejected certificate carries `DEPTH_ZERO_SELF_SIGNED_CERT`,
// and a request the client will not send carries no code at all.

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";

import { apiKeyCreateEffectFromArgs } from "../src/commands/api_key.ts";
import { secretAddEffectFromFlags } from "../src/commands/fleet_secret.ts";
import { createSession } from "../src/commands/login-device-flow.ts";
import { NetworkError } from "../src/errors/index.ts";
import { DEFAULT_MAX_ATTEMPTS } from "../src/lib/http-retry.ts";
import { CliConfig, DEFAULT_POSTHOG_HOST } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { httpClientLayer, type HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { asFetchImpl, socketDropped, unsendableRequest, type ResponseLike } from "./helpers.ts";
import { outputDouble } from "./helpers-output-double.ts";

const WS_ID = "ws_transport_replay";
const SECRET_NAME = "replayed-secret";
const SESSION_ID = "sess_transport_replay";
const PUBLIC_KEY = "cli-public-key";
const TOKEN_NAME = "unit-cli";
const API_KEY_NAME = "unit-key";
const FIXTURE_TOKEN = "header.payload.sig";
const HTTP_CREATED = 201;
const HTTP_CONFLICT = 409;
const ERR_SECRET_NAME_TAKEN = "UZ-VAULT-005";
const ATTEMPTS_AFTER_ONE_DROP = 2;
const ATTEMPTS_WITHOUT_A_REPLAY = 1;
const ATTEMPTS_AT_CEILING = DEFAULT_MAX_ATTEMPTS;
const NO_RETRY_ENV = "AGENTSFLEET_NO_RETRY";
const SELF_SIGNED_MESSAGE = "self signed certificate";

const answer = (status: number, body: unknown): ResponseLike => ({
  ok: status >= 200 && status < 300,
  status,
  statusText: String(status),
  headers: { get: () => null },
  text: async () => JSON.stringify(body),
});
const created = (body: unknown): ResponseLike => answer(HTTP_CREATED, body);
const nameTaken = (): ResponseLike =>
  answer(HTTP_CONFLICT, { error: { code: ERR_SECRET_NAME_TAKEN, message: "Secret name already taken" } });

/** Bun's shape for a certificate the client would not accept. */
const rejectedCertificate = (): TypeError =>
  Object.assign(new TypeError(SELF_SIGNED_MESSAGE), { code: "DEPTH_ZERO_SELF_SIGNED_CERT" });

/** The socket is lost on the first call, then the server answers. */
function droppingOnce(then: ResponseLike) {
  let calls = 0;
  const fetchImpl = asFetchImpl(async () => {
    calls += 1;
    if (calls === 1) throw socketDropped();
    return then;
  });
  return { fetchImpl, calls: () => calls };
}

/** Every call fails the same way. */
function alwaysFailing(failure: () => Error) {
  let calls = 0;
  const fetchImpl = asFetchImpl(async () => {
    calls += 1;
    throw failure();
  });
  return { fetchImpl, calls: () => calls };
}

const configWith = (fetchImpl: ReturnType<typeof asFetchImpl>): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.some(Redacted.make(FIXTURE_TOKEN)),
    jsonMode: false,
    noOpen: true,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: DEFAULT_POSTHOG_HOST,
    fetchImpl,
  });

const credentialsLayer: Layer.Layer<Credentials> = Layer.succeed(Credentials, {
  getAccessToken: Effect.succeed(Option.some(Redacted.make(FIXTURE_TOKEN))),
  snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
  saveAccessToken: () => Effect.void,
  clearAccessToken: Effect.void,
});

const workspacesLayer: Layer.Layer<Workspaces> = Layer.succeed(Workspaces, {
  load: Effect.succeed({
    current_workspace_id: WS_ID,
    items: [{ workspace_id: WS_ID, name: "test-ws", created_at: Date.now() }],
  }),
  save: () => Effect.void,
});

const outputLayer = (captured: string[]): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    ...outputDouble(),
    info: (msg) => Effect.sync(() => { captured.push(msg); }),
    success: (msg) => Effect.sync(() => { captured.push(msg); }),
    printJson: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
  });

const runCommand = <E>(
  effect: Effect.Effect<void, E, CliConfig | Credentials | HttpClient | Output | Workspaces>,
  fetchImpl: ReturnType<typeof asFetchImpl>,
  captured: string[],
) =>
  Effect.runPromiseExit(
    effect.pipe(
      Effect.provide(httpClientLayer),
      Effect.provide(configWith(fetchImpl)),
      Effect.provide(credentialsLayer),
      Effect.provide(workspacesLayer),
      Effect.provide(outputLayer(captured)),
    ),
  );

const failureOf = <E>(exit: Exit.Exit<unknown, E>): unknown =>
  Exit.isFailure(exit) ? Option.getOrNull(Cause.findErrorOption(exit.cause)) : null;

const storeSecret = (fetchImpl: ReturnType<typeof asFetchImpl>, captured: string[]) =>
  runCommand(secretAddEffectFromFlags({ name: SECRET_NAME, data: '{"k":"v"}' }), fetchImpl, captured);

// The retry loop reads the escape hatch from the process environment, and
// nothing here passes an environment of its own. A developer who exports
// AGENTSFLEET_NO_RETRY would otherwise see every replay assertion fail.
let savedNoRetry: string | undefined;
beforeEach(() => {
  savedNoRetry = process.env[NO_RETRY_ENV];
  delete process.env[NO_RETRY_ENV];
});
afterEach(() => {
  if (savedNoRetry === undefined) delete process.env[NO_RETRY_ENV];
  else process.env[NO_RETRY_ENV] = savedNoRetry;
});

describe("a request whose socket dropped is sent again", () => {
  test("secret create: two fetches, and the command reports the secret stored", async () => {
    const server = droppingOnce(created({}));
    const captured: string[] = [];
    const exit = await storeSecret(server.fetchImpl, captured);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(server.calls()).toBe(ATTEMPTS_AFTER_ONE_DROP);
    expect(captured.join("\n")).toContain(`Secret '${SECRET_NAME}' stored in vault.`);
  });

  test("secret create: a drop after the server stored it lands on the taken-name skip", async () => {
    // The drop can arrive after the write landed — while the reply is read.
    // The repeat is then answered UZ-VAULT-005, which `secret create` reports
    // as a skip. The value in the vault is the one this run sent, and the
    // changelog says so, because the printed line cannot tell the operator
    // which of the two it is.
    const server = droppingOnce(nameTaken());
    const captured: string[] = [];
    const exit = await storeSecret(server.fetchImpl, captured);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(server.calls()).toBe(ATTEMPTS_AFTER_ONE_DROP);
    expect(captured.join("\n")).toContain("already exists");
    expect(captured.join("\n")).not.toContain("stored in vault");
  });

  test("login session request: two fetches, and the session the server named comes back", async () => {
    const server = droppingOnce(
      created({ session_id: SESSION_ID, login_url: `https://dash.test.local/cli-auth/${SESSION_ID}`, request_id: "req_1" }),
    );
    const exit = await Effect.runPromiseExit(
      createSession(PUBLIC_KEY, TOKEN_NAME).pipe(
        Effect.provide(httpClientLayer),
        Effect.provide(configWith(server.fetchImpl)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    if (Exit.isSuccess(exit)) expect(exit.value.session_id).toBe(SESSION_ID);
    expect(server.calls()).toBe(ATTEMPTS_AFTER_ONE_DROP);
  });

  test("api-key create: two fetches — the accepted trade, not an oversight", async () => {
    // This is the cost of the rule. If the drop arrives after the daemon
    // minted the first key, the operator holds two and sees one — and the
    // unique `(tenant_id, key_name)` means the repeat is refused, so the
    // first key survives with its secret unread. The alternative failed a
    // command that mostly never reached the server at all.
    // `docs/architecture/web_app.md` names the trade; an idempotency key
    // retires it.
    const server = droppingOnce(created({ id: "key_1", key_name: API_KEY_NAME, secret: "agt_t_never_printed" }));
    const captured: string[] = [];
    const exit = await runCommand(
      apiKeyCreateEffectFromArgs({ name: API_KEY_NAME, description: undefined }),
      server.fetchImpl,
      captured,
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(server.calls()).toBe(ATTEMPTS_AFTER_ONE_DROP);
  });

  test("the socket keeps dropping: it stops at the ceiling, and no success line is printed", async () => {
    const server = alwaysFailing(socketDropped);
    const captured: string[] = [];
    const exit = await storeSecret(server.fetchImpl, captured);
    expect(server.calls()).toBe(ATTEMPTS_AT_CEILING);
    const failure = failureOf(exit);
    expect(failure).toBeInstanceOf(NetworkError);
    // Bun's wording must land on the same operator-facing text as Node's,
    // not on Bun's own "pass `verbose: true`" hint.
    expect((failure as NetworkError).detail).toStartWith("cannot reach agentsfleet API at");
    expect((failure as NetworkError).suggestion).toContain("check network connectivity");
    expect(captured.join("\n")).not.toMatch(/stored in vault|already exists|skipped/);
  });
});

describe("a failure that is not the connection is not sent again", () => {
  test("a rejected certificate: one fetch, and the operator reads why", async () => {
    // The code says the client refused the certificate, not that the network
    // is down. Retrying would fail identically three times, and answering
    // "check your proxy" would bury the one error worth reading closely —
    // an unexpected certificate is what interception looks like.
    const server = alwaysFailing(rejectedCertificate);
    const captured: string[] = [];
    const exit = await storeSecret(server.fetchImpl, captured);
    expect(server.calls()).toBe(ATTEMPTS_WITHOUT_A_REPLAY);
    const failure = failureOf(exit);
    expect(failure).toBeInstanceOf(NetworkError);
    expect((failure as NetworkError).detail).toContain(SELF_SIGNED_MESSAGE);
    expect((failure as NetworkError).detail).not.toContain("cannot reach agentsfleet API");
  });

  test("a request the client refused to send: one fetch, and the refusal surfaces", async () => {
    // No code at all means no socket: a header the client would not put on
    // the wire. Retrying costs a second of backoff and fails the same way.
    const server = alwaysFailing(unsendableRequest);
    const captured: string[] = [];
    const exit = await storeSecret(server.fetchImpl, captured);
    expect(server.calls()).toBe(ATTEMPTS_WITHOUT_A_REPLAY);
    expect(failureOf(exit)).toBeInstanceOf(NetworkError);
  });

  test(`${NO_RETRY_ENV}=1 sends a dropped write once and surfaces the drop`, async () => {
    process.env[NO_RETRY_ENV] = "1";
    const server = droppingOnce(created({}));
    const captured: string[] = [];
    const exit = await storeSecret(server.fetchImpl, captured);
    expect(server.calls()).toBe(ATTEMPTS_WITHOUT_A_REPLAY);
    expect(failureOf(exit)).toBeInstanceOf(NetworkError);
  });
});
