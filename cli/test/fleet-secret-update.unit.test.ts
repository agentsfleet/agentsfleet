// `agentsfleet secret update` — whole-body replace over one PUT.
//
// The value under test is a credential, so the assertions are as much about
// what does NOT happen as what does: no request on a rejected invocation, no
// secret bytes in either renderer's output, and exactly one PUT on the happy
// path — a preflight GET or a DELETE+POST pair would each reopen the window
// the command exists to remove.

import { describe, test, expect, mock } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";

import { secretUpdateEffectFromFlags } from "../src/commands/fleet_secret.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { ServerError, ValidationError, type CliError } from "../src/errors/index.ts";

const WS_ID = "ws_unit_update_test";
const SECRET_NAME = "acme";
const NEW_TOKEN = "ghp-updated-DO-NOT-LEAK-4b7f";
const ITEM_PATH = `/v1/workspaces/${WS_ID}/secrets/${SECRET_NAME}`;

interface Sent {
  readonly path: string;
  readonly method?: string | undefined;
  readonly body?: unknown;
}

const makeOutputLayer = (captured: string[]): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: (msg) => Effect.sync(() => { captured.push(msg); }),
    info: (msg) => Effect.sync(() => { captured.push(msg); }),
    success: (msg) => Effect.sync(() => { captured.push(`ok: ${msg}`); }),
    warn: (msg) => Effect.sync(() => { captured.push(`warn: ${msg}`); }),
    error: (msg) => Effect.sync(() => { captured.push(`err: ${msg}`); }),
    outro: (msg) => Effect.sync(() => { captured.push(msg); }),
    printJson: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
    printJsonErr: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
    printKeyValue: (r) => Effect.sync(() => { captured.push(JSON.stringify(r)); }),
    printSection: (t) => Effect.sync(() => { captured.push(`# ${t}`); }),
    printTable: (_cols, rows) =>
      Effect.sync(() => { for (const row of rows) captured.push(JSON.stringify(row)); }),
  });

const makeConfigLayer = (jsonMode = false): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.some(Redacted.make("header.payload.sig")),
    jsonMode,
    noOpen: true,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

const makeCredsLayer = (): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(Option.some(Redacted.make("header.payload.sig"))),
    getSavedAt: Effect.succeed(Date.now()),
    getSessionId: Effect.succeed("sess_test"),
    getApiUrl: Effect.succeed(null),
    getCredentialId: Effect.succeed(null),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

const makeWsLayer = (): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.succeed({
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "test-ws", created_at: Date.now() }],
    }),
    save: () => Effect.void,
  });

/** Records every request the command issues, so a test can assert the ledger
 *  is exactly what it should be — including that it is empty. */
const makeRecordingHttp = (
  sent: Sent[],
  responder?: () => Effect.Effect<unknown, CliError>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: ((input: Sent) => {
      sent.push({ path: input.path, method: input.method, body: input.body });
      return (responder ? responder() : Effect.succeed({ name: SECRET_NAME }));
    }) as HttpClient["request"],
  });

const run = (
  effect: Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces>,
  captured: string[],
  sent: Sent[],
  jsonMode = false,
  responder?: () => Effect.Effect<unknown, CliError>,
) =>
  Effect.runPromiseExit(
    effect.pipe(
      Effect.provide(makeConfigLayer(jsonMode)),
      Effect.provide(makeCredsLayer()),
      Effect.provide(makeRecordingHttp(sent, responder)),
      Effect.provide(makeOutputLayer(captured)),
      Effect.provide(makeWsLayer()),
    ),
  );

const withStdin = async <T>(text: string, fn: () => Promise<T>): Promise<T> => {
  const original = Bun.stdin.text.bind(Bun.stdin);
  Bun.stdin.text = mock(() => Promise.resolve(text));
  try {
    return await fn();
  } finally {
    Bun.stdin.text = original;
  }
};

describe("test_secret_update_sends_single_put", () => {
  test("issues exactly one PUT carrying the whole body, and no read or delete around it", async () => {
    const captured: string[] = [];
    const sent: Sent[] = [];
    const exit = await run(
      secretUpdateEffectFromFlags({ name: SECRET_NAME, data: `{"token":"${NEW_TOKEN}"}` }),
      captured,
      sent,
    );

    expect(Exit.isSuccess(exit)).toBe(true);
    // The ledger is the assertion. A preflight GET or a DELETE+POST pair would
    // each reopen the window this command exists to close.
    expect(sent.map((s) => `${s.method} ${s.path}`)).toEqual([`PUT ${ITEM_PATH}`]);
    expect(sent[0]?.body).toEqual({ data: { token: NEW_TOKEN } });
    expect(captured.join("\n")).toMatch(/updated/i);
  });

  test("the typed custom-endpoint flags compose the same replacement body create composes", async () => {
    const captured: string[] = [];
    const sent: Sent[] = [];
    const exit = await run(
      secretUpdateEffectFromFlags({
        name: SECRET_NAME,
        provider: "openai-compatible",
        baseUrl: "https://gw.example.com/v1",
        model: "kimi-k2.6",
        apiKey: NEW_TOKEN,
      }),
      captured,
      sent,
    );

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(sent[0]?.body).toEqual({
      data: {
        provider: "openai-compatible",
        base_url: "https://gw.example.com/v1",
        model: "kimi-k2.6",
        api_key: NEW_TOKEN,
      },
    });
  });
});

describe("test_secret_update_body_sources_and_validation", () => {
  test("an absent --data (and no typed flags) fails before anything is sent", async () => {
    const sent: Sent[] = [];
    const exit = await run(secretUpdateEffectFromFlags({ name: SECRET_NAME }), [], sent);
    expect(Exit.isFailure(exit)).toBe(true);
    expect(sent).toEqual([]);
  });

  test("an absent name fails before anything is sent", async () => {
    const sent: Sent[] = [];
    const exit = await run(secretUpdateEffectFromFlags({ data: '{"k":"v"}' }), [], sent);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      expect((err as ValidationError).suggestion).toMatch(/secret update/);
    }
    expect(sent).toEqual([]);
  });

  test("a non-object --data fails before anything is sent", async () => {
    const sent: Sent[] = [];
    const exit = await run(
      secretUpdateEffectFromFlags({ name: SECRET_NAME, data: '"just-a-string"' }),
      [],
      sent,
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(sent).toEqual([]);
  });

  test("--data=@- reads the replacement body from stdin", async () => {
    await withStdin(`{"token":"${NEW_TOKEN}"}`, async () => {
      const sent: Sent[] = [];
      const exit = await run(
        secretUpdateEffectFromFlags({ name: SECRET_NAME, data: "@-" }),
        [],
        sent,
      );
      expect(Exit.isSuccess(exit)).toBe(true);
      expect(sent[0]?.body).toEqual({ data: { token: NEW_TOKEN } });
    });
  });

  test("--data together with the typed flags is rejected, and nothing is sent", async () => {
    const sent: Sent[] = [];
    const exit = await run(
      secretUpdateEffectFromFlags({ name: SECRET_NAME, data: '{"k":"v"}', provider: "anthropic", apiKey: "x", model: "m" }),
      [],
      sent,
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(sent).toEqual([]);
  });
});

describe("test_secret_update_output_modes_omit_secret", () => {
  test("JSON mode emits status and name only — never the body", async () => {
    const captured: string[] = [];
    const sent: Sent[] = [];
    const exit = await run(
      secretUpdateEffectFromFlags({ name: SECRET_NAME, data: `{"token":"${NEW_TOKEN}"}` }),
      captured,
      sent,
      true,
    );

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(JSON.parse(captured[0] ?? "{}")).toEqual({ status: "updated", name: SECRET_NAME });
    expect(captured.join("\n")).not.toContain(NEW_TOKEN);
  });

  test("human mode prints no secret bytes either", async () => {
    const captured: string[] = [];
    await run(
      secretUpdateEffectFromFlags({ name: SECRET_NAME, data: `{"token":"${NEW_TOKEN}"}` }),
      captured,
      [],
    );
    expect(captured.join("\n")).not.toContain(NEW_TOKEN);
  });
});

describe("test_secret_update_renders_missing_secret", () => {
  test("a UZ-VAULT-003 from the route fails the command after exactly one attempt", async () => {
    const captured: string[] = [];
    const sent: Sent[] = [];
    const exit = await run(
      secretUpdateEffectFromFlags({ name: SECRET_NAME, data: `{"token":"${NEW_TOKEN}"}` }),
      captured,
      sent,
      false,
      () =>
        Effect.fail(
          new ServerError({
            status: 404,
            code: "UZ-VAULT-003",
            detail: "secret not found in this workspace",
            suggestion: "list available with: agentsfleet secret list",
            requestId: null,
          }),
        ),
    );

    expect(Exit.isFailure(exit)).toBe(true);
    // The PUT was attempted exactly once; a failure must not become a retry
    // loop against a credential endpoint.
    expect(sent).toHaveLength(1);
    expect(captured.join("\n")).not.toMatch(/updated/i);
  });
});
