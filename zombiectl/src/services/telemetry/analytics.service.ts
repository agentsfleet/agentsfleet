// Analytics — PostHog product-analytics boundary. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/analytics.service.ts.
//
// `capture` automatically merges device_id / session_id from
// TelemetryRuntime and the current AnalyticsContext (command name,
// flags_used, flag_values, distinct_id, groups) inside the layer, so
// commands cannot accidentally lose those props on any emit.
//
// Telemetry failures are swallowed inside the layer's capture impl —
// Analytics never blocks user-facing UX. The dispatcher does NOT need
// to wrap captures in Effect.ignore.

import { Context, Effect } from "effect";

export interface AnalyticsShape {
  readonly capture: (
    event: string,
    properties?: Record<string, unknown>,
  ) => Effect.Effect<void>;
  readonly identify: (
    distinctId: string,
    properties?: Record<string, unknown>,
  ) => Effect.Effect<void>;
  readonly alias: (distinctId: string, alias: string) => Effect.Effect<void>;
  readonly groupIdentify: (
    groupType: string,
    groupKey: string,
    properties?: Record<string, unknown>,
  ) => Effect.Effect<void>;
}

export class Analytics extends Context.Service<Analytics, AnalyticsShape>()(
  "zombiectl/telemetry/Analytics",
) {}
