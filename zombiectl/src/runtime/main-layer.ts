// MainLayer — the runtime-boundary composition the dispatcher provides.
// Every Effect-shaped command declares the services it reads (its `R`)
// via `yield* Service`; the dispatcher composes mainLayerFor(input) at
// the boundary and the type-checker verifies the command's R is a
// subset.
//
// Layer order is significant for dependency resolution:
//   - CliConfig has no deps
//   - TelemetryRuntime is wired from runtime values resolved at process
//     start (cli.ts session record) and passed in via `input.telemetry`
//   - Analytics consumes TelemetryRuntime
//   - HttpClient consumes CliConfig
//   - Output, Credentials, Browser, Workspaces, Spinner have no service deps
//
// Two entry points:
//   - `MainLayer` — defaults-only constant, used by callers that don't
//     need per-invocation overrides (and by tests that exercise the
//     env-resolved shape).
//   - `mainLayerFor(input)` — composes a layer with telemetry/config/
//     streams overrides. Mirrors Supabase's `cliConfigLayerFor` helper
//     factory in shared/cli/run.ts — the dispatcher and the handler-
//     binding layer call this and apply `Effect.provide` at the outer
//     boundary.

import { Layer } from "effect";
import {
  CliConfig,
  cliConfigLayer,
  cliConfigFromValuesLayer,
  type CliConfigShape,
} from "../services/config.ts";
import {
  TelemetryRuntime,
  telemetryRuntimeEmptyLayer,
  telemetryRuntimeFromValuesLayer,
} from "../services/telemetry-runtime.ts";
import {
  Output,
  outputStdioLayer,
  outputFromStreamsLayer,
} from "../services/output.ts";
import { Credentials, credentialsLayer } from "../services/credentials.ts";
import { Analytics, analyticsLayer } from "../services/analytics.ts";
import { HttpClient, httpClientLayer } from "../services/http-client.ts";
import { Browser, browserLayer } from "../services/browser.ts";
import { Workspaces, workspacesLayer } from "../services/workspaces.ts";
import { Spinner, spinnerLayer } from "../services/spinner.ts";

// Every service `mainLayerFor` provides. Command Effects' R channel
// must be a subset.
export type MainLayerServices =
  | Analytics
  | Browser
  | CliConfig
  | Credentials
  | HttpClient
  | Output
  | Spinner
  | TelemetryRuntime
  | Workspaces;

export interface MainLayerInput {
  readonly telemetry?: {
    readonly sessionId: string | null;
    readonly deviceId: string | null;
  };
  readonly config?: Partial<CliConfigShape>;
  readonly streams?: {
    readonly stdout: NodeJS.WritableStream;
    readonly stderr: NodeJS.WritableStream;
  };
}

export const mainLayerFor = (
  input: MainLayerInput = {},
): Layer.Layer<MainLayerServices> => {
  const configBase =
    input.config !== undefined ? cliConfigFromValuesLayer(input.config) : cliConfigLayer;
  const telemetryBase =
    input.telemetry !== undefined
      ? telemetryRuntimeFromValuesLayer({
          sessionId: input.telemetry.sessionId,
          deviceId: input.telemetry.deviceId,
        })
      : telemetryRuntimeEmptyLayer;
  const outputBase =
    input.streams !== undefined ? outputFromStreamsLayer(input.streams) : outputStdioLayer;

  const http = httpClientLayer.pipe(Layer.provide(configBase));
  const analytics = analyticsLayer.pipe(Layer.provide(telemetryBase));

  return Layer.mergeAll(
    configBase,
    telemetryBase,
    outputBase,
    credentialsLayer,
    browserLayer,
    workspacesLayer,
    spinnerLayer,
    http,
    analytics,
  );
};

export const MainLayer: Layer.Layer<MainLayerServices> = mainLayerFor();
