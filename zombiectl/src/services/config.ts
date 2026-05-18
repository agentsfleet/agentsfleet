// CliConfig service — resolved at process start from env + defaults.
// Carries the API base URL, dashboard URL, env-sourced access token,
// and runtime flags every command might need to read.
//
// Tokens read from env are wrapped in `Redacted` so the value can flow
// through Effects without risking accidental log emission. The actual
// string is extracted only at the HTTP authorization-header site.

import { Context, Effect, Layer, Option, Redacted } from "effect";
import type { FetchImpl } from "../lib/http.ts";

const DEFAULT_API_URL = "https://api.usezombie.com";
const DEFAULT_DASHBOARD_URL = "https://dashboard.usezombie.com";

export interface CliConfigShape {
  readonly apiUrl: string;
  readonly dashboardUrl: string;
  readonly accessToken: Option.Option<Redacted.Redacted<string>>;
  readonly jsonMode: boolean;
  readonly noOpen: boolean;
  // Injectable fetch impl — integration tests pass a stubbed fetch via
  // runCli's RunCliIo, which threads here so HttpClient bypasses
  // globalThis.fetch. Defaults to undefined → globalThis.fetch.
  readonly fetchImpl?: FetchImpl;
}

export class CliConfig extends Context.Tag("CliConfig")<CliConfig, CliConfigShape>() {}

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
  return {
    apiUrl,
    dashboardUrl,
    accessToken:
      envToken !== undefined
        ? Option.some(Redacted.make(envToken))
        : Option.none(),
    jsonMode: false,
    noOpen: false,
  };
};

export const CliConfigLive: Layer.Layer<CliConfig> = Layer.effect(
  CliConfig,
  Effect.sync(resolveCliConfig),
);

export const CliConfigFromValues = (
  overrides: Partial<CliConfigShape> = {},
): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    ...resolveCliConfig(),
    ...overrides,
  });
