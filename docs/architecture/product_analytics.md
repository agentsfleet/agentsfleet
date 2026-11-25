---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Product analytics: the PostHog events and what they may carry

> Read this when you add, rename or read a product event in the daemon, the dashboard or the marketing site. Parent: [`README.md`](./README.md) · Operations telemetry: [observability](./observability.md).

## What it is

PostHog holds product events: what people do and which steps they finish.
It is separate from the OpenTelemetry Protocol (OTLP) logs, traces and metrics, and from the money in Postgres.
Three producers write into one PostHog project.

| Producer | Library | Captures | Declared in |
|---|---|---|---|
| `agentsfleetd` | `posthog-rs` | conversions that complete on the server | `af_observability`'s `product` module |
| Dashboard, `ui/packages/app` | `posthog-js` | dashboard actions, autocapture, pageviews, `identify` and `reset` | `ui/packages/app/lib/analytics/events.ts` |
| Marketing site, `ui/packages/website` | `posthog-js` | signup, navigation and interaction clicks | `ui/packages/website/src/analytics/posthog.ts` |

## Why it exists

Browser events get blocked by ad blockers and lost when a tab closes.
So every conversion that completes on the server, such as creating a fleet, is captured by the daemon alone.
Dashboard events add what only the browser sees, stitched to the same person by `identify`.

## How it behaves

### Rules for every event

- **Names** are snake_case, object first, past tense, such as `api_key_minted`.
- **Properties** are identifiers, fixed words and enum values only. Never send a token, a key, message text, a name, pinned memory, a URL or a secret.
- **Typing.** Dashboard call sites import `EVENTS` and call `captureProductEvent`, whose event parameter is typed `EventName`, so a misspelled event fails the typecheck.
- **Allowlist.** The dashboard's emit path also filters every payload against `EVENT_PROP_KEYS`, so a widened argument cannot add fields.
- **Success only.** Mutation events fire after success; a validation failure or an aborted action sends nothing.
- **Never blocking.** A capture failure never breaks the action it records.

### Daemon events

The client is optional: with no project key, `Analytics::silent` sends nothing.
With a key, `posthog-rs` flushes every 100 events or 5 s, and a full 10,000-event queue drops new events with one warning.
`capture` returns once the event is queued, so code says `submitted`, never `delivered`.

| Event | Fires when | Properties |
|---|---|---|
| `fleet_created` | the create commits, once per fleet; a replayed create sends nothing | `fleet_id`, `origin` (`cli` or `dashboard`) |
| `fleet_access_changed` | a grant is requested, approved, denied or revoked | `fleet_id`, `kind` (`secret` or `integration`), `change` |
| `fleet_completed` | a run's report settles for the first time; a repeated report sends nothing | `workspace_id`, `fleet_id`, `turn_id`, `tokens`, `wall_ms`, `exit_status`, `time_to_first_token_ms`; `$insert_id` is the SHA-256 of `fleet_id`, a zero byte and `turn_id` |
| `server_started`, `worker_started` | the daemon or a worker starts | `port`, or `concurrency` |
| `startup_failed` | boot fails | `command`, `reason`, `error_code` |
| `workspace_created` | a workspace is created | `workspace_id`, `tenant_id`, `request_id` |
| `auth_login_completed`, `auth_rejected` | sign-in succeeds, or a token is refused | `session_id` or `reason`, and `request_id` |
| `entitlement_rejected` | a request is refused at an entitlement boundary | `workspace_id`, `boundary`, `reason_code`, `request_id` |
| `api_error` | a request fails | `error_code`, `message`, `workspace_id`, `request_id` |
| `invite_email_sent`, `invite_email_failed`, `invite_email_unconfigured` | an invite email is sent, fails, or has no mail service | `tenant_id`, `invite_id`, `attempt` |
The daemon reports each event after the commit that made it true, attributed to the person's identity-provider subject.
Scheduler mechanics, logs, spans, heartbeats, renewals and activity frames never become product events.
A trigger added or removed is the operator log line `trigger_changed`, not a product event ([observability](./observability.md#the-chat-paths-signals)).

### Dashboard events

`EVENTS` in `events.ts` is the list, and it grows with the dashboard; read it rather than a copy.
It covers workspaces, API keys, models and provider keys, secrets, fleets, fleet memory, integrations, approvals and the error boundary (`dashboard_error_shown`).
The browser adds its pathname to each event and never captures query strings or fragments.

`AnalyticsBootstrap` supplies the signed-in Clerk identity and the current route to `lib/analytics/posthog.ts`.
The PostHog software development kit (SDK) loads after an idle delay, and its loaded callback sets the identity before any buffered event is released.
Authentication routes and sensitive destinations are excluded from capture.

`AnalyticsIdentityGate` records which person owns buffered events, and a different person discards the buffer.
A `uz_analytics_identified` mark in local storage detects a previous identity after a hard navigation, and sign-out resets it.
`setAnalyticsContext`, called from `ShellControls`, binds the active workspace with `group("workspace", …)` and sets `workspace_count` and `workspace_plan`.

### Website events

| Event | Sources | Meaning |
|---|---|---|
| `signup_started` | `header_signup`, `hero_signup`, `pricing_signup`, `footer_signup`, `composer_create_fleet`, `example_create_fleet`, `source_early_access`, `pricing_early_access` | a click into the waitlist; count each entrance separately |
| `interaction_clicked` | `hero_suggestion_incident`, `hero_suggestion_tests`, `hero_suggestion_agent` | one of the three job suggestions; `target` names the example |
| `interaction_clicked` | `cli_install_copy` | Copy on the installer terminal, including a denied clipboard write |
| `interaction_clicked` | `example_read_full` | Read full conversation |
| `navigation_clicked` | `hero_github`, `pricing_quickstart`, `cli_install_guide`, `cli_self_hosted` | repository, quickstart, installation and self-hosting links |
| `navigation_clicked` | `source_github`, `source_self_hosted`, `continuity_docs` | source-strip and documentation links |
| `navigation_clicked` | `composer_example`, `example_back`, `example_repository` | open an example, return home, or view its repository |
| `navigation_clicked` | `pricing_contact` | Tell us about your workflow |
| `navigation_clicked` | `header_home`, `header_nav_product`, `header_pricing`, `header_nav_docs`, `header_nav_source` | header links |
| `navigation_clicked` | `footer_home`, `footer_examples`, `footer_docs`, `footer_agents`, `footer_openapi_json`, `footer_github`, `footer_discord`, `footer_contact`, `footer_privacy`, `footer_terms` | footer links |

Each event carries its `source`, `surface`, a fixed `target` or `mode`, and the pathname.
A `before_send` filter strips URL, referrer, campaign, element and person-property data after PostHog enriches the event.
Feature-flag and remote-configuration requests are off, because they bypass that filter and can carry the first URL.

The visitor's typed request stays in React memory, and a reload clears it.
It never enters URL parameters, analytics, session storage or local storage, and nothing hands it to the app.
The pricing banner has no handler and is marked `ph-no-capture`, and the composer is excluded from autocapture.

Count a button by filtering its explicit event on `source`; never add the autocapture count for the same click.
The site reads `VITE_POSTHOG_KEY` and `VITE_POSTHOG_HOST`, mapped to the app's projects by `playbooks/founding/01_bootstrap/02_vercel_env.sh`.
`VITE_WAITLIST_URL` overrides the waitlist, `https://accounts.agentsfleet.net/waitlist` by default.

## Limits

- **No signup-completion event.** Signup finishes on the app's origin under Clerk, and the site's local-storage-only analytics cannot cross subdomains.
- **One free-text property.** `api_error` carries the error's `message`, so an error sentence must never hold private input.
- **Local storage is half the identity.** Clearing local storage while keeping cookies can leave a PostHog identity without the `uz_analytics_identified` mark.

## Related pages

- [Observability](./observability.md): logs, traces and metrics
- [Web app](./web_app.md): the dashboard that sends client events
- [Billing](./billing.md): money, which never rides an event
