import { describe, expect, it } from "bun:test";
import { STUB } from "./analytics.layer.fixture.ts";

// Imports happen AFTER the mock.module call above resolves.
const { analyticsLayer } = await import("../../src/services/telemetry/analytics.layer.ts");
const { Analytics } = await import("../../src/services/telemetry/analytics.service.ts");
const { withAnalyticsContext } = await import(
  "../../src/services/telemetry/analytics-context.ts"
);
const { cliConfigLayer } = await import("../../src/services/config.ts");
const { Effect, Layer } = await import("effect");

const EXPECTED_REQUEST_TIMEOUT_MS = 1_000;

function denyConsentViaKillSwitch(): void {
  process.env.AGENTSFLEET_TELEMETRY_DISABLED = "1";
}

function denyConsentViaDoNotTrack(): void {
  process.env.DO_NOT_TRACK = "1";
}

function getAnalytics() {
  return Effect.gen(function* () {
    return yield* Analytics;
  }).pipe(Effect.provide(Layer.provide(analyticsLayer, cliConfigLayer)));
}

describe("analyticsLayer", () => {
  it("uses bounded delivery settings", async () => {
    const program = Effect.scoped(
      Effect.gen(function* () {
        yield* getAnalytics();
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.options).toMatchObject({
      fetchRetryCount: 0,
      flushAt: 100,
      flushInterval: 0,
      requestTimeout: EXPECTED_REQUEST_TIMEOUT_MS,
    });
  });

  it("emits when env is clean (default consent=granted, supabase parity)", async () => {
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.capture("evt", { a: 1 });
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.captured).toHaveLength(1);
  });

  it("returns a noop service when AGENTSFLEET_TELEMETRY_DISABLED=1", async () => {
    denyConsentViaKillSwitch();
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.capture("evt", { a: 1 });
        yield* svc.identify("user-1", { p: 1 });
        yield* svc.alias("user-1", "alias-1");
        yield* svc.groupIdentify("workspace", "ws-1", { p: 2 });
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.captured).toEqual([]);
    expect(STUB.identified).toEqual([]);
    expect(STUB.aliased).toEqual([]);
    expect(STUB.groupIdentified).toEqual([]);
    expect(STUB.shutdownCalls).toBe(0);
  });

  it("returns a noop service when DO_NOT_TRACK=1", async () => {
    denyConsentViaDoNotTrack();
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.capture("evt", { a: 1 });
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.captured).toEqual([]);
    expect(STUB.shutdownCalls).toBe(0);
  });

  it("capture merges base properties + AnalyticsContext + per-call properties", async () => {
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc
          .capture("evt", { custom: "x", drop: undefined })
          .pipe(
            withAnalyticsContext({
              command_run_id: "rid-1",
              command: "login",
              flags_used: ["a"],
              flag_values: { a: 1 },
              groups: { workspace: "ws-1" },
            }),
          );
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.captured).toHaveLength(1);
    const evt = STUB.captured[0]!;
    expect(evt.event).toBe("evt");
    expect(typeof evt.distinctId).toBe("string");
    expect(evt.groups).toEqual({ workspace: "ws-1" });
    expect(evt.properties?.platform).toBe("cli");
    expect(evt.properties?.schema_version).toBe(1);
    expect(typeof evt.properties?.device_id).toBe("string");
    expect(typeof evt.properties?.$session_id).toBe("string");
    expect(evt.properties?.command_run_id).toBe("rid-1");
    expect(evt.properties?.command).toBe("login");
    expect(evt.properties?.flags_used).toEqual(["a"]);
    expect(evt.properties?.flag_values).toEqual({ a: 1 });
    expect(evt.properties?.custom).toBe("x");
    expect(Object.hasOwn(evt.properties ?? {}, "drop")).toBe(false);
  });

  it("capture records the non-interactive fallback when no agent is detected", async () => {
    const restoreStdout = STUB.forceStdoutIsTty(false);
    try {
      const program = Effect.scoped(
        Effect.gen(function* () {
          const svc = yield* getAnalytics();
          yield* svc.capture("evt");
        }),
      );
      await Effect.runPromise(program);
      expect(STUB.captured[0]?.properties?.ai_tool).toBe("unknown_non_interactive");
    } finally {
      restoreStdout();
    }
  });

  it("capture records the continuous-integration fallback when no agent is detected", async () => {
    process.env.CI = "1";
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.capture("evt");
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.captured[0]?.properties?.ai_tool).toBe("ci");
  });

  it("capture omits ai_tool for an interactive terminal when no agent is detected", async () => {
    const restoreStdout = STUB.forceStdoutIsTty(true);
    try {
      const program = Effect.scoped(
        Effect.gen(function* () {
          const svc = yield* getAnalytics();
          yield* svc.capture("evt");
        }),
      );
      await Effect.runPromise(program);
      expect(Object.hasOwn(STUB.captured[0]?.properties ?? {}, "ai_tool")).toBe(false);
    } finally {
      restoreStdout();
    }
  });

  it("capture without context defaults distinctId to runtime deviceId", async () => {
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.capture("evt");
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.captured).toHaveLength(1);
    const evt = STUB.captured[0]!;
    expect(evt.distinctId).toBe(evt.properties?.device_id as string);
    expect(evt.groups).toBeUndefined();
  });

  it("capture uses runtime distinctId (from telemetry.json) when set", async () => {
    STUB.writeTelemetryJson({
      consent: "granted",
      device_id: "ignored",
      session_id: "ignored",
      session_last_active: Date.now(),
      distinct_id: "user-rt",
    });
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.capture("evt");
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.captured[0]?.distinctId).toBe("user-rt");
  });

  it("capture uses context.distinct_id with highest precedence", async () => {
    STUB.writeTelemetryJson({
      consent: "granted",
      device_id: "ignored",
      session_id: "ignored",
      session_last_active: Date.now(),
      distinct_id: "user-rt",
    });
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.capture("evt").pipe(withAnalyticsContext({ distinct_id: "user-ctx" }));
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.captured[0]?.distinctId).toBe("user-ctx");
  });

  it("identify passes through with cli_version / os / arch + extras", async () => {
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.identify("user-1", { email: "kk@example.com" });
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.identified).toHaveLength(1);
    const ident = STUB.identified[0]!;
    expect(ident.distinctId).toBe("user-1");
    expect(ident.properties?.email).toBe("kk@example.com");
    expect(typeof ident.properties?.cli_version).toBe("string");
    expect(typeof ident.properties?.os).toBe("string");
    expect(typeof ident.properties?.arch).toBe("string");
  });

  it("identify with no extra properties only emits cli_version / os / arch", async () => {
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.identify("user-1");
      }),
    );
    await Effect.runPromise(program);
    expect(Object.keys(STUB.identified[0]?.properties ?? {}).sort()).toEqual([
      "arch",
      "cli_version",
      "os",
    ]);
  });

  it("alias passes through unchanged", async () => {
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.alias("user-1", "alias-1");
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.aliased).toEqual([{ distinctId: "user-1", alias: "alias-1" }]);
  });

  it("groupIdentify uses context.distinct_id when present", async () => {
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc
          .groupIdentify("workspace", "ws-1", { plan: "pro" })
          .pipe(withAnalyticsContext({ distinct_id: "user-ctx" }));
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.groupIdentified).toHaveLength(1);
    const g = STUB.groupIdentified[0]!;
    expect(g.groupType).toBe("workspace");
    expect(g.groupKey).toBe("ws-1");
    expect(g.distinctId).toBe("user-ctx");
    expect(g.properties).toEqual({ plan: "pro" });
  });

  it("groupIdentify falls back to runtime deviceId when context lacks distinct_id", async () => {
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.groupIdentify("workspace", "ws-2");
      }),
    );
    await Effect.runPromise(program);
    expect(typeof STUB.groupIdentified[0]?.distinctId).toBe("string");
    expect(STUB.groupIdentified[0]?.distinctId.length).toBeGreaterThan(0);
  });

  it("flushes without invoking the client shutdown logger when the scope closes", async () => {
    const program = Effect.scoped(
      Effect.gen(function* () {
        yield* getAnalytics();
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.flushCalls).toBe(1);
    expect(STUB.shutdownCalls).toBe(0);
  });

  it("ignores a flush failure when the scope closes", async () => {
    STUB.flushError = new Error("unreachable telemetry host");
    const program = Effect.scoped(
      Effect.gen(function* () {
        yield* getAnalytics();
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.flushCalls).toBe(1);
    expect(STUB.shutdownCalls).toBe(0);
  });

});
