// Analytics — PostHog product-analytics boundary. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/analytics.service.ts.
//
// `capture` automatically merges device_id / session_id from
// TelemetryRuntime and the current AnalyticsContext (command name,
// flags_used, flag_values, distinct_id, groups) inside the layer, so
// commands cannot accidentally lose those props on any emit.
//
// Telemetry failures bubble through the Effect cause channel; call
// sites swallow with `.pipe(Effect.ignore)`. Matches `analytics.layer.ts`
// and supabase `apps/cli/src/shared/telemetry/analytics.layer.ts` —
// no try/catch inside the layer's emit functions. Every `capture` call
// site is therefore responsible for the `.pipe(Effect.ignore)` suffix.

import { Context, Effect } from "effect";

interface AnalyticsShape {
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
