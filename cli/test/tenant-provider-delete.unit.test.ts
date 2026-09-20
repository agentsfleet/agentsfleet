// `tenant provider delete` — reverting to the platform default, plus the low
// balance warning it reads on the way out. The billing snapshot is best
// effort: its failure must not turn a completed delete into a failed one.

import { describe, test, expect } from "bun:test";
import { Effect, Exit } from "effect";
import { tenantProviderDeleteEffect } from "../src/commands/tenant.ts";
import { OUTPUT_FORMAT } from "../src/services/output.ts";
import { ServerError } from "../src/errors/index.ts";
import { PROVIDER_MODE } from "../src/constants/billing.ts";
import {
  TENANT_PROVIDER_PATH,
  TENANT_BILLING_PATH,
  ONE_CENT_NANOS,
  makeRecorder,
  outputLayer,
  credentialsLayer,
  httpClientLayer,
  configLayer,
  runWith,
} from "./helpers-tenant-effect.ts";

describe("tenantProviderDeleteEffect", () => {
  test("DELETEs and warns on low balance", async () => {
    const rec = makeRecorder();
    const program = tenantProviderDeleteEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === TENANT_PROVIDER_PATH) {
            return Effect.succeed({
              mode: PROVIDER_MODE.platform,
              provider: "fireworks",
              model: "kimi-k2.6",
              context_cap_tokens: 256000,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({
            balance_nanos: 42 * ONE_CENT_NANOS,
          }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.httpCalls.map((c) => `${c.method} ${c.path}`)).toEqual([
      `DELETE ${TENANT_PROVIDER_PATH}`,
      `GET ${TENANT_BILLING_PATH}`,
    ]);
    expect(
      rec.stderr.some((line) =>
        /Tenant balance is low: \$0\.42/.test(line),
      ),
    ).toBe(true);
  });

  test("high balance suppresses warning", async () => {
    const rec = makeRecorder();
    const program = tenantProviderDeleteEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === TENANT_PROVIDER_PATH) {
            return Effect.succeed({
              mode: PROVIDER_MODE.platform,
              provider: "fireworks",
              model: "kimi-k2.6",
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({
            balance_nanos: 999 * ONE_CENT_NANOS,
          }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stderr.some((line) => /Tenant balance is low/.test(line)),
    ).toBe(false);
  });

  test("billing snapshot failure does not break delete success path", async () => {
    const rec = makeRecorder();
    const program = tenantProviderDeleteEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === TENANT_PROVIDER_PATH) {
            return Effect.succeed({
              mode: PROVIDER_MODE.platform,
              provider: "fireworks",
              model: "kimi-k2.6",
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.fail(
            new ServerError({
              detail: "boom",
              suggestion: "retry",
              code: "INTERNAL_ERROR",
              status: 500,
              requestId: null,
            }),
          ) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(
      rec.stdout.some((line) =>
        /Custom LLM provider removed/.test(line),
      ),
    ).toBe(true);
  });

  test("--json mode prints raw response", async () => {
    const rec = makeRecorder();
    const payload = {
      mode: PROVIDER_MODE.platform,
      provider: "fireworks",
      model: "kimi-k2.6",
    };
    const program = tenantProviderDeleteEffect.pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          (path) => {
            if (path === TENANT_PROVIDER_PATH) {
              return Effect.succeed(payload) as Effect.Effect<unknown, ServerError>;
            }
            return Effect.succeed({
              balance_nanos: 999 * ONE_CENT_NANOS,
            }) as Effect.Effect<unknown, ServerError>;
          },
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
    );
    await runWith(program);
    expect(rec.stdout[0]).toBe(JSON.stringify(payload));
  });
});
