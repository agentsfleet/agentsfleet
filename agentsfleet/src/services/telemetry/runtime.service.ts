// TelemetryRuntime — process-wide telemetry identity + environment
// snapshot. Mirrors ~/Projects/oss/cli/apps/cli/src/shared/telemetry/
// runtime.service.ts.
//
// Both sinks read from this service:
//   - Analytics (PostHog) reads deviceId/sessionId/consent on capture
//   - Tracing (NDJSON/OTLP) reads tracesDir/consent/showDebug on span end
//
// Resolved once in runtime.layer.ts at process start; commands cannot
// mutate it. The previous shape (sessionId/deviceId only, both
// nullable) is subsumed — those two fields stay non-null in the new
// shape since the runtime layer always resolves an identity (granted
// path persists, denied path generates ephemeral ids).

import { Context, Layer } from "effect";
import type { ConsentState } from "./types.ts";

interface TelemetryRuntimeShape {
  readonly configDir: string;
  readonly tracesDir: string;
  readonly consent: ConsentState;
  readonly showDebug: boolean;
  readonly deviceId: string;
  readonly sessionId: string;
  readonly distinctId?: string;
  readonly isFirstRun: boolean;
  readonly isTty: boolean;
  readonly isCi: boolean;
  readonly os: string;
  readonly arch: string;
  readonly cliVersion: string;
}

export class TelemetryRuntime extends Context.Service<
  TelemetryRuntime,
  TelemetryRuntimeShape
>()("agentsfleet/telemetry/TelemetryRuntime") {}

// Test seam — fixture layer with caller-supplied values. The Live
// layer (telemetryRuntimeLayer in runtime.layer.ts) reads from
// session.json + process.env + Node os module.
export const telemetryRuntimeFromValuesLayer = (
  values: TelemetryRuntimeShape,
): Layer.Layer<TelemetryRuntime> =>
  Layer.succeed(TelemetryRuntime, TelemetryRuntime.of(values));
