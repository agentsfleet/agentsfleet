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
emit nothing.

## Private chat to fleet funnel

| Event | Owner | Trigger |
|---|---|---|
| `draft_message_accepted` | Daemon | A new message is durably accepted, excluding idempotent replays. |
| `draft_message_finished` | Daemon | An accepted message reaches its durable terminal state. |
| `fleet_setup_finished` | Daemon | Setup reaches its durable successful outcome. |
| `fleet_setup_cancelled` | Daemon | Setup cancellation is durable. |
| `fleet_library_published` | Daemon | A new Library revision is committed, excluding idempotent replays. |

Acceptance and completion counts must come from the daemon so terminal-client
activity, closed browser tabs, and blocked analytics scripts cannot turn a
browser click into conversion truth. Join only through opaque identifiers within the authorized workspace.
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
payload.

## Identity lifecycle

`AnalyticsBootstrap` supplies the committed Clerk identity and current route to
[`posthog.ts`](../../ui/packages/app/lib/analytics/posthog.ts).
An unresolved identity or expired component effect prevents event delivery.

The PostHog software development kit (SDK) loads after an idle delay.
Its loaded callback synchronizes the current identity before releasing workspace context, buffered events and the initial pageview.
Authentication routes and sensitive navigation destinations are excluded during initialization, capture and delivery.

[`AnalyticsIdentityGate`](../../ui/packages/app/lib/analytics/identity-gate.ts) records the owner before buffering events or workspace context.
A different resolved owner discards both queues; same-account navigation retains owned data.
Unresolved identities cannot add new unowned telemetry.

A localStorage marker (`uz_analytics_identified`) detects a prior identity after a hard navigation or session expiry.
A signed-out identity resets that state before telemetry resumes; failed resets retain the marker for retry.
Clearing localStorage while keeping cookies can leave a PostHog identity without this marker, since PostHog uses both stores by default.

## Workspace group + person context

`setAnalyticsContext`, called from `ShellControls`, binds the active workspace through `group("workspace", …)`.
It also supplies `workspace_count` and `workspace_plan` through `setPersonProperties`.
Both operations wait for the current identity before buffered product events are released.

Repeated context for the same workspace skips another `group()` call.
Identity reset clears the group, so the next session binds its workspace again.

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

The `header_early_access`, `source_early_access`, and `pricing_early_access` identifiers remain stable for historical comparisons.
Current hosted-access labels and destinations belong to [Website job entrance](website-job-entrance.md#signup-and-installer-boundaries).

The website reads `VITE_POSTHOG_KEY` and `VITE_POSTHOG_HOST` from the existing deployment configuration.
The [bootstrap mapping](../../playbooks/founding/01_bootstrap/02_vercel_env.sh) selects the same development and production PostHog projects as the app.

The funnel is redirect-based — signup completes on the app origin under Clerk, and
the deliberate localStorage-only persistence (the cookie-less posture) does not
cross subdomains — so signup *completion* deliberately has no client event.
Server events: [observability.md](./observability.md) §"PostHog is product analytics, not operations telemetry".
