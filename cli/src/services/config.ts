// CliConfig service — resolved at process start from env + defaults.
// Carries the API base URL, dashboard URL, env-sourced access token,
// and runtime flags every command might need to read.
//
// Tokens read from env are wrapped in `Redacted` so the value can flow
// through Effects without risking accidental log emission. The actual
// string is extracted only at the HTTP authorization-header site.

import { Effect, Layer, Option, Redacted, Context } from "effect";
import type { FetchImpl } from "../lib/http.ts";
import {
  DEFAULT_API_URL,
  normalizeApiUrl,
  resolveDashboardUrl,
} from "../util/url.ts";

// PostHog project key is public-by-design (write-only capture scope,
// no read/admin), same model as Stripe pk_live_…. Supabase ships
// theirs as a plain string in cli-config.layer.ts; we match that.
export const DEFAULT_POSTHOG_HOST = "https://us.i.posthog.com";
export const DEFAULT_POSTHOG_KEY = "phc_XmuRIXBSTRfxka7IgfkU0VPMD3LDRR3IqILXNg3bXzv"; // gitleaks:allow — public phc_ key (write-only capture scope), see header comment
// The service-auth env-var name. A machine principal (an `agt_t…` tenant
// API key) exported here authenticates the CLI without a browser login,
// and — by the env-wins precedence in `resolveToken` — takes priority over
// a stored login JWT. This is the only env-sourced bearer the CLI reads; the
// older `AGENTSFLEET_TOKEN` env var (and the unprefixed `API_KEY` alias) were
// removed.
export const AGENTSFLEET_API_KEY_ENV = "AGENTSFLEET_API_KEY";

export interface CliConfigShape {
  readonly apiUrl: string;
  readonly dashboardUrl: string;
  readonly accessToken: Option.Option<Redacted.Redacted<string>>;
  readonly jsonMode: boolean;
  readonly noOpen: boolean;
  readonly telemetryPosthogKey: string;
  readonly telemetryPosthogHost: string;
  // Injectable fetch impl — integration tests pass a stubbed fetch via
  // runCli's RunCliIo, which threads here so HttpClient bypasses
  // globalThis.fetch. Defaults to undefined → globalThis.fetch.
  readonly fetchImpl?: FetchImpl;
}

export type CliConfig = CliConfigShape;
export const CliConfig = Context.Service<CliConfig>(
  "agentsfleet/config/CliConfig",
);

// Single guarded accessor for the process environment (returns an empty env
// in non-Node contexts). All env reads route through here so the
// `typeof process` guard lives in exactly one place.
const processEnv = (): NodeJS.ProcessEnv =>
  typeof process !== "undefined" ? process.env : ({} as NodeJS.ProcessEnv);

const readEnv = (key: string): string | undefined => processEnv()[key];

const trimmed = (v: string | undefined): string | undefined => {
  if (typeof v !== "string") return undefined;
  const t = v.trim();
  return t.length > 0 ? t : undefined;
};

// Single source for the env-sourced service API key. Trimmed so a
// whitespace-only export counts as unset (never reaches the wire as a blank
// Bearer). Both cli.ts and resolveCliConfig resolve through here so the read
// can't drift between the two paths. Only `AGENTSFLEET_API_KEY` is honoured —
// the unprefixed `API_KEY` alias was dropped as off-brand.
export const resolveApiKeyFromEnv = (env: NodeJS.ProcessEnv): string | null =>
  trimmed(env[AGENTSFLEET_API_KEY_ENV]) ?? null;

export const resolveCliConfig = (): CliConfigShape => {
  const apiUrl = normalizeApiUrl(
    trimmed(readEnv("AGENTSFLEET_API_URL")) ?? DEFAULT_API_URL,
  );
  const dashboardUrl = resolveDashboardUrl(
    apiUrl,
    trimmed(readEnv("AGENTSFLEET_DASHBOARD_URL")),
  );
  // The env-sourced bearer is the service API key (env slot). It wins over
  // a stored login JWT via `resolveToken`'s env-first precedence. Resolution
  // is centralised in cli.ts before this layer; tests that bypass runCli see
  // the env value here.
  const envToken = resolveApiKeyFromEnv(processEnv());
  const telemetryPosthogKey =
    trimmed(readEnv("AGENTSFLEET_TELEMETRY_POSTHOG_KEY")) ?? DEFAULT_POSTHOG_KEY;
  const telemetryPosthogHost =
    trimmed(readEnv("AGENTSFLEET_TELEMETRY_POSTHOG_HOST")) ?? DEFAULT_POSTHOG_HOST;
  return {
    apiUrl,
    dashboardUrl,
    accessToken:
      envToken !== null
        ? Option.some(Redacted.make(envToken))
        : Option.none(),
    jsonMode: false,
    noOpen: false,
    telemetryPosthogKey,
    telemetryPosthogHost,
  };
};

export const cliConfigLayer: Layer.Layer<CliConfig> = Layer.effect(
  CliConfig,
  Effect.sync(() => CliConfig.of(resolveCliConfig())),
);

export const cliConfigFromValuesLayer = (
  overrides: Partial<CliConfigShape> = {},
): Layer.Layer<CliConfig> =>
  Layer.succeed(
    CliConfig,
    CliConfig.of({ ...resolveCliConfig(), ...overrides }),
  );
