// The one declaration site for every environment-variable name the CLI spells
// as a string literal.
//
// Before this file the three names below were spelled in sixteen places —
// once each in `lib/config-dir.ts`, `services/config.ts` and
// `program/entry/argv-scan.ts`, and thirteen more times across the test and
// acceptance trees, each with its own local identifier (`ENV_API_URL`,
// `API_URL_ENV_KEY`, `ENV_STATE_DIR`, `STATE_DIR_ENV_KEY`, …). Two of those
// files carried a comment claiming to be the single declaration site while
// four siblings said the same thing. `audits/ufs.sh` never caught it: it
// checks for a literal repeated inside one file, and each copy sat alone.
//
// Renaming an env var is a breaking change for anyone who exports it, so the
// name belongs in one place where that break is visible in a single diff.
//
// Two names are deliberately absent: `services/telemetry/consent.ts` reads
// `AGENTSFLEET_TELEMETRY_DISABLED` and `DO_NOT_TRACK` as dotted properties
// (`env.DO_NOT_TRACK`), never as a literal, so there is no string here to
// drift out of sync.

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
