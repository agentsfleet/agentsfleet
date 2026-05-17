// TelemetryRuntime — sessionId + deviceId resolved at process start.
// The Live layer reads from the persisted session record (see
// program/session-store.ts); the Test layer provides fixed values.
//
// Every analytics emit reads these from the service, not from a
// thread-local ctx field. Commands cannot accidentally lose the
// cli_session_id / cli_device_id props because the Analytics service
// reaches into TelemetryRuntime directly.

import { Context, Layer } from "effect";

export interface TelemetryRuntimeShape {
  readonly sessionId: string | null;
  readonly deviceId: string | null;
}

export class TelemetryRuntime extends Context.Tag("TelemetryRuntime")<
  TelemetryRuntime,
  TelemetryRuntimeShape
>() {}

export const TelemetryRuntimeFromValues = (
  values: TelemetryRuntimeShape,
): Layer.Layer<TelemetryRuntime> => Layer.succeed(TelemetryRuntime, values);

export const TelemetryRuntimeEmpty: Layer.Layer<TelemetryRuntime> =
  Layer.succeed(TelemetryRuntime, { sessionId: null, deviceId: null });
