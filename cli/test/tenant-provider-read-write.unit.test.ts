// `tenant provider show` and `tenant provider add` — the read and the write
// that answer the same question, so they read together.

import { describe, test, expect } from "bun:test";
import { Effect, Exit } from "effect";
import { tenantProviderShowEffect, tenantProviderAddEffectFromArgs } from "../src/commands/tenant.ts";
import { OUTPUT_FORMAT } from "../src/services/output.ts";
import { ServerError, ValidationError } from "../src/errors/index.ts";
import { PROVIDER_MODE } from "../src/constants/billing.ts";
import {
  TENANT_PROVIDER_PATH,
  makeRecorder,
  outputLayer,
  credentialsLayer,
  httpClientLayer,
  configLayer,
  runWith,
  expectFailure,
} from "./helpers-tenant-effect.ts";

describe("tenantProviderShowEffect", () => {
  test("GETs provider config and emits table in text mode", async () => {
    const rec = makeRecorder();
    const program = tenantProviderShowEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () =>
            Effect.succeed({
              mode: PROVIDER_MODE.platform,
              provider: "fireworks",
              model: "kimi-k2.6",
              context_cap_tokens: 256000,
              secret_ref: null,
              synthesised_default: true,
            }) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.httpCalls).toEqual([
      { path: TENANT_PROVIDER_PATH, method: "GET", body: null },
    ]);
    expect(rec.stdout.some((line) => line.includes("fireworks"))).toBe(true);
    expect(
      rec.stdout.some((line) => line.includes("platform default")),
    ).toBe(true);
  });

  test("surfaces credential_missing error to stderr while still rendering table", async () => {
    const rec = makeRecorder();
    const program = tenantProviderShowEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () =>
            Effect.succeed({
              mode: PROVIDER_MODE.self_managed,
              provider: "fireworks",
              error: "credential_missing",
              secret_ref: "fw-key",
            }) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stderr.some((line) => /Secret fw-key is missing/.test(line)),
    ).toBe(true);
    expect(
      rec.stdout.some((line) => line.includes("self_managed")),
    ).toBe(true);
  });

  test("surfaces generic provider resolver errors with the credential reference", async () => {
    const rec = makeRecorder();
    const program = tenantProviderShowEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () =>
            Effect.succeed({
              mode: PROVIDER_MODE.self_managed,
              provider: "fireworks",
              error: "provider_unreachable",
              secret_ref: "fw-key",
            }) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stderr.some((line) =>
        /Provider resolver error: provider_unreachable \(secret_ref=fw-key\)/.test(line),
      ),
    ).toBe(true);
  });

  test("--json mode prints raw response and skips warning prose", async () => {
    const rec = makeRecorder();
    const payload = {
      mode: PROVIDER_MODE.self_managed,
      error: "credential_missing",
      secret_ref: "fw-key",
    };
    const program = tenantProviderShowEffect.pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () => Effect.succeed(payload) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
    );
    await runWith(program);
    expect(rec.stdout[0]).toBe(JSON.stringify(payload));
    expect(rec.stderr).toEqual([]);
  });
});

describe("tenantProviderAddEffectFromArgs", () => {
  test("PUTs mode=self_managed with secret_ref and prints tip", async () => {
    const rec = makeRecorder();
    const program = tenantProviderAddEffectFromArgs("fw-key", undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () =>
            Effect.succeed({
              mode: PROVIDER_MODE.self_managed,
              provider: "fireworks",
              model: "kimi-k2.6",
              context_cap_tokens: 256000,
              secret_ref: "fw-key",
            }) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.httpCalls).toHaveLength(1);
    const call = rec.httpCalls[0];
    if (!call) throw new Error("expected one http call");
    expect(call.path).toBe(TENANT_PROVIDER_PATH);
    expect(call.method).toBe("PUT");
    expect(call.body).toEqual({
      mode: PROVIDER_MODE.self_managed,
      secret_ref: "fw-key",
    });
    expect(
      rec.stdout.some((line) =>
        /Tip: run a test event to verify the key works against fireworks/.test(line),
      ),
    ).toBe(true);
  });

  test("--model flag forwards as body.model", async () => {
    const rec = makeRecorder();
    const program = tenantProviderAddEffectFromArgs(
      "fw-key",
      "accounts/fireworks/models/kimi-k2.6",
    ).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () =>
            Effect.succeed({
              mode: PROVIDER_MODE.self_managed,
              provider: "fireworks",
              model: "accounts/fireworks/models/kimi-k2.6",
              secret_ref: "fw-key",
            }) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    const call = rec.httpCalls[0];
    if (!call) throw new Error("expected one http call");
    expect(call.body).toEqual({
      mode: PROVIDER_MODE.self_managed,
      secret_ref: "fw-key",
      model: "accounts/fireworks/models/kimi-k2.6",
    });
  });

  test("--json mode prints raw response and skips tip prose", async () => {
    const rec = makeRecorder();
    const payload = {
      mode: PROVIDER_MODE.self_managed,
      provider: "fireworks",
      model: "kimi-k2.6",
      secret_ref: "fw-key",
    };
    const program = tenantProviderAddEffectFromArgs("fw-key", undefined).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () => Effect.succeed(payload) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout[0]).toBe(JSON.stringify(payload));
    // The success/tip prose is text-mode only; --json short-circuits before it.
    expect(rec.stdout.some((line) => /Tip: run a test event/.test(line))).toBe(
      false,
    );
  });

  test("missing --secret fails ValidationError without making a request", async () => {
    const rec = makeRecorder();
    const program = tenantProviderAddEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer(
          () => Effect.succeed({}) as Effect.Effect<unknown, ServerError>,
          rec,
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(rec.httpCalls).toEqual([]);
  });
});
