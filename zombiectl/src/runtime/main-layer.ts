// MainLayer — the runtime-boundary composition the dispatcher provides.
// Every Effect-shaped command declares the services it reads (its `R`)
// via `yield* Service`; the dispatcher composes mainLayerFor(input) at
// the boundary and the type-checker verifies the command's R is a
// subset.
//
// Layer order is significant for dependency resolution:
//   - CliConfig has no deps
//   - TelemetryRuntime is resolved on disk + env by services/telemetry/
//     runtime.layer.ts — no input thread-through from cli.ts anymore.
//   - Analytics + Tracing both consume TelemetryRuntime; Analytics
//     also consumes CliConfig for telemetryPosthogKey/Host.
//   - CommandRuntime is per-invocation; populated from MainLayerInput.commandPath
//   - HttpClient consumes CliConfig
//   - Output, Credentials, Browser, Workspaces have no service deps
//
// Two entry points:
//   - `MainLayer` — defaults-only constant, used by callers that don't
//     need per-invocation overrides (and by tests that exercise the
//     env-resolved shape).
//   - `mainLayerFor(input)` — composes a layer with config/streams/
//     commandPath overrides. Mirrors Supabase's cliProgramFor helper
//     factory in shared/cli/run.ts.

import { Layer } from "effect";
import {
  CliConfig,
  cliConfigLayer,
  cliConfigFromValuesLayer,
  type CliConfigShape,
} from "../services/config.ts";
import {
  Output,
  outputStdioLayer,
  outputFromStreamsLayer,
} from "../services/output.ts";
import { Credentials, credentialsLayer } from "../services/credentials.ts";
import { HttpClient, httpClientLayer } from "../services/http-client.ts";
import { Input, inputLayer } from "../services/input.ts";
import { Stdin, stdinLayer, stdinFromStreamLayer } from "../services/stdin.ts";
import { Browser } from "../services/browser.service.ts";
import { browserLayer } from "../services/browser.layer.ts";
import { Workspaces, workspacesLayer } from "../services/workspaces.ts";
import {
  CommandRuntime,
  commandRuntimeFromValuesLayer,
} from "./command-runtime.service.ts";
import { TelemetryRuntime } from "../services/telemetry/runtime.service.ts";
import { telemetryRuntimeLayer } from "../services/telemetry/runtime.layer.ts";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { analyticsLayer } from "../services/telemetry/analytics.layer.ts";
import { tracingLayer } from "../services/telemetry/tracing.layer.ts";

// Every service `mainLayerFor` provides. Command Effects' R channel
// must be a subset.
//
// `Tracing` (Tracer.Tracer) is a Context.Reference with a default,
// not a Service — Effect resolves it from the active reference even
// when not explicitly in R. The tracing layer still installs the
// CLI tracer at the boundary so spans flow to NDJSON; the service
// just doesn't appear in MainLayerServices.
export type MainLayerServices =
  | Analytics
  | Browser
  | CliConfig
  | CommandRuntime
  | Credentials
  | HttpClient
  | Input
  | Output
  | Stdin
  | TelemetryRuntime
  | Workspaces;

export interface MainLayerInput {
  readonly config?: Partial<CliConfigShape>;
  readonly streams?: {
    readonly stdout: NodeJS.WritableStream;
    readonly stderr: NodeJS.WritableStream;
  };
  // Injected stdin (runCli threads io.stdin here). Defaults to process.stdin
  // via stdinLayer when omitted. The login direct-token resolve reads its
  // isTTY + piped payload from this seam.
  readonly stdin?: NodeJS.ReadableStream;
  // commandPath populates CommandRuntime so the supabase-pattern span
  // name + analytics command label are non-empty. handlers-bind.ts
  // passes the wrap site's `name` (e.g. "agent.add") split by "."; the
  // commander-bridge passes ["__parse__"]. Defaults to ["unknown"]
  // when omitted (tests that don't care about CommandRuntime).
  readonly commandPath?: ReadonlyArray<string>;
  // commandRunId correlates analytics events + spans + log lines for
  // one invocation. handlers-bind.ts generates one per wrap call.
  // Defaults to crypto.randomUUID() per mainLayerFor call.
  readonly commandRunId?: string;
}

export const mainLayerFor = (
  input: MainLayerInput = {},
): Layer.Layer<MainLayerServices> => {
  const configBase =
    input.config !== undefined ? cliConfigFromValuesLayer(input.config) : cliConfigLayer;
  const outputBase =
    input.streams !== undefined ? outputFromStreamsLayer(input.streams) : outputStdioLayer;
  const stdinBase =
    input.stdin !== undefined ? stdinFromStreamLayer(input.stdin) : stdinLayer;

  const commandRuntime = commandRuntimeFromValuesLayer({
    commandPath: input.commandPath ?? ["unknown"],
    commandRunId: input.commandRunId ?? crypto.randomUUID(),
  });

  const http = httpClientLayer.pipe(Layer.provide(configBase));
  const analytics = analyticsLayer.pipe(
    Layer.provide(telemetryRuntimeLayer),
    Layer.provide(configBase),
  );
  const tracing = tracingLayer.pipe(Layer.provide(telemetryRuntimeLayer));

  return Layer.mergeAll(
    configBase,
    telemetryRuntimeLayer,
    outputBase,
    credentialsLayer,
    browserLayer,
    workspacesLayer,
    inputLayer,
    stdinBase,
    commandRuntime,
    http,
    analytics,
    tracing,
  ) as Layer.Layer<MainLayerServices>;
};
