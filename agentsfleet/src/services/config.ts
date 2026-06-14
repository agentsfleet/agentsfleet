// CliConfig service — resolved at process start from env + defaults.
// Carries the API base URL, dashboard URL, env-sourced access token,
// and runtime flags every command might need to read.
//
// Tokens read from env are wrapped in `Redacted` so the value can flow
// through Effects without risking accidental log emission. The actual
// string is extracted only at the HTTP authorization-header site.

import { Effect, Layer, Option, Redacted, Context } from "effect";
import type { FetchImpl } from "../lib/http.ts";

const DEFAULT_API_URL = "https://api.agentsfleet.net";
// PROD dashboard is `app.agentsfleet.net` (DEV is the Vercel preview at
// `usezombie-app.vercel.app`; see `runtime_loader.zig:121 APP_URL`,
// `BILLING_DASHBOARD_URL` in `commands/billing.ts`, and acceptance's
// `DASHBOARD_URL_PROD`). The earlier `dashboard.agentsfleet.net` value
// was a typo that pointed at a nonexistent domain.
const DEFAULT_DASHBOARD_URL = "https://app.agentsfleet.net";
// PostHog project key is public-by-design (write-only capture scope,
// no read/admin), same model as Stripe pk_live_…. Supabase ships
// theirs as a plain string in cli-config.layer.ts; we match that.
export const DEFAULT_POSTHOG_HOST = "https://us.i.posthog.com";
export const DEFAULT_POSTHOG_KEY = "phc_XmuRIXBSTRfxka7IgfkU0VPMD3LDRR3IqILXNg3bXzv"; // gitleaks:allow — public phc_ key (write-only capture scope), see header comment
// The single auth-token env-var name. One identifier shared by the
// config resolver, the TTY-aware file/env resolver, and the login
// command's direct-token source so the three never drift.
export const AGENTSFLEET_TOKEN_ENV = "AGENTSFLEET_TOKEN";

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

export class CliConfig extends Context.Service<CliConfig, CliConfigShape>()(
  "agentsfleet/config/CliConfig",
) {}

const readEnv = (key: string): string | undefined =>
  typeof process !== "undefined" ? process.env[key] : undefined;

const trimmed = (v: string | undefined): string | undefined => {
  if (typeof v !== "string") return undefined;
  const t = v.trim();
  return t.length > 0 ? t : undefined;
};

export const resolveCliConfig = (): CliConfigShape => {
  const apiUrl = trimmed(readEnv("AGENTSFLEET_API_URL")) ?? DEFAULT_API_URL;
  const dashboardUrl =
    trimmed(readEnv("AGENTSFLEET_DASHBOARD_URL")) ?? DEFAULT_DASHBOARD_URL;
  // AGENTSFLEET_TOKEN is the auth-token env var. TTY-aware precedence vs
  // credentials.json is resolved in cli.ts before this layer; tests that
  // bypass runCli see the env value here.
  const envToken = trimmed(readEnv(AGENTSFLEET_TOKEN_ENV));
  const telemetryPosthogKey =
    trimmed(readEnv("AGENTSFLEET_TELEMETRY_POSTHOG_KEY")) ?? DEFAULT_POSTHOG_KEY;
  const telemetryPosthogHost =
    trimmed(readEnv("AGENTSFLEET_TELEMETRY_POSTHOG_HOST")) ?? DEFAULT_POSTHOG_HOST;
  return {
    apiUrl,
    dashboardUrl,
    accessToken:
      envToken !== undefined
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
