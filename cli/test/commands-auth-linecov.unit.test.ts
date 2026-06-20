// Line-coverage backfill for src/commands/auth.ts `deriveTokenSummary`
// (the token-claim summariser, src lines 61-85). It is not exported, so
// it is reached through `authStatusEffect`: when the active token decodes
// to a real JWT payload, the summary fields (iss/aud/sub/tenant_id/role/
// exp/expired) are populated and surfaced via printJson / printKeyValue.
//
// The sibling suite (auth-effect.unit.test.ts) drives authStatusEffect
// with `Redacted.make("test-token")`, which decodes to null and bails at
// the early return — so the body never runs. These tests feed real
// base64url JWTs and assert the derived summary values, exercising both
// sides of each branch (metadata-vs-top-level, exp-present-vs-absent,
// typed-vs-mistyped claims).

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";
import { authStatusEffect } from "../src/commands/auth.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";

const API_URL = "https://api.test.local";
const FIXED_SAVED_AT = 1700000000000;
const SESSION_ID = "sess-cov";

// A far-future / far-past second-resolution epoch for exp claims.
const FUTURE_EXP_SEC = 4102444800; // 2100-01-01
const PAST_EXP_SEC = 1000000000; // 2001-09-09

// Forge an unsigned JWT (`alg: none`) carrying `payload` as the body.
// The CLI never verifies signatures, so a placeholder sig is fine.
const makeJwt = (payload: Record<string, unknown>): string => {
  const header = Buffer.from(
    JSON.stringify({ alg: "none", typ: "JWT" }),
  ).toString("base64url");
  const body = Buffer.from(JSON.stringify(payload)).toString("base64url");
  return `${header}.${body}.sig`;
};

interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
}

const makeRecorder = (): Recorder => ({ stdout: [], stderr: [] });

const outputLayer = (rec: Recorder): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    info: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    success: (msg) => Effect.sync(() => rec.stdout.push(`ok: ${msg}`)),
    warn: (msg) => Effect.sync(() => rec.stderr.push(`warn: ${msg}`)),
    error: (msg) => Effect.sync(() => rec.stderr.push(`error: ${msg}`)),
    outro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    printJson: (payload) =>
      Effect.sync(() => rec.stdout.push(JSON.stringify(payload))),
    printJsonErr: (payload) =>
      Effect.sync(() => rec.stderr.push(JSON.stringify(payload))),
    printKeyValue: (record) =>
      Effect.sync(() => {
        for (const [k, v] of Object.entries(record)) {
          rec.stdout.push(`  ${k}: ${v}`);
        }
      }),
    printSection: (title) => Effect.sync(() => rec.stdout.push(`# ${title}`)),
    printTable: (_columns, rows) =>
      Effect.sync(() => {
        for (const row of rows) rec.stdout.push(JSON.stringify(row));
      }),
  });

const credentialsLayer = (
  token: Option.Option<Redacted.Redacted<string>>,
): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(token),
    getSavedAt: Effect.succeed(FIXED_SAVED_AT),
    getSessionId: Effect.succeed(SESSION_ID),
    getApiUrl: Effect.succeed(API_URL),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

const configLayer = (jsonMode: boolean): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: API_URL,
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

// Probe always succeeds → status "valid", so authStatusEffect proceeds to
// build the AuthStatusResult (calling deriveTokenSummary) and prints it.
const okHttpLayer: Layer.Layer<HttpClient> = Layer.succeed(HttpClient, {
  request: () => Effect.succeed({} as never),
});

// Run authStatusEffect in jsonMode so the derived summary is emitted
// verbatim as one JSON line, returning {exit, json}.
const runJson = async (
  jwt: string,
): Promise<{ exit: Exit.Exit<void, unknown>; json: Record<string, unknown> }> => {
  const rec = makeRecorder();
  const exit = await Effect.runPromiseExit(
    authStatusEffect.pipe(
      Effect.provide(configLayer(true)),
      Effect.provide(credentialsLayer(Option.some(Redacted.make(jwt)))),
      Effect.provide(okHttpLayer),
      Effect.provide(outputLayer(rec)),
    ),
  );
  const line = rec.stdout.find((l) => l.startsWith("{")) ?? "{}";
  return { exit, json: JSON.parse(line) as Record<string, unknown> };
};

const tokenOf = (json: Record<string, unknown>): Record<string, unknown> =>
  json["token"] as Record<string, unknown>;

describe("authStatusEffect token summary derivation", () => {
  test("populates iss/aud/sub and a future expiry as not-expired", async () => {
    const { exit, json } = await runJson(
      makeJwt({
        iss: "https://issuer.test",
        aud: "fleet-cli",
        sub: "user_42",
        exp: FUTURE_EXP_SEC,
      }),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    const t = tokenOf(json);
    expect(t["iss"]).toBe("https://issuer.test");
    expect(t["aud"]).toBe("fleet-cli");
    expect(t["sub"]).toBe("user_42");
    expect(t["exp_at"]).toBe(new Date(FUTURE_EXP_SEC * MS_PER_SECOND).toISOString());
    expect(t["expired"]).toBe(false);
  });

  test("flags a past expiry as expired", async () => {
    const { json } = await runJson(makeJwt({ sub: "u", exp: PAST_EXP_SEC }));
    const t = tokenOf(json);
    expect(t["expired"]).toBe(true);
    expect(t["exp_at"]).toBe(new Date(PAST_EXP_SEC * MS_PER_SECOND).toISOString());
  });

  test("nulls exp_at/expired when exp is absent or non-finite", async () => {
    const { json } = await runJson(makeJwt({ sub: "u", exp: Infinity }));
    const t = tokenOf(json);
    // Infinity serialises to null in JSON, so payload.exp is not a finite
    // number → expSec null → exp_at/expired both null.
    expect(t["exp_at"]).toBeNull();
    expect(t["expired"]).toBeNull();
  });

  test("nulls iss/aud/sub when claims are present but mistyped", async () => {
    const { json } = await runJson(
      makeJwt({ iss: 123, aud: ["a", "b"], sub: { nested: true } }),
    );
    const t = tokenOf(json);
    expect(t["iss"]).toBeNull();
    expect(t["aud"]).toBeNull();
    expect(t["sub"]).toBeNull();
  });

  test("prefers metadata.tenant_id and metadata.role over top-level", async () => {
    const { json } = await runJson(
      makeJwt({
        tenant_id: "top_tenant",
        role: "user",
        metadata: { tenant_id: "meta_tenant", role: "admin" },
      }),
    );
    const t = tokenOf(json);
    expect(t["tenant_id"]).toBe("meta_tenant");
    expect(t["role"]).toBe("admin");
  });

  test("falls back to top-level tenant_id/role when metadata is absent", async () => {
    const { json } = await runJson(
      makeJwt({ tenant_id: "top_tenant", role: "operator" }),
    );
    const t = tokenOf(json);
    expect(t["tenant_id"]).toBe("top_tenant");
    expect(t["role"]).toBe("operator");
  });

  test("nulls tenant_id/role when metadata is not an object and top-level is mistyped", async () => {
    const { json } = await runJson(
      makeJwt({ metadata: "not-an-object", tenant_id: 7, role: false }),
    );
    const t = tokenOf(json);
    expect(t["tenant_id"]).toBeNull();
    expect(t["role"]).toBeNull();
  });

  test("surfaces metadata.role/tenant_id through the human key-value renderer", async () => {
    const rec = makeRecorder();
    const jwt = makeJwt({ metadata: { tenant_id: "acme", role: "operator" } });
    const exit = await Effect.runPromiseExit(
      authStatusEffect.pipe(
        Effect.provide(configLayer(false)),
        Effect.provide(credentialsLayer(Option.some(Redacted.make(jwt)))),
        Effect.provide(okHttpLayer),
        Effect.provide(outputLayer(rec)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes("tenant_id: acme"))).toBe(true);
    expect(rec.stdout.some((l) => l.includes("role: operator"))).toBe(true);
    expect(rec.stdout.some((l) => l.includes("ok: authenticated"))).toBe(true);
  });
});

// An opaque api key (AGENTSFLEET_API_KEY) does not decode to a JWT payload, so
// auth status must label it as such instead of emitting empty JWT-claim rows.
describe("authStatusEffect credential kind", () => {
  test("an opaque api key renders as api_key with no JWT claims", async () => {
    const { exit, json } = await runJson("agt_t9f3c_opaque_not_a_jwt");
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(json["credential_kind"]).toBe("api_key");
    expect(json["token"]).toBeNull();
    expect(json["authenticated"]).toBe(true);
  });

  test("the human render shows the api-key line, not dashed JWT claims", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      authStatusEffect.pipe(
        Effect.provide(configLayer(false)),
        Effect.provide(credentialsLayer(Option.some(Redacted.make("agt_t9f3c_opaque")))),
        Effect.provide(okHttpLayer),
        Effect.provide(outputLayer(rec)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes("credential: api key"))).toBe(true);
    expect(rec.stdout.some((l) => l.includes("tenant_id:"))).toBe(false);
    expect(rec.stdout.some((l) => l.includes("ok: authenticated"))).toBe(true);
  });

  test("a decodable JWT is tagged credential_kind jwt", async () => {
    const { json } = await runJson(makeJwt({ sub: "user_1", exp: FUTURE_EXP_SEC }));
    expect(json["credential_kind"]).toBe("jwt");
    expect(tokenOf(json)).not.toBeNull();
  });
});
const MS_PER_SECOND = 1000 as const;
