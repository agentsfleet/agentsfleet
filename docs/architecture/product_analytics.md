# Product analytics (PostHog)

PostHog is the product-analytics plane, kept apart from the OpenTelemetry
Protocol (OTLP) export of logs, traces and metrics and from the Postgres
execution-telemetry store. Two halves write into one PostHog project:

| Half | Owner | Captures |
|---|---|---|
| **Client activation events** | `ui/packages/app` (`posthog-js`) | User-driven dashboard actions (catalog below), autocapture, pageviews, `identify` on Clerk sign-in, `reset` on sign-out. |
| **Server conversion truth** | agentsfleetd (`posthog-rs`, `rustd/crates/afd_observability/src/product/telemetry/mod.rs`) | Server events: [observability.md](./observability.md) §"PostHog is product analytics, not operations telemetry". |

Client events stitch to the same person via `identify(clerk_user_id)`. A
conversion that completes server-side — workspace creation, fleet runs — is
captured server-side only. Browser events get ad-blocked and
lost on tab close, so the backend is authoritative for them.

## Client event rules

Single-sourced in `ui/packages/app/lib/analytics/events.ts` (`EVENTS`,
`EventProps`, and the `EVENT_PROP_KEYS` runtime mirror). Naming: snake_case,
object-first past tense (`fleet_created`, `api_key_minted`). Props carry IDs,
names, and enum values only — never a token, raw API key, credential payload,
or free-text from a sensitive field. Call sites import `EVENTS` +
`captureProductEvent`; the `app-event-literal` rule in
`scripts/check_app_source.py` (run by `make lint-all`) fails on any bare
event-name literal outside the catalog. Catalog captures bypass the legacy `sanitizeProps` allowlist (its
closed key set would silently drop event-specific keys) — the `EventProps`
types are the compile-time guard, and the emit path allowlists every payload
against the `EVENT_PROP_KEYS` runtime mirror, so a spread or widened argument
cannot smuggle extra fields. Capture is exception-contained: analytics can
never break the product flow it instruments.

`EVENTS` is the list. It carries two dozen entries across fleets, runners,
keys, secrets, models, approvals, the library and onboarding, and it grows with the dashboard — so
reading it beats reading a copy here that stops being true the next time someone
adds a surface.

Mutation events fire on success only — validation failures and aborted actions
emit nothing. `draft_review_opened` records opening the review drawer; it does
not imply that the review request succeeded or that a fleet was saved.

## Private chat to fleet funnel

| Event | Owner | Trigger |
|---|---|---|
| `draft_message_accepted` | Daemon | A new message is durably accepted, excluding idempotent replays. |
| `draft_message_finished` | Daemon | An accepted message reaches its durable terminal state. |
| `draft_review_opened` | Browser | The operator opens Review setup; only `workspace_id` and an optional `draft_id` are supplied. A source preview can open before a draft exists. |
| `fleet_setup_finished` | Daemon | Setup reaches its durable successful outcome. |
| `fleet_setup_cancelled` | Daemon | Setup cancellation is durable. |
| `fleet_library_published` | Daemon | A new Library revision is committed, excluding idempotent replays. |

Acceptance and completion counts must come from the daemon so terminal-client
activity, closed browser tabs, and blocked analytics scripts cannot turn a
browser click into conversion truth. Keep review openings as a separate intent
step. Join only through opaque identifiers within the authorized workspace.
The daemon reports each step from `afd_authoring` after the commit that made it
true, attributed to the person's identity-provider subject. Every property is a
UUID or a fixed word, and
`rustd/crates/afd_observability/src/product/telemetry/authoring_tests.rs` pins
each event's exact key set.

These events allow only kind, state, outcome, and opaque authorized identifiers.
Never attach message text, titles, Definitions, instructions, URLs, or secret
values. The browser uses the existing event catalog's runtime property allowlist
and automatic pathname context; it does not capture query strings or fragments.
`tests/draft-analytics-privacy.test.ts` exercises that allowlist with a widened
payload, and `tests/workspace-chat/review-analytics.test.tsx` checks the action.

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

`ui/packages/website` emits `signup_started`, `navigation_clicked`, and
`interaction_clicked`. Each explicit event includes a stable `source`, its
`surface`, a fixed `target` or `mode`, and the current pathname. The property
allowlist drops private input; request text, clipboard text, query parameters,
and hashes are never supplied by these handlers. The composer excludes
automatic capture. Capture failures cannot interrupt the visitor's action.

PostHog enriches events before sending them. The final `before_send` filter removes
URL, referrer, campaign, element and person-property payloads after that enrichment.
It retains pathname, catalog properties and required ingestion fields.
Unused feature-flag and remote-configuration requests are disabled because those
requests bypass the event filter and can carry initial URLs.

| Event | Sources | Meaning |
|---|---|---|
| `signup_started` | `header_early_access`, `composer_create_fleet`, `example_create_fleet`, `source_early_access`, `pricing_early_access` | Click intent to enter app signup; count each entrance separately |
| `interaction_clicked` | `hero_suggestion_incident`, `hero_suggestion_tests`, `hero_suggestion_agent` | Click one of the three job suggestions; `target` is the fixed example identifier |
| `interaction_clicked` | `cli_install_copy` | Click Copy on the installer Terminal, including a denied clipboard write |
| `interaction_clicked` | `example_read_full` | Read full conversation |
| `navigation_clicked` | `cli_install_guide`, `cli_self_hosted` | Installation guide and repository self-hosting links |
| `navigation_clicked` | `source_github`, `source_self_hosted`, `continuity_docs` | Source-strip and product-documentation links |
| `navigation_clicked` | `composer_example`, `example_back`, `example_repository` | Explore an example, return home, or inspect its real repository |
| `navigation_clicked` | `pricing_contact` | Tell us about your workflow |
| `navigation_clicked` | `header_nav_product`, `header_nav_docs`, `header_nav_source`, `header_dashboard` | Header product, docs, source, and sign-in links |
| `navigation_clicked` | `footer_use_cases`, `footer_sign_in`, `footer_docs`, `footer_agents`, `footer_github`, `footer_discord`, `footer_contact`, `footer_privacy`, `footer_terms` | Footer destinations |

Filter the explicit event by `source` to count a particular button or link.
Automatic capture remains enabled for general website exploration; do not add
its click count to the explicit count for the same action.
The “$0 to start · $25 credit included” banner has no custom handler and uses `ph-no-capture`.
The existing `header_early_access`, `source_early_access`, and `pricing_early_access` source identifiers remain stable for historical comparisons; their visible labels are Get started, Hosted ↗, and Create your first fleet.

The website reads `VITE_POSTHOG_KEY` and `VITE_POSTHOG_HOST` from the existing deployment configuration.
The [bootstrap mapping](../../playbooks/founding/01_bootstrap/02_vercel_env.sh) selects the same development and production PostHog projects as the app.

The funnel is redirect-based — signup completes on the app origin under Clerk, and
the deliberate localStorage-only persistence (the cookie-less posture) does not
cross subdomains — so signup *completion* deliberately has no client event.
Server events: [observability.md](./observability.md) §"PostHog is product analytics, not operations telemetry".
