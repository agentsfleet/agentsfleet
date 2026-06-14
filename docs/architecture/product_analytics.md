# Product analytics (PostHog)

PostHog is the product-analytics plane — distinct from Prometheus metrics, the
OpenTelemetry Protocol (OTLP) export, and the Postgres execution-telemetry
store. Two halves write into one PostHog project:

| Half | Owner | Captures |
|---|---|---|
| **Client activation events** | `ui/packages/app` (`posthog-js`) | User-driven dashboard actions (catalog below), autocapture, pageviews, `identify` on Clerk sign-in, `reset` on sign-out. |
| **Server conversion truth** | agentsfleetd (`posthog-zig`, `src/agentsfleetd/observability/telemetry.zig`) | State-owning events: `ZombieTriggered`/`Completed`, `SignupBootstrapped`, `AuthLoginCompleted`, billing. |

Client events stitch to the same person via `identify(clerk_user_id)`. A
conversion that completes server-side (billing, signup completion, zombie runs)
is captured server-side only — browser events get ad-blocked and lost on tab
close, so the backend is authoritative for them.

## Client event catalog

Single-sourced in `ui/packages/app/lib/analytics/events.ts` (`EVENTS`,
`EventProps`, and the `EVENT_PROP_KEYS` runtime mirror). Naming: snake_case,
object-first past tense (`zombie_created`, `api_key_minted`). Props carry IDs,
names, and enum values only — never a token, raw API key, credential payload,
or free-text from a sensitive field. Call sites import `EVENTS` +
`captureProductEvent`; a grep test fails on any bare event-name literal outside
the catalog. Catalog captures bypass the legacy `sanitizeProps` allowlist (its
closed key set would silently drop event-specific keys) — the `EventProps`
types are the compile-time guard, and the emit path allowlists every payload
against the `EVENT_PROP_KEYS` runtime mirror, so a spread or widened argument
cannot smuggle extra fields. Capture is exception-contained: analytics can
never break the product flow it instruments.

| Event | Fires when | Props |
|---|---|---|
| `zombie_created` | the dashboard install form succeeds | `zombie_id` |
| `runner_token_minted` | the add-runner dialog mints a registration token (the runner goes live later, host-side) | `runner_id`, `sandbox_tier` |
| `api_key_minted` | the API-key dialog succeeds | `api_key_id` (never the key) |
| `model_added` | the models wizard saves a Bring-Your-Own-Key (BYOK) setup (a platform-defaults reset emits nothing) | `provider`, `mode`, `model?` |
| `credential_added` | the credentials-page form succeeds | `credential_name` (never `data_json`) |
| `approval_resolved` | approve/deny actually resolves the gate (the `already_resolved` race emits nothing) | `gate_id`, `decision`, `has_reason` |

Events fire on success only — validation failures and aborted actions emit
nothing.

## Identity lifecycle

`AnalyticsBootstrap` (root layout) identifies on Clerk sign-in and calls
`resetAnalyticsIdentity()` exactly once when a signed-out render still carries
a prior identity. Staleness is detected via the module cache plus a
localStorage marker (`uz_analytics_identified`), which also covers hard
navigations and session expiry — cases where the sign-out edge is never
observable in-page. Anonymous visitors never carry the marker, so reset never
churns anonymous ids. Identity work that races the lazy posthog-js chunk load
is deferred, not dropped: a racing reset keeps its marker until the client can
actually reset, and a racing identify is queued and flushed at init. Accepted
residual risk: a user who clears localStorage but keeps cookies can retain a
posthog identity with no marker (the app's default posthog persistence is
localStorage+cookie).

## Website (marketing)

`ui/packages/website` emits `signup_started` + `navigation_clicked` only. The
funnel is redirect-based — signup completes on the app origin under Clerk, and
the deliberate localStorage-only persistence (the cookie-less posture) does not
cross subdomains — so signup *completion* deliberately has no client event;
`SignupBootstrapped` (server) is the conversion truth.
