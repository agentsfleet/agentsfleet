// Effect-shaped billing handler tests. Mirrors auth-effect / workspace-effect:
// compose the command Effect with in-memory layers and assert on Exit +
// captured side-effects.

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { billingShowEffectFromArgs } from "../src/commands/billing.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import {
  ServerError,
  ValidationError,
  type CliError,
} from "../src/errors/index.ts";
import {
  CHARGE_TYPE,
  NANOS_PER_USD,
  PROVIDER_MODE,
} from "../src/constants/billing.ts";

const BILLING_PATH = "/v1/tenants/me/billing";
const CHARGES_PATH_PREFIX = "/v1/tenants/me/billing/charges";
const ONE_CENT_NANOS = NANOS_PER_USD / 100;

interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly httpCalls: string[];
}

const makeRecorder = (): Recorder => ({ stdout: [], stderr: [], httpCalls: [] });

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
    printTable: (_columns, rows) =>
      Effect.sync(() => {
        rec.stdout.push(`TABLE:${rows.length}`);
      }),
  });

const credentialsLayer = (): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(Option.some(Redacted.make("test-token"))),
    getSavedAt: Effect.succeed(null),
    getSessionId: Effect.succeed(null),
    getApiUrl: Effect.succeed(null),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

const httpClientLayer = (
  responder: (path: string) => Effect.Effect<unknown, ServerError>,
  rec: Recorder,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input) => {
      rec.httpCalls.push(input.path);
      return responder(input.path) as Effect.Effect<never, ServerError | never>;
    },
  });

const configLayer = (
  overrides: Partial<{ jsonMode: boolean }> = {},
): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode: overrides.jsonMode ?? false,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

const runWith = <E extends CliError>(
  effect: Effect.Effect<void, E, never>,
): Promise<Exit.Exit<void, E>> => Effect.runPromiseExit(effect);

const expectFailure = <E extends CliError>(
  exit: Exit.Exit<void, E>,
): E => {
  if (Exit.isSuccess(exit)) throw new Error("expected failure");
  const failure = Option.getOrNull(Cause.findErrorOption(exit.cause));
  if (failure === null) throw new Error("no typed failure in cause");
  return failure;
};

const RECEIVE_ROW = {
  event_id: "evt_1",
  charge_type: CHARGE_TYPE.receive,
  posture: PROVIDER_MODE.platform,
  model: "kimi-k2.6",
  credit_deducted_nanos: ONE_CENT_NANOS,
  token_count_input: null,
  token_count_output: null,
  recorded_at: 1_000_000,
};
const STAGE_ROW = {
  event_id: "evt_1",
  charge_type: CHARGE_TYPE.stage,
  posture: PROVIDER_MODE.platform,
  model: "kimi-k2.6",
  credit_deducted_nanos: 2 * ONE_CENT_NANOS,
  token_count_input: 820,
  token_count_output: 1040,
  recorded_at: 1_000_005,
};

describe("billingShowEffectFromArgs", () => {
  test("GETs balance + charges with default limit=10 → charges limit=20", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: undefined,
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 471 * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({ items: [] }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.httpCalls.sort()).toEqual(
      [BILLING_PATH, `${CHARGES_PATH_PREFIX}?limit=20`].sort(),
    );
  });

  test("--limit 5 charges path uses limit=10 (limit*2)", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: "5",
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 100 * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({ items: [] }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.httpCalls).toContain(`${CHARGES_PATH_PREFIX}?limit=10`);
  });

  test("rejects --limit 0 with ValidationError", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: "0",
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(() => Effect.succeed({}) as Effect.Effect<unknown, ServerError>, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(failure.message).toMatch(/--limit must be an integer/);
  });

  test("rejects non-numeric --limit", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: "lots",
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(() => Effect.succeed({}) as Effect.Effect<unknown, ServerError>, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
  });

  test("rejects --limit above max", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: "9999",
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(() => Effect.succeed({}) as Effect.Effect<unknown, ServerError>, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
  });

  test("rejects empty --cursor", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: undefined,
      cursor: "",
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(() => Effect.succeed({}) as Effect.Effect<unknown, ServerError>, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(failure.message).toMatch(/--cursor must not be empty/);
  });

  test("forwards --cursor URI-encoded to charges endpoint", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: undefined,
      cursor: "abc/=def",
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 100 * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({ items: [] }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.httpCalls.some((u) => u.includes("cursor=abc%2F%3Ddef"))).toBe(true);
  });

  test("text mode renders balance, table, and footer pointer", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: undefined,
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 471 * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({ items: [] }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout.join("\n")).toMatch(/Tenant balance: {4}\$4\.71/);
    expect(rec.stdout.join("\n")).toMatch(/No billable events recorded yet\./);
    expect(rec.stdout.join("\n")).toMatch(/Out of credits\? See /);
  });

  test("exhausted balance surfaces explicit warning on stderr", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: undefined,
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 0,
              is_exhausted: true,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({ items: [] }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stderr.some((line) => /⚠ Out of credits\. See /.test(line)),
    ).toBe(true);
  });

  test("groups receive+stage rows by event_id and emits the table", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: undefined,
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 500 * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({
            items: [STAGE_ROW, RECEIVE_ROW],
          }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    const out = rec.stdout.join("\n");
    expect(out).toMatch(/Last 1 events drained credits:/);
    expect(out).toMatch(/TABLE:1/);
  });

  test("--json emits balance + grouped events + next_cursor", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: undefined,
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 250 * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({
            items: [RECEIVE_ROW, STAGE_ROW],
            next_cursor: "tok_for_page_2",
          }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    const body = JSON.parse(rec.stdout[0] ?? "{}") as {
      balance_nanos: number;
      is_exhausted: boolean;
      events: Array<{
        event_id: string;
        receive_nanos: number;
        stage_nanos: number;
        total_nanos: number;
        token_count_input: number;
        token_count_output: number;
      }>;
      next_cursor: string | null;
    };
    expect(body.balance_nanos).toBe(250 * ONE_CENT_NANOS);
    expect(body.is_exhausted).toBe(false);
    expect(body.events).toHaveLength(1);
    const ev = body.events[0];
    if (!ev) throw new Error("expected grouped event");
    expect(ev.event_id).toBe("evt_1");
    expect(ev.receive_nanos).toBe(ONE_CENT_NANOS);
    expect(ev.stage_nanos).toBe(2 * ONE_CENT_NANOS);
    expect(ev.total_nanos).toBe(3 * ONE_CENT_NANOS);
    expect(ev.token_count_input).toBe(820);
    expect(ev.token_count_output).toBe(1040);
    expect(body.next_cursor).toBe("tok_for_page_2");
  });

  test("--limit slices grouped events not raw rows", async () => {
    const rec = makeRecorder();
    const items: Array<Record<string, unknown>> = [];
    for (const eid of ["evt_a", "evt_b", "evt_c"]) {
      items.push({ ...RECEIVE_ROW, event_id: eid, recorded_at: items.length });
      items.push({ ...STAGE_ROW, event_id: eid, recorded_at: items.length });
    }
    const program = billingShowEffectFromArgs({
      limit: "2",
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 1000 * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({ items }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    const body = JSON.parse(rec.stdout[0] ?? "{}") as {
      events: Array<unknown>;
    };
    expect(body.events).toHaveLength(2);
  });

  test("surfaces next_cursor in text mode footer", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: undefined,
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 100 * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({
            items: [RECEIVE_ROW, STAGE_ROW],
            next_cursor: "next_token_xyz",
          }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout.join("\n")).toMatch(
      /more events available — re-run with --cursor next_token_xyz/,
    );
  });
});
