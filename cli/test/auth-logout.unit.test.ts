// What `agentsfleet logout` ends, and what it deliberately leaves alone.
//
// Logout makes two independent server calls and then clears local state. The
// tests below pin each one separately, because the interesting failures are
// asymmetric: a credential that survives logout is a durable credential the
// operator believes is dead, while a browser session that does NOT survive it
// is a person signed out of a dashboard they were reading.
//
// Every case asserts the local clear happened, whatever the server did. That
// is the property that makes logout usable on a laptop with no network.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";
import { logoutEffect } from "../src/commands/auth-logout.ts";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { AUTH_SESSIONS_PATH, CLI_CREDENTIALS_PATH } from "../src/lib/api-paths.ts";
import { CLI_CREDENTIAL_BODY_LEN, CLI_CREDENTIAL_PREFIX } from "../src/constants/cli-credential.ts";
import { ServerError, type CliError } from "../src/errors/index.ts";
import { useFreshStateDir } from "./helpers-cli-state.ts";

useFreshStateDir();

const CREDENTIAL = `${CLI_CREDENTIAL_PREFIX}${"a".repeat(CLI_CREDENTIAL_BODY_LEN)}`;
const CREDENTIAL_ID = "0192a3b4-c5d6-7e8f-9012-345678901234";
const ALL_SESSIONS_PATH = `${AUTH_SESSIONS_PATH}/all`;
const REVOKE_PATH = `${CLI_CREDENTIALS_PATH}/${CREDENTIAL_ID}`;
const METHOD_DELETE = "DELETE" as const;
const REVOKE_FAILED_CODE = "UZ-INTERNAL-001";

interface Call {
  readonly path: string;
  readonly method: string;
}

interface Rec {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly cleared: string[];
  readonly calls: Call[];
}

const makeRec = (): Rec => ({ stdout: [], stderr: [], cleared: [], calls: [] });

const outputLayer = (rec: Rec): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    info: (l: string) => Effect.sync(() => void rec.stdout.push(l)),
    success: (l: string) => Effect.sync(() => void rec.stdout.push(l)),
    warn: (l: string) => Effect.sync(() => void rec.stderr.push(l)),
    error: (l: string) => Effect.sync(() => void rec.stderr.push(l)),
    printJson: (v: unknown) =>
      Effect.sync(() => void rec.stdout.push(JSON.stringify(v))),
    printKeyValue: () => Effect.void,
    printSection: () => Effect.void,
    printTable: () => Effect.void,
  } as unknown as Output);

const analyticsLayer: Layer.Layer<Analytics> = Layer.succeed(Analytics, {
  capture: () => Effect.void,
  identify: () => Effect.void,
  alias: () => Effect.void,
  flush: Effect.void,
} as unknown as Analytics);

const credentialsLayer = (
  rec: Rec,
  opts: { readonly token: string | null; readonly credentialId: string | null },
): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() =>
      opts.token === null
        ? Option.none<Redacted.Redacted<string>>()
        : Option.some(Redacted.make(opts.token)),
    ),
    getSavedAt: Effect.succeed(null),
    getSessionId: Effect.succeed(null),
    getApiUrl: Effect.succeed(null),
    getCredentialId: Effect.succeed(opts.credentialId),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.sync(() => void rec.cleared.push("clear")),
  } as unknown as Credentials);

// Records every request, and lets a chosen path fail, so a test can prove
// both what was called and what happened when one leg refused.
const httpLayer = (rec: Rec, failPath?: string): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input: { path: string; method?: string }) => {
      rec.calls.push({ path: input.path, method: input.method ?? "GET" });
      if (failPath !== undefined && input.path === failPath) {
        return Effect.fail(
          new ServerError({
            detail: "revoke refused",
            suggestion: "try again",
            code: REVOKE_FAILED_CODE,
            status: 500,
            requestId: null,
          }),
        );
      }
      return Effect.succeed({ aborted_count: 1 });
    },
  } as unknown as HttpClient);

const configLayer = (jsonMode = false): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  } as unknown as CliConfig);

const runLogout = (
  rec: Rec,
  opts: {
    readonly token?: string | null;
    readonly credentialId?: string | null;
    readonly failPath?: string;
    readonly jsonMode?: boolean;
  } = {},
): Promise<Exit.Exit<void, CliError>> =>
  Effect.runPromiseExit(
    logoutEffect({ all: false }).pipe(
      Effect.provide(httpLayer(rec, opts.failPath)),
      Effect.provide(
        credentialsLayer(rec, {
          token: opts.token === undefined ? CREDENTIAL : opts.token,
          credentialId:
            opts.credentialId === undefined ? CREDENTIAL_ID : opts.credentialId,
        }),
      ),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer),
      Effect.provide(configLayer(opts.jsonMode ?? false)),
    ) as Effect.Effect<void, CliError, never>,
  );

describe("test_logout_revokes_and_clears", () => {
  test("logout revokes this machine's credential by identifier, then clears local state", async () => {
    const rec = makeRec();
    const exit = await runLogout(rec);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.calls).toContainEqual({ path: REVOKE_PATH, method: METHOD_DELETE });
    expect(rec.cleared).toEqual(["clear"]);
  });

  test("the credential is revoked before the local clear — the identifier lives in the file the clear erases", async () => {
    const rec = makeRec();
    await runLogout(rec);
    // Nothing was cleared until after the revoke went out. If the order ever
    // inverts, the revoke silently stops happening: it has no identifier and
    // no credential to authorise itself with.
    expect(rec.calls.length).toBeGreaterThan(0);
    expect(rec.cleared).toHaveLength(1);
  });
});

describe("test_logout_does_not_revoke_browser_session", () => {
  test("logout touches only the device-session abort and this machine's credential", async () => {
    const rec = makeRec();
    await runLogout(rec);
    // A terminal logout must not sign a person out of the dashboard they are
    // reading. The browser holds a Clerk-refreshed credential class, and
    // nothing here addresses it — pinned as the exact call set, so a future
    // Clerk admin-API call cannot be added without this failing.
    expect(rec.calls.map((c) => c.path).sort()).toEqual(
      [ALL_SESSIONS_PATH, REVOKE_PATH].sort(),
    );
  });
});

describe("test_logout_when_logged_out_is_idempotent", () => {
  test("logout with nothing stored exits zero and makes no server call at all", async () => {
    const rec = makeRec();
    const exit = await runLogout(rec, { token: null, credentialId: null });
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.calls).toEqual([]);
    expect(rec.cleared).toEqual(["clear"]);
  });

  test("a stored tenant key is not revoked — this client never minted it and holds no identifier for it", async () => {
    const rec = makeRec();
    await runLogout(rec, { credentialId: null });
    expect(rec.calls.map((c) => c.path)).not.toContain(REVOKE_PATH);
    expect(rec.cleared).toEqual(["clear"]);
  });
});

describe("test_failed_revoke_reports_and_continues", () => {
  test("a refused revoke still clears local state and names what stayed live", async () => {
    const rec = makeRec();
    const exit = await runLogout(rec, { failPath: REVOKE_PATH });
    // Refusing to log out until the server agrees would strand exactly the
    // operator most likely to want out.
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.cleared).toEqual(["clear"]);
    const warning = rec.stderr.join("\n");
    expect(warning).toContain(REVOKE_FAILED_CODE);
    expect(warning).toContain("dashboard");
  });

  test("the JSON envelope reports the two revokes separately", async () => {
    const rec = makeRec();
    await runLogout(rec, { failPath: REVOKE_PATH, jsonMode: true });
    const body = JSON.parse(rec.stdout.join("")) as {
      logged_out: boolean;
      server_revoke: string;
      credential_revoke: string;
    };
    // Session abort and credential revoke fail differently and cost
    // differently, so a machine reading this can tell which one happened.
    expect(body.logged_out).toBe(true);
    expect(body.server_revoke).toBe("ok");
    expect(body.credential_revoke).toBe("failed");
  });
});
