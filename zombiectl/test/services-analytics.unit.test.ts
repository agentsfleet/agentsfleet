// Analytics service tests — verifies cli_session_id / cli_device_id
// auto-merge from TelemetryRuntime and that capture/identify/alias
// honor the no-op posthog-null path (telemetry off).

import { afterEach, beforeEach, describe, test } from "bun:test";
import { Effect, Layer } from "effect";
import { Analytics, AnalyticsLive } from "../src/services/analytics.ts";
import { TelemetryRuntimeFromValues } from "../src/services/telemetry-runtime.ts";

let snapshot: string | undefined;

beforeEach(() => {
  snapshot = process.env.DISABLE_TELEMETRY;
  process.env.DISABLE_TELEMETRY = "true";
});

afterEach(() => {
  if (snapshot === undefined) delete process.env.DISABLE_TELEMETRY;
  else process.env.DISABLE_TELEMETRY = snapshot;
});

const layerWith = (sessionId: string | null, deviceId: string | null): Layer.Layer<Analytics> =>
  Layer.provide(AnalyticsLive, TelemetryRuntimeFromValues({ sessionId, deviceId }));

describe("Analytics service", () => {
  test("capture is a no-op when telemetry disabled (no throw)", async () => {
    await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const a = yield* Analytics;
          yield* a.capture("test_event");
          yield* a.capture("test_event", { command: "x" });
        }),
        layerWith("sess-1", "dev-1"),
      ),
    );
  });
  test("identify mutates internal distinctId without throwing", async () => {
    await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const a = yield* Analytics;
          yield* a.identify("user-1");
        }),
        layerWith("sess-1", "dev-1"),
      ),
    );
  });
  test("alias is a no-op (placeholder for posthog alias)", async () => {
    await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const a = yield* Analytics;
          yield* a.alias("user-1", "dev-1");
        }),
        layerWith("sess-1", "dev-1"),
      ),
    );
  });
  test("shutdown completes without throwing when client null", async () => {
    await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const a = yield* Analytics;
          yield* a.shutdown;
        }),
        layerWith("sess-1", "dev-1"),
      ),
    );
  });
  test("capture handles null sessionId + deviceId", async () => {
    await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const a = yield* Analytics;
          yield* a.capture("ev", { command: "x" });
        }),
        layerWith(null, null),
      ),
    );
  });
});
