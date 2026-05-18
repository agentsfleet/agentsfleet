// Analytics service — wraps the existing PostHog client construction
// in lib/analytics.ts. `capture` automatically merges `cli_session_id`
// and `cli_device_id` from TelemetryRuntime, so commands cannot
// accidentally lose those props on any emit.
//
// Telemetry failures are swallowed via `Effect.ignore` at the
// dispatcher — Analytics never blocks user-facing UX.

import { Effect, Layer, Context } from "effect";
// The `cliAnalytics` namespace import (rather than named imports) is
// load-bearing for tests: cli-analytics.unit.test.ts mutates the
// namespace members in place to inject capture/no-op stubs, and named
// imports would be frozen at module-load time. RULE NLR — keep this
// indirection.
import { cliAnalytics, type AnalyticsClient } from "../lib/analytics.ts";
import { TelemetryRuntime } from "./telemetry-runtime.ts";

export interface AnalyticsShape {
  readonly capture: (
    event: string,
    properties?: Record<string, unknown>,
  ) => Effect.Effect<void>;
  readonly identify: (distinctId: string) => Effect.Effect<void>;
  readonly alias: (distinctId: string, deviceId: string | null) => Effect.Effect<void>;
  readonly shutdown: Effect.Effect<void>;
}

export class Analytics extends Context.Service<Analytics, AnalyticsShape>()(
  "zombiectl/telemetry/Analytics",
) {}

interface AnalyticsState {
  client: AnalyticsClient | null;
  distinctId: string;
}

const makeShape = (
  state: AnalyticsState,
  telemetry: { sessionId: string | null; deviceId: string | null },
): AnalyticsShape => ({
  capture: (event, properties = {}) =>
    Effect.sync(() => {
      const base: Record<string, unknown> = {};
      if (telemetry.sessionId) base["cli_session_id"] = telemetry.sessionId;
      if (telemetry.deviceId) base["cli_device_id"] = telemetry.deviceId;
      cliAnalytics.trackCliEvent(state.client, state.distinctId, event, {
        ...base,
        ...properties,
      });
    }),
  identify: (distinctId) =>
    Effect.sync(() => {
      state.distinctId = distinctId;
    }),
  alias: () => Effect.void,
  shutdown: Effect.promise(() => cliAnalytics.shutdownCliAnalytics(state.client)),
});

export const analyticsLayer: Layer.Layer<Analytics, never, TelemetryRuntime> = Layer.effect(
  Analytics,
  Effect.gen(function* () {
    const telemetry = yield* TelemetryRuntime;
    const client = yield* Effect.promise(() => cliAnalytics.createCliAnalytics());
    const state: AnalyticsState = { client, distinctId: "anonymous" };
    return Analytics.of(makeShape(state, telemetry));
  }),
);
