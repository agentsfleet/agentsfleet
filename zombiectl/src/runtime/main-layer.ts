// MainLayer — the runtime-boundary composition the dispatcher
// provides. Every Effect-shaped command declares the services it
// reads (its `R`) via `yield* Service`; the dispatcher composes
// MainLayer at the boundary and the type-checker verifies the
// command's R is a subset.
//
// Layer order is significant for dependency resolution:
//   - CliConfig has no deps
//   - TelemetryRuntime is wired by the bootstrap (cli.ts) with the
//     resolved session/device id pair; the default layer here is
//     empty/null and is overridden at provide-time
//   - Analytics consumes TelemetryRuntime
//   - HttpClient consumes CliConfig
//   - Output, Credentials, Browser, Workspaces, Spinner have no service deps

import { Layer } from "effect";
import { CliConfigLive } from "../services/config.ts";
import { TelemetryRuntimeEmpty } from "../services/telemetry-runtime.ts";
import { OutputStdioLayer } from "../services/output.ts";
import { CredentialsLive } from "../services/credentials.ts";
import { AnalyticsLive } from "../services/analytics.ts";
import { HttpClientLive } from "../services/http-client.ts";
import { BrowserLive } from "../services/browser.ts";
import { WorkspacesLive } from "../services/workspaces.ts";
import { SpinnerLive } from "../services/spinner.ts";

const Base = Layer.mergeAll(
  CliConfigLive,
  TelemetryRuntimeEmpty,
  OutputStdioLayer,
  CredentialsLive,
  BrowserLive,
  WorkspacesLive,
  SpinnerLive,
);

const WithAnalytics = AnalyticsLive.pipe(Layer.provide(TelemetryRuntimeEmpty));
const WithHttp = HttpClientLive.pipe(Layer.provide(CliConfigLive));

export const MainLayer = Layer.mergeAll(Base, WithAnalytics, WithHttp);
