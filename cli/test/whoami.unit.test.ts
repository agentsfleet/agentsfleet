// `whoami` — the render, the refusal, and the two ways a server can disappoint.
//
// Same pattern as the auth suite beside it: compose the command Effect with
// in-memory layers, run to an Exit, assert on the Exit and on what the Output
// service captured. The handler is pure, so nothing is mocked at module level.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { whoamiEffect } from "../src/commands/whoami.ts";
import { USERS_ME_PATH } from "../src/lib/api-paths.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output, OUTPUT_FORMAT, type OutputFormat } from "../src/services/output.ts";
import { AuthError, ServerError, type CliError } from "../src/errors/index.ts";

const API_URL = "https://api.test.local";
const FILE_TOKEN = "afc_from_disk";
const ENV_TOKEN = "agt_t_from_env";

const IDENTITY = {
  user_id: "0193c5e1-0000-7000-8000-00000000abcd",
  email: "ada@example.com",
  display_name: "Ada Lovelace",
  tenant_id: "0193c5e0-0000-7000-8000-000000001234",
  tenant_name: "Ada's Workshop",
  credential: "cli_credential",
  scopes: ["fleet:read", "secret:read"],
} as const;

interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly paths: string[];
  readonly tokens: string[];
}

const makeRecorder = (): Recorder => ({ stdout: [], stderr: [], paths: [], tokens: [] });

const outputLayer = (
  rec: Recorder,
  format: OutputFormat = OUTPUT_FORMAT.text,
): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    format,
    intro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    info: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    success: (msg, data) =>
      Effect.sync(() =>
        rec.stdout.push(
          format === OUTPUT_FORMAT.json
            ? JSON.stringify(data ?? { message: msg })
            : `ok: ${msg}`,
        ),
      ),
    warn: (msg) => Effect.sync(() => rec.stderr.push(`warn: ${msg}`)),
    error: (msg) => Effect.sync(() => rec.stderr.push(`error: ${msg}`)),
    outro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    printJson: (payload) => Effect.sync(() => rec.stdout.push(JSON.stringify(payload))),
    printJsonErr: (payload) => Effect.sync(() => rec.stderr.push(JSON.stringify(payload))),
    printKeyValue: (record) =>
      Effect.sync(() => {
        for (const [key, value] of Object.entries(record)) rec.stdout.push(`${key}: ${value}`);
      }),
    printSection: (title) => Effect.sync(() => rec.stdout.push(`# ${title}`)),
    printTable: (_columns, rows) =>
      Effect.sync(() => {
        for (const row of rows) rec.stdout.push(JSON.stringify(row));
      }),
  });

const credentialsLayer = (token: string | null): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() =>
      token === null ? Option.none() : Option.some(Redacted.make(token)),
    ),
    snapshot: Effect.sync(() => ({
      accessToken: token === null ? Option.none() : Option.some(Redacted.make(token)),
      savedAt: null,
      sessionId: null,
      apiUrl: null,
      credentialId: null,
    })),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  } as unknown as Credentials);

const httpLayer = (
  rec: Recorder,
  responder: () => Effect.Effect<unknown, ServerError>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input: { path: string; token?: Redacted.Redacted<string> }) => {
      rec.paths.push(input.path);
      if (input.token) rec.tokens.push(Redacted.value(input.token));
      return responder() as Effect.Effect<never, ServerError>;
    },
  } as unknown as HttpClient);

const configLayer = (
  jsonMode: boolean,
  envToken: string | null = null,
): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: API_URL,
    dashboardUrl: "https://dash.test.local",
    accessToken: envToken === null ? Option.none() : Option.some(Redacted.make(envToken)),
    jsonMode,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  } as unknown as CliConfig);

interface RunInput {
  readonly jsonMode?: boolean;
  readonly fileToken?: string | null;
  readonly envToken?: string | null;
  readonly responder?: () => Effect.Effect<unknown, ServerError>;
}

const run = async (
  input: RunInput = {},
): Promise<{ exit: Exit.Exit<void, CliError>; rec: Recorder }> => {
  const rec = makeRecorder();
  const responder = input.responder ?? (() => Effect.succeed(IDENTITY));
  const exit = await Effect.runPromiseExit(
    whoamiEffect.pipe(
      Effect.provide(
        Layer.mergeAll(
          outputLayer(
            rec,
            input.jsonMode === true ? OUTPUT_FORMAT.json : OUTPUT_FORMAT.text,
          ),
          credentialsLayer(input.fileToken === undefined ? FILE_TOKEN : input.fileToken),
          httpLayer(rec, responder),
          configLayer(input.jsonMode ?? false, input.envToken ?? null),
        ),
      ),
    ) as Effect.Effect<void, CliError, never>,
  );
  return { exit, rec };
};

const failureOf = (exit: Exit.Exit<void, CliError>): CliError | null =>
  Exit.isFailure(exit) ? Option.getOrNull(Cause.findErrorOption(exit.cause)) : null;

describe("whoamiEffect", () => {
  test("renders the person, the tenant and the credential class", async () => {
    const { exit, rec } = await run();

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout).toContain("# Identity");
    expect(rec.stdout).toContain("email: ada@example.com");
    expect(rec.stdout).toContain("name: Ada Lovelace");
    expect(rec.stdout).toContain(
      `tenant: Ada's Workshop (${IDENTITY.tenant_id})`,
    );
    expect(rec.stdout).toContain("credential: command-line credential (this machine)");
    expect(rec.stdout).toContain(`api_url: ${API_URL}`);
    expect(rec.stdout).toContain("scopes: fleet:read, secret:read");
  });

  test("reads the identity route and sends the stored credential", async () => {
    const { rec } = await run();

    expect(rec.paths).toEqual([USERS_ME_PATH]);
    expect(rec.tokens).toEqual([FILE_TOKEN]);
  });

  test("an exported service key wins over the credential on disk", async () => {
    const { rec } = await run({ envToken: ENV_TOKEN });

    expect(rec.tokens).toEqual([ENV_TOKEN]);
  });

  test("a missing display name renders the placeholder, never a guess", async () => {
    const { display_name: _absent, ...rest } = IDENTITY;
    const { rec } = await run({ responder: () => Effect.succeed(rest) });

    expect(rec.stdout).toContain("name: —");
    expect(rec.stdout.some((line) => line.startsWith("name: Ada"))).toBe(false);
  });

  test("a person holding no capability still reads their own identity", async () => {
    const { exit, rec } = await run({
      responder: () => Effect.succeed({ ...IDENTITY, scopes: [] }),
    });

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout).toContain("scopes: none");
  });

  test("an unrecognised credential class renders as itself", async () => {
    const { rec } = await run({
      responder: () => Effect.succeed({ ...IDENTITY, credential: "future_class" }),
    });

    expect(rec.stdout).toContain("credential: future_class");
  });

  test("--json prints the server's fields plus the resolved target", async () => {
    const { exit, rec } = await run({ jsonMode: true });

    expect(Exit.isSuccess(exit)).toBe(true);
    const payload = JSON.parse(rec.stdout[0] ?? "{}") as Record<string, unknown>;
    expect(payload["authenticated"]).toBe(true);
    expect(payload["email"]).toBe(IDENTITY.email);
    expect(payload["tenant_name"]).toBe(IDENTITY.tenant_name);
    expect(payload["credential"]).toBe(IDENTITY.credential);
    expect(payload["api_url"]).toBe(API_URL);
  });

  test("nothing signed in refuses locally, naming login, with no request sent", async () => {
    const { exit, rec } = await run({ fileToken: null });

    expect(failureOf(exit)).toBeInstanceOf(AuthError);
    expect(rec.paths).toEqual([]);
    expect(rec.stderr.join("\n")).toContain("agentsfleet login");
  });

  test("nothing signed in answers JSON when asked for JSON", async () => {
    const { exit, rec } = await run({ fileToken: null, jsonMode: true });

    expect(Exit.isFailure(exit)).toBe(true);
    const payload = JSON.parse(rec.stdout[0] ?? "{}") as Record<string, unknown>;
    expect(payload["authenticated"]).toBe(false);
    expect(payload["api_url"]).toBe(API_URL);
  });

  test("a server refusal fails rather than rendering a partial identity", async () => {
    const { exit, rec } = await run({
      responder: () =>
        Effect.fail(
          new ServerError({
            detail: "Authenticated subject has no user record",
            suggestion: "sign in again",
            code: "UZ-AUTH-001",
            status: 403,
            requestId: "req_xyz",
          }),
        ),
    });

    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.stdout).toEqual([]);
  });

  test("a deployment without the route says so, not 'check the payload'", async () => {
    const { exit, rec } = await run({
      responder: () =>
        Effect.fail(
          new ServerError({
            detail: "",
            suggestion: "verify the request payload and retry",
            code: "HTTP_404",
            status: 404,
            requestId: null,
          }),
        ),
    });

    expect(Exit.isFailure(exit)).toBe(true);
    const failure = failureOf(exit) as InstanceType<typeof ServerError>;
    // The generic 404 sentence sends the reader after a request body this
    // command never sends. A router matches a path before any guard runs, so
    // the honest report is that the deployment is older than the client.
    expect(failure.detail).toContain("older than this client");
    expect(failure.suggestion).toContain("AGENTSFLEET_API_URL");
    expect(rec.stdout).toEqual([]);
  });

  test("a 200 carrying no readable identity renders nothing", async () => {
    const { exit, rec } = await run({ responder: () => Effect.succeed({ email: 1 }) });

    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.stdout).toEqual([]);
  });
});
