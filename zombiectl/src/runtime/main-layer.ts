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
import { cliConfigLayer } from "../services/config.ts";
import { telemetryRuntimeEmptyLayer } from "../services/telemetry-runtime.ts";
import { outputStdioLayer } from "../services/output.ts";
import { credentialsLayer } from "../services/credentials.ts";
import { analyticsLayer } from "../services/analytics.ts";
import { httpClientLayer } from "../services/http-client.ts";
import { browserLayer } from "../services/browser.ts";
import { workspacesLayer } from "../services/workspaces.ts";
import { spinnerLayer } from "../services/spinner.ts";

const Base = Layer.mergeAll(
  cliConfigLayer,
  telemetryRuntimeEmptyLayer,
  outputStdioLayer,
  credentialsLayer,
  browserLayer,
  workspacesLayer,
  spinnerLayer,
);

const WithAnalytics = analyticsLayer.pipe(Layer.provide(telemetryRuntimeEmptyLayer));
const WithHttp = httpClientLayer.pipe(Layer.provide(cliConfigLayer));

export const MainLayer = Layer.mergeAll(Base, WithAnalytics, WithHttp);
