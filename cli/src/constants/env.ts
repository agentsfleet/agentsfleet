// The one declaration site for every environment-variable name the CLI spells
// as a string literal. Renaming an env var is a breaking change for anyone who
// exports it, so the name belongs in one place where that break is visible in
// a single diff. `test/constants-env.unit.test.ts` fails on a second spelling
// anywhere under `src/`, quoted or dotted.
//
// Four names are deliberately absent because they are only ever read as dotted
// properties, never spelled as a string: `AGENTSFLEET_TELEMETRY_DISABLED` and
// `DO_NOT_TRACK` (`services/telemetry/consent.ts`), `AGENTSFLEET_TELEMETRY_DEBUG`
// (`services/telemetry/runtime.layer.ts`) and `AGENTSFLEET_NO_RETRY`
// (`lib/http-retry.ts`).

// The API base the CLI talks to. Read by `argv-scan` after `--api`, so the
// flag wins and this is the fallback.
export const API_URL_ENV = "AGENTSFLEET_API_URL" as const;

// The service-auth env var. A machine principal (an `agt_t…` tenant API key)
// exported here authenticates the CLI without a browser login, and — by the
// env-wins precedence in `resolveToken` — takes priority over a stored login
// JWT. This is the only env-sourced bearer the CLI reads; the unprefixed
// `API_KEY` and `AGENTSFLEET_TOKEN` names are not accepted.
export const API_KEY_ENV = "AGENTSFLEET_API_KEY" as const;

// Overrides the config directory that otherwise defaults to
// `~/.config/agentsfleet`. Every test that needs an isolated state dir sets
// this rather than writing to the developer's real home.
export const STATE_DIR_ENV = "AGENTSFLEET_STATE_DIR" as const;

// Overrides the dashboard the CLI prints links to. Unset, it derives from the
// API URL, so the two cannot disagree by accident.
export const DASHBOARD_URL_ENV = "AGENTSFLEET_DASHBOARD_URL" as const;

// PostHog overrides. Defaults ship in `services/config.ts`; these exist so a
// self-hosted deployment can point telemetry at its own collector.
export const TELEMETRY_POSTHOG_KEY_ENV = "AGENTSFLEET_TELEMETRY_POSTHOG_KEY" as const;
export const TELEMETRY_POSTHOG_HOST_ENV = "AGENTSFLEET_TELEMETRY_POSTHOG_HOST" as const;

// The industry convention (no-color.org): any value disables ANSI colour.
export const NO_COLOR_ENV = "NO_COLOR" as const;
