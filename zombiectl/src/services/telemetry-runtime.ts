// TelemetryRuntime — sessionId + deviceId resolved at process start.
// The Live layer reads from the persisted session record (see
// program/session-store.ts); the Test layer provides fixed values.
//
// Every analytics emit reads these from the service, not from a
// thread-local ctx field. Commands cannot accidentally lose the
// cli_session_id / cli_device_id props because the Analytics service
// reaches into TelemetryRuntime directly.

import { Layer, Context } from "effect";

export interface TelemetryRuntimeShape {
  readonly sessionId: string | null;
  readonly deviceId: string | null;
}

export class TelemetryRuntime extends Context.Service<
  TelemetryRuntime,
  TelemetryRuntimeShape
>()("zombiectl/telemetry/TelemetryRuntime") {}

export const telemetryRuntimeFromValuesLayer = (
  values: TelemetryRuntimeShape,
): Layer.Layer<TelemetryRuntime> =>
  Layer.succeed(TelemetryRuntime, TelemetryRuntime.of(values));

export const telemetryRuntimeEmptyLayer: Layer.Layer<TelemetryRuntime> =
  Layer.succeed(
    TelemetryRuntime,
    TelemetryRuntime.of({ sessionId: null, deviceId: null }),
  );
