// `billing show` — what the handler asks the API for, and what it refuses to
// ask. Limit and cursor validation with the plain balance render.

import { describe, test, expect } from "bun:test";
import { Effect, Exit } from "effect";
import { billingShowEffectFromArgs } from "../src/commands/billing.ts";
import { ServerError, ValidationError } from "../src/errors/index.ts";
import {
  BILLING_PATH,
  CHARGES_PATH_PREFIX,
  ONE_CENT_NANOS,
  makeRecorder,
  outputLayer,
  credentialsLayer,
  httpClientLayer,
  configLayer,
  runWith,
  expectFailure,
} from "./helpers-billing-effect.ts";

describe("billingShowEffectFromArgs — request shape and validation", () => {
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
});
