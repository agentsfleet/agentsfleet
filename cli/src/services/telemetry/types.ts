// Shared types for the telemetry tree. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/types.ts.
//
// ConsentState is the single gate read by every exporter in the tree
// (PostHog Analytics and NDJSON/debug-console Tracing). It is resolved
// once in runtime.layer.ts from AGENTSFLEET_TELEMETRY_DISABLED/DO_NOT_TRACK env, the
// persisted telemetry.json file, and TTY/CI heuristics.

export type ConsentState = "granted" | "denied";

export interface TelemetryConfig {
  readonly consent: ConsentState;
  readonly device_id: string;
  readonly session_id: string;
  readonly session_last_active: number;
  readonly distinct_id?: string;
}
