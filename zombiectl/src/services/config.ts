// CliConfig service — resolved at process start from env + defaults.
// Carries the API base URL, dashboard URL, env-sourced access token,
// and runtime flags every command might need to read.
//
// Tokens read from env are wrapped in `Redacted` so the value can flow
// through Effects without risking accidental log emission. The actual
// string is extracted only at the HTTP authorization-header site.

import { Effect, Layer, Option, Redacted, Context } from "effect";
import type { FetchImpl } from "../lib/http.ts";

const DEFAULT_API_URL = "https://api.usezombie.com";
const DEFAULT_DASHBOARD_URL = "https://dashboard.usezombie.com";
// PostHog project key is public-by-design (write-only capture scope,
// no read/admin), same model as Stripe pk_live_…. Supabase ships
// theirs as a plain string in cli-config.layer.ts; we match that.
export const DEFAULT_POSTHOG_HOST = "https://us.i.posthog.com";
export const DEFAULT_POSTHOG_KEY = "phc_XmuRIXBSTRfxka7IgfkU0VPMD3LDRR3IqILXNg3bXzv"; // gitleaks:allow — public phc_ key (write-only capture scope), see header comment

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
  "zombiectl/config/CliConfig",
) {}

const readEnv = (key: string): string | undefined =>
  typeof process !== "undefined" ? process.env[key] : undefined;

const trimmed = (v: string | undefined): string | undefined => {
  if (typeof v !== "string") return undefined;
  const t = v.trim();
  return t.length > 0 ? t : undefined;
};

export const resolveCliConfig = (): CliConfigShape => {
  const apiUrl = trimmed(readEnv("ZOMBIE_API_URL")) ?? DEFAULT_API_URL;
  const dashboardUrl =
    trimmed(readEnv("ZOMBIE_DASHBOARD_URL")) ?? DEFAULT_DASHBOARD_URL;
  const envToken = trimmed(readEnv("ZOMBIE_TOKEN"));
  const telemetryPosthogKey =
    trimmed(readEnv("ZOMBIE_TELEMETRY_POSTHOG_KEY")) ?? DEFAULT_POSTHOG_KEY;
  const telemetryPosthogHost =
    trimmed(readEnv("ZOMBIE_TELEMETRY_POSTHOG_HOST")) ?? DEFAULT_POSTHOG_HOST;
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
