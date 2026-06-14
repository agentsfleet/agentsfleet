// analyticsLayer + analyticsInternals coverage. No Supabase counterpart
// — usezombie's analytics layer wraps posthog-node directly. Mocks the
// PostHog constructor via bun:test mock.module so capture / identify /
// alias / groupIdentify / shutdown are observed in-process without
// touching the network.

import { afterEach, beforeEach, describe, expect, it, mock } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import pathMod from "node:path";

interface CapturedEvent {
  event: string;
  distinctId: string;
  properties?: Record<string, unknown>;
  groups?: Record<string, string>;
}

interface PostHogStub {
  capture: ReturnType<typeof mock>;
  identify: ReturnType<typeof mock>;
  alias: ReturnType<typeof mock>;
  groupIdentify: ReturnType<typeof mock>;
  shutdown: ReturnType<typeof mock>;
  captured: CapturedEvent[];
  identified: Array<{ distinctId: string; properties?: Record<string, unknown> }>;
  aliased: Array<{ distinctId: string; alias: string }>;
  groupIdentified: Array<{
    groupType: string;
    groupKey: string;
    distinctId: string;
    properties?: Record<string, unknown>;
  }>;
  shutdownCalls: number;
}

const STUB: PostHogStub = {
  captured: [],
  identified: [],
  aliased: [],
  groupIdentified: [],
  shutdownCalls: 0,
  capture: mock(() => undefined),
  identify: mock(() => undefined),
  alias: mock(() => undefined),
  groupIdentify: mock(() => undefined),
  shutdown: mock(async () => undefined),
};

mock.module("posthog-node", () => ({
  PostHog: class PostHogStubClass {
      constructor(_key: string, _opts: Record<string, unknown>) {}
      capture(evt: CapturedEvent): void {
        STUB.captured.push(evt);
        STUB.capture(evt);
      }
      identify(payload: { distinctId: string; properties?: Record<string, unknown> }): void {
        STUB.identified.push(payload);
        STUB.identify(payload);
      }
      alias(payload: { distinctId: string; alias: string }): void {
        STUB.aliased.push(payload);
        STUB.alias(payload);
      }
      groupIdentify(payload: {
        groupType: string;
        groupKey: string;
        distinctId: string;
        properties?: Record<string, unknown>;
      }): void {
        STUB.groupIdentified.push(payload);
        STUB.groupIdentify(payload);
      }
      async shutdown(): Promise<void> {
        STUB.shutdownCalls += 1;
        await STUB.shutdown();
      }
      async _shutdown(_timeoutMs?: number): Promise<void> {
        STUB.shutdownCalls += 1;
        await STUB.shutdown();
      }
    },
}));

// Imports happen AFTER the mock.module call above resolves.
const { analyticsLayer } = await import("../../src/services/telemetry/analytics.layer.ts");
const { Analytics } = await import("../../src/services/telemetry/analytics.service.ts");
const { withAnalyticsContext } = await import(
  "../../src/services/telemetry/analytics-context.ts"
);
const { cliConfigLayer } = await import("../../src/services/config.ts");
const { Effect, Layer } = await import("effect");

const ENV_KEYS = [
  "ZOMBIE_TELEMETRY_POSTHOG_KEY",
  "ZOMBIE_TELEMETRY_POSTHOG_HOST",
  "ZOMBIE_STATE_DIR",
  "ZOMBIE_TELEMETRY_DISABLED",
  "DO_NOT_TRACK",
  "ZOMBIE_TELEMETRY_DEBUG",
  "CI",
  "GITHUB_ACTIONS",
  "GITLAB_CI",
  "CIRCLECI",
  "JENKINS_URL",
  "BUILDKITE",
] as const;
const saved: Record<string, string | undefined> = {};
let tmpDir: string | undefined;

beforeEach(() => {
  for (const k of ENV_KEYS) saved[k] = process.env[k];
  for (const k of ENV_KEYS) delete process.env[k];
  STUB.captured.length = 0;
  STUB.identified.length = 0;
  STUB.aliased.length = 0;
  STUB.groupIdentified.length = 0;
  STUB.shutdownCalls = 0;
  tmpDir = mkdtempSync(pathMod.join(tmpdir(), "agentsfleet-analytics-test-"));
  process.env.ZOMBIE_STATE_DIR = tmpDir;
});

afterEach(() => {
  if (tmpDir !== undefined) rmSync(tmpDir, { recursive: true, force: true });
  tmpDir = undefined;
  for (const k of ENV_KEYS) {
    if (saved[k] === undefined) delete process.env[k];
    else process.env[k] = saved[k];
  }
});

// Default is opt-IN now (matches supabase). grantConsent is a no-op
// kept for test-readability — every emit path runs under the default
// granted state unless a test explicitly opts out via denyConsent().
function grantConsent(): void {
  // intentionally empty — default state is granted
}

function denyConsentViaKillSwitch(): void {
  process.env.ZOMBIE_TELEMETRY_DISABLED = "1";
}

function denyConsentViaDoNotTrack(): void {
  process.env.DO_NOT_TRACK = "1";
}

function getAnalytics() {
  return Effect.gen(function* () {
    return yield* Analytics;
  }).pipe(Effect.provide(Layer.provide(analyticsLayer, cliConfigLayer)));
}

function writeTelemetryJson(body: Record<string, unknown>): void {
  // tmpDir is reset in beforeEach. mkdtempSync already created the dir.
  const fs = require("node:fs") as typeof import("node:fs");
  fs.writeFileSync(pathMod.join(tmpDir!, "telemetry.json"), JSON.stringify(body));
}

describe("analyticsLayer", () => {
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

  it("returns a noop service when ZOMBIE_TELEMETRY_DISABLED=1", async () => {
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
    grantConsent();
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

  it("capture without context defaults distinctId to runtime deviceId", async () => {
    grantConsent();
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
    grantConsent();
    writeTelemetryJson({
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
    grantConsent();
    writeTelemetryJson({
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
    grantConsent();
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
    grantConsent();
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
    grantConsent();
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
    grantConsent();
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
    grantConsent();
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

  it("invokes shutdown via Effect.addFinalizer when the scope closes", async () => {
    grantConsent();
    const program = Effect.scoped(
      Effect.gen(function* () {
        yield* getAnalytics();
      }),
    );
    await Effect.runPromise(program);
    expect(STUB.shutdownCalls).toBe(1);
  });

});
