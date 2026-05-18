// Direct unit tests for the smaller services — exercising every
// branch so each file is >97% line + function covered.

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { Effect, Layer, Option, Redacted } from "effect";
import { CliConfig, cliConfigLayer } from "../src/services/config.ts";
import {
  TelemetryRuntime,
  telemetryRuntimeEmptyLayer,
  telemetryRuntimeFromValuesLayer,
} from "../src/services/telemetry-runtime.ts";

const provideEffect = async <A, E, R, S>(
  effect: Effect.Effect<A, E, R>,
  layer: Layer.Layer<S, never, never>,
): Promise<A> => {
  const provided = Effect.provide(effect, layer) as unknown as Effect.Effect<A, E, never>;
  return Effect.runPromise(provided);
};

describe("TelemetryRuntime", () => {
  test("Empty layer resolves to null sessionId + deviceId", async () => {
    const result = await provideEffect(
      Effect.gen(function* () {
        const t = yield* TelemetryRuntime;
        return t;
      }),
      telemetryRuntimeEmptyLayer,
    );
    expect(result.sessionId).toBeNull();
    expect(result.deviceId).toBeNull();
  });
  test("FromValues layer carries the provided values", async () => {
    const layer = telemetryRuntimeFromValuesLayer({ sessionId: "sess-x", deviceId: "dev-x" });
    const result = await provideEffect(
      Effect.gen(function* () {
        const t = yield* TelemetryRuntime;
        return t;
      }),
      layer,
    );
    expect(result.sessionId).toBe("sess-x");
    expect(result.deviceId).toBe("dev-x");
  });
  test("FromValues handles null values explicitly", async () => {
    const layer = telemetryRuntimeFromValuesLayer({ sessionId: null, deviceId: null });
    const result = await provideEffect(
      Effect.gen(function* () {
        const t = yield* TelemetryRuntime;
        return t;
      }),
      layer,
    );
    expect(result.sessionId).toBeNull();
    expect(result.deviceId).toBeNull();
  });
  test("FromValues factory is referentially distinct per call", () => {
    const a = telemetryRuntimeFromValuesLayer({ sessionId: "x", deviceId: "y" });
    const b = telemetryRuntimeFromValuesLayer({ sessionId: "x", deviceId: "y" });
    expect(a).not.toBe(b);
  });
});

describe("CliConfig", () => {
  // We mutate process.env around the Default layer to assert each env-resolution branch.
  const SNAPSHOT: Record<string, string | undefined> = {};
  const ENV_KEYS = ["ZOMBIE_API_URL", "ZOMBIE_DASHBOARD_URL", "ZOMBIE_TOKEN"] as const;

  beforeEach(() => {
    for (const k of ENV_KEYS) SNAPSHOT[k] = process.env[k];
  });
  afterEach(() => {
    for (const k of ENV_KEYS) {
      if (SNAPSHOT[k] === undefined) delete process.env[k];
      else process.env[k] = SNAPSHOT[k];
    }
  });

  test("Default layer uses fallback URLs + no token when env unset", async () => {
    delete process.env.ZOMBIE_API_URL;
    delete process.env.ZOMBIE_DASHBOARD_URL;
    delete process.env.ZOMBIE_TOKEN;
    const result = await provideEffect(
      Effect.gen(function* () {
        return yield* CliConfig;
      }),
      cliConfigLayer,
    );
    expect(result.apiUrl).toBe("https://api.usezombie.com");
    expect(result.dashboardUrl).toBe("https://dashboard.usezombie.com");
    expect(Option.isNone(result.accessToken)).toBe(true);
    expect(result.jsonMode).toBe(false);
    expect(result.noOpen).toBe(false);
  });
  test("Default layer reads explicit env overrides", async () => {
    process.env.ZOMBIE_API_URL = "https://api.test.local";
    process.env.ZOMBIE_DASHBOARD_URL = "https://dash.test.local";
    process.env.ZOMBIE_TOKEN = "tok-1";
    const result = await provideEffect(
      Effect.gen(function* () {
        return yield* CliConfig;
      }),
      cliConfigLayer,
    );
    expect(result.apiUrl).toBe("https://api.test.local");
    expect(result.dashboardUrl).toBe("https://dash.test.local");
    expect(Option.isSome(result.accessToken)).toBe(true);
    if (Option.isSome(result.accessToken)) {
      expect(Redacted.value(result.accessToken.value)).toBe("tok-1");
    }
  });
  test("Default layer treats whitespace-only env values as unset", async () => {
    process.env.ZOMBIE_API_URL = "   ";
    process.env.ZOMBIE_TOKEN = "";
    const result = await provideEffect(
      Effect.gen(function* () {
        return yield* CliConfig;
      }),
      cliConfigLayer,
    );
    expect(result.apiUrl).toBe("https://api.usezombie.com");
    expect(Option.isNone(result.accessToken)).toBe(true);
  });
});
