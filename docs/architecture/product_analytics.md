# Product analytics (PostHog)

PostHog is the product-analytics plane — distinct from Prometheus metrics, the
OpenTelemetry Protocol (OTLP) export, and the Postgres execution-telemetry
store. Two halves write into one PostHog project:

| Half | Owner | Captures |
|---|---|---|
| **Client activation events** | `ui/packages/app` (`posthog-js`) | User-driven dashboard actions (catalog below), autocapture, pageviews, `identify` on Clerk sign-in, `reset` on sign-out. |
| **Server conversion truth** | agentsfleetd (`posthog-zig`, `src/agentsfleetd/observability/telemetry.zig`) | Five state-owning events, and only these five: `FleetTriggered`, `FleetCompleted`, `SignupBootstrapped`, `WorkspaceCreated`, `ServerStarted`. |

Client events stitch to the same person via `identify(clerk_user_id)`. A
conversion that completes server-side — signup completion, workspace creation,
fleet runs — is captured server-side only. Browser events get ad-blocked and
lost on tab close, so the backend is authoritative for them.

`telemetry_events.zig` declares more event types than the five above. A declared
type with no capture site fires nothing; read the capture sites, not the
declarations, when asking what reaches PostHog.

## Client event rules

Single-sourced in `ui/packages/app/lib/analytics/events.ts` (`EVENTS`,
`EventProps`, and the `EVENT_PROP_KEYS` runtime mirror). Naming: snake_case,
object-first past tense (`fleet_created`, `api_key_minted`). Props carry IDs,
names, and enum values only — never a token, raw API key, credential payload,
or free-text from a sensitive field. Call sites import `EVENTS` +
`captureProductEvent`; a grep test fails on any bare event-name literal outside
the catalog. Catalog captures bypass the legacy `sanitizeProps` allowlist (its
closed key set would silently drop event-specific keys) — the `EventProps`
types are the compile-time guard, and the emit path allowlists every payload
against the `EVENT_PROP_KEYS` runtime mirror, so a spread or widened argument
cannot smuggle extra fields. Capture is exception-contained: analytics can
never break the product flow it instruments.

`EVENTS` is the list. It carries two dozen entries across fleets, runners,
keys, secrets, the library and onboarding, and it grows with the dashboard — so
reading it beats reading a copy here that stops being true the next time someone
adds a surface.

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

## Workspace group + person context

`setAnalyticsContext` (bound from `ShellControls`) binds the active workspace as a
PostHog **group** (`group("workspace", …)`), so every subsequent event and
pageview is sliceable per workspace — mirroring Supabase Studio's
`$groups: { organization, project }`. The same call sets org-level **person
properties** (`workspace_count`, `workspace_plan`) on the identified user via
`setPersonProperties`. Both are best-effort and ride the same pending-queue
deferral as identify/reset: context that races the lazy posthog-js chunk load
is queued and flushed at init *after* identify, so person properties attach to
the identified user, never a pre-identify anonymous id. A redundant `group()`
for the already-bound workspace is skipped; `resetAnalyticsIdentity()` rebinds
on the next session.

## Website (marketing)

`ui/packages/website` emits `signup_started` + `navigation_clicked` only. The
funnel is redirect-based — signup completes on the app origin under Clerk, and
the deliberate localStorage-only persistence (the cookie-less posture) does not
cross subdomains — so signup *completion* deliberately has no client event;
`SignupBootstrapped` (server) is the conversion truth.
