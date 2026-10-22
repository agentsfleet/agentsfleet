# Connectors — the registry-driven connector platform

> Parent: [`README.md`](./README.md) · Sibling: [`../AUTH.md`](../AUTH.md) §"OAuth connectors (separate surface — M106, generalized by the M108 registry)" (flow behavior, trust-anchor mechanics, error taxonomy of the shipped providers). · User-facing setup: [docs.agentsfleet.net/fleets/connectors](https://docs.agentsfleet.net/fleets/connectors).
>
> Scope: the platform shape — the compile-time registry + archetype dispatch that makes a new provider a data entry, the callback and event-ingress trust anchors, the bounded-outbound rule for vendor calls, and the connector-vs-integration terminology. Read this before adding a provider or writing any connector outbound call. Flow behavior stays in AUTH.md; this doc owns the invariants that make the flows generic.

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Vocabulary | connector ≠ integration | connector = auth + credential plumbing; integration = a capability built ON the credential | §Terminology |
| Registry | the closed `Provider` enum, 5 variants, declared once by `catalogue!` | adding a provider is one variant + its match arms; never new route or flow code | §The registry |
| Dispatch | on archetype SHAPE, never provider id | exhaustive match on the `Archetype` enum; a missing arm is a compile error | §The registry |
| Archetypes | 2 — `oauth2` · `app_install` | `slack` / `zoho` (multi-data-center) / `jira` / `linear` · `github`; the `api_key` archetype was considered and dropped (M108_002) | §Archetypes |
| Trust anchors | 6 | signed single-use state bound to workspace and starter identity (`UZ-CONN-002`) · user-authorization installation proof (`UZ-CONN-008`) · admin-vault `<provider>-app` bags (`UZ-CONN-001`) · provider signatures · signature proves origin, not producer · the standing integration grant | §Trust anchors |
| GitHub App URLs | 2, different jobs | `/api/connectors/github/callback` on the dashboard (browser install) vs `/v1/ingress/github` on the API (machine events) | §GitHub App |
| Disconnect | internal state only | `DELETE` removes the workspace handle and routing rows; provider authorization remains active | §The registry |
| Binding writes | callback and Disconnect atomic | each commits the sealed grant and the routing rows in one transaction, taking the workspace row lock first, so the two serialise even before a grant exists | §The registry |
| App replay identity | authenticated body digest, per fleet | the unsigned delivery header is diagnostic only; failed fan-out legs retry without duplicating others | §GitHub App |
| Outbound HTTP | a deadline on every vendor call | refusal is `UZ-CONN-003` (502); token exchange 10 s · Slack post 5 s · thread re-read 1.5 s · answer check 3 s | §Bounded outbound |
| Front-door failures | 404 vs 503 | unknown provider → `UZ-CONN-004`; registry id with no `<provider>-app` bag → `UZ-CONN-001`, fail-loud | §Unknown vs unconfigured |

## Traps

Each trap is enforced in its owner section; this list is the index.

- "Slack integration is broken" and "Slack connector is broken" name different layers — keep the vocabulary straight (§Terminology).
- No `if provider == "slack"` exists anywhere in the flow; adding one is a design regression (§The registry).
- A static vendor key (Datadog, Grafana, Fly) is a plain workspace secret, never a registry entry (§Archetypes).
- Generic connect plumbing does not imply generic event behavior — inbound routing follows the provider's real shape (§The registry).
- No pool slot rides a vendor call — credentials load under a short acquire released before the exchange (§Bounded outbound).
- The App private key and webhook secret never enter the lease, runner environment, sandbox, logs, or response frames (§GitHub App).

## Terminology (binding)

| Term | Means | Lives in |
|---|---|---|
| **connector** | auth + credential plumbing for a third-party provider: the connect/callback/status routes, the vaulted per-workspace credential handle, the platform app secrets | `rustd/crates/afd_connector/` (provider catalogue, connect flow, grant handle), `rustd/crates/afd_credential/` (token minting from the handle), and the routes in `rustd/crates/afd_http/src/route/connector.rs` |
| **integration** | a product-facing capability built ON a connector's credential (the Slack resident bot, GitHub fleet triggers, future Zoho/Jira/Linear surfaces) | feature code that consumes the vault handle |

A workspace *connects* a provider once (connector); everything fleets then do with that credential is *integration*. Specs, UI copy, and code comments follow this split — "Slack integration is broken" and "Slack connector is broken" name different layers.

## The registry: a provider is a data entry

The connector registry is the closed `Provider` enum (`rustd/crates/afd_connector/src/provider.rs`), declared once by the `catalogue!` macro, which also generates `Provider::ALL`. `Provider::archetype()` (`rustd/crates/afd_connector/src/registry/mod.rs`) says which flow each variant runs. Adding a provider is one variant plus its match arms — never new route or flow code:

```
            ┌─────────────────────────────────────────────────────────────────────────────┐
            │ catalogue! { Slack, GitHub, Zoho, Jira, Linear } → enum Provider + ALL      │
            │   each variant: id (route segment, vault-key stem) · display name           │
            │ Provider::archetype() → enum Archetype {                                    │
            │       Oauth2(Oauth2Flow{authorize_endpoint, token_endpoint, scopes,         │
            │                         scope_delimiter, extra_query, refresh}),            │
            │       AppInstall(AppInstall{authorize_endpoint, token_endpoint}),           │
            │   }  + event_ingress() · state_binding()                                    │
            │ total match: a variant with no arm does not compile                         │
            └─────────────────────────────────────────────────────────────────────────────┘
   runtime Provider::parse(segment) ── None → 404 UZ-CONN-004 (body names the id)
                                    ── Some → exhaustive match on ARCHETYPE
            ┌───────────────────────────────────────────────────────────────────────┐
            │ generic {provider} handlers: connect · callback · status · disconnect │
            │ per-provider deltas: complete::read arm per provider;                 │
            │                      github {probe, ownership check},                 │
            │                      zoho {multi-data-center token endpoint},         │
            │                      slack {mention events, replies}                  │
            └───────────────────────────────────────────────────────────────────────┘
```

- **Routes are generic over `{provider}`** (API reference › Connectors). The dashboard owns `/api/connectors/{provider}/callback`; Disconnect removes only `agentsfleet` state.
- **Dispatch is on SHAPE, never on provider id.** The archetype tagged-union owns which flow runs; handlers match exhaustively on it (a new archetype cannot land half-wired — the compiler forces every arm). No `if provider == "slack"` exists anywhere in the flow.
- **Invariants are compile-time facts or pinned tests.** Two variants cannot share a name and a variant with a missing arm does not compile. Unit tests pin the rest: every id parses back to its variant, no two display names match, every OAuth connector asks for at least one scope, and no two providers share a signed-state domain or nonce namespace (`provider.rs`, `registry/tests.rs`).
- **A callback and a Disconnect each write both rows in one transaction.** A completing callback commits its sealed grant and its routing row together, so a connect that cannot seal its grant leaves no routing row behind. Disconnect deletes the grant and the routing rows together, so a refused delete leaves both (`afd_connector/src/grant/holding.rs`). Both lock the workspace row first (`identity.workspaces`, `FOR NO KEY UPDATE`, `afd_connector/src/sql.rs`), so a callback and a Disconnect take turns even on a first connect, before the grant's row exists, and end with both rows or neither. The vault and the routing table share one Postgres, so no advisory lock is needed.
- **Inbound routing follows the provider's real shape.** App-level webhooks whose payload carries a stable routing key use `POST /v1/ingress/{provider}`, but the shipped implementation is provider-owned: GitHub has its own `/v1/ingress/github` handler, and its routing statements live in `rustd/crates/afd_ingress/src/app/mod.rs`. Slack keeps `POST /v1/connectors/slack/events` (the `/v1/connectors/{provider}/events` route) because its challenge, retry, timestamp, channel, and thread semantics are load-bearing; that route verifies the signature, answers the challenge, and admits a mention once, under the `slack_mention` admission producer, on the fleet it routes to (`rustd/crates/afd_api_ingress/src/handler/mention/mod.rs`); a mention no subscribed fleet takes reaches the channel's resident, installed on the first one ([`scenarios/slack-incident-responder.md`](./scenarios/slack-incident-responder.md) §"4. Routing a mention"). Jira and Linear have connected credentials but no inbound integration yet. Generic connect plumbing does not imply generic event behavior.

## Archetypes

| Archetype | Flow | Callback carries | Writes | Shipped instances |
|---|---|---|---|---|
| `oauth2` | authorize-redirect → code exchange (deadline-armed) → the provider's `complete::read` arm parses, then the grant is sealed | `code` + `state` | vault handle (+ provider-specific rows, e.g. Slack's `connector.installs`) | `slack`, `zoho` (multi-data-center — the callback's `location` resolves the effective token endpoint), `jira`, `linear` |
| `app_install` | user authorization → discover or verify App installation → `complete::read` (GitHub arm); zero reachable installations REFUSE with `UZ-CONN-008` (no install-page continuation in this tree — see §GitHub App) | `code` + `state`; `installation_id` is optional | vault handle + non-secret connector-install routing row | `github` |

**There is no `api_key` archetype.** One was considered for operator-pasted vendor keys (Datadog, Grafana, Fly) and dropped (M108_002). A static vendor key is just a workspace secret referenced as `${secrets.<name>.<field>}`, not a connector: it never had a connect/callback round-trip or a platform app secret to protect. Those three providers are plain `agentsfleet secret create` entries, never registry entries. The `catalogue!` macro declares five variants (`rustd/crates/afd_connector/src/provider.rs`) — five Open Authorization (OAuth) or app-install connectors, not eight.

## Trust anchors

1. **The signed single-use `state` binds workspace intent and starter identity.** It is keyed with the approval signing secret, carries a keyed identity tag instead of a raw subject, verifies in constant time, and is consumed exactly once. The authenticated callbacks endpoint checks the same identity, `connector:write`, and current workspace access before consuming state. Forged, expired, replayed, or identity-mismatched state returns 400 `UZ-CONN-002`. State does not prove ownership of a provider installation.
2. **GitHub user authorization proves installation access.** The callback exchanges the one-time `code` with the platform `client_id` and `client_secret`. A claimed `installation_id` is probed directly. With no claim, exactly one accessible installation repairs internal state loss. Zero returns 403 `UZ-CONN-008` — install the App, then connect again; the Rust daemon carries no App slug and so no install-page continuation. A claimed `installation_id` on the return is probed with the user token before persistence, without exposing the token to the browser or provider URL. More than one returns 403 `UZ-CONN-008`. The datastore also refuses to move an installation already bound to another workspace.
3. **Platform app secrets live in the admin-workspace vault** as per-provider `<provider>-app` bags (`slack-app`, `github-app`, …) — one app per provider shared across all tenants, catastrophic-if-leaked, never on a per-tenant surface. GitHub's bag carries its App identity, user-authorization client credentials, and App-level webhook secret; an unprovisioned bag fails loud: 503 `UZ-CONN-001`.
4. **Provider signatures authenticate inbound events.** GitHub App traffic is verified against the platform `github-app.webhook_secret`; manual per-fleet webhooks still use the workspace `<source>.webhook_secret`; Slack App events use the platform `slack-app.signing_secret`. No inbound route falls back to Bearer authentication.
5. **A signature proves origin, never the producer.** GitHub permits every
   push-capable identity to create deployment statuses, so every such identity in
   a mapped repository sits inside the trusted producer boundary. The daemon
   verifies the App signature and the installation routing; it does not inspect
   `deployment_status.creator` or attest which system produced the status. A
   scenario that depends on this says so and cites this anchor rather than
   restating it.
6. **The standing integration grant authorises every mint, including a
   repository write.** `connector.integration_grants` holds one row per
   `(fleet_id, service)` — `UNIQUE (fleet_id, service)` — and only an `approved`,
   unrevoked row admits a mint. Save writes that row `pending` and raises one
   approval card; a person's yes on that card is the answer, because the
   reviewed Definition names the integration and its binding names the
   repositories and the access level. A repository write raises no per-event
   card; `agentsfleet grant delete` is the manual stop and takes effect on the
   next event before any provider call, and `budget.daily_nanos` bounds the
   money. The token is still narrowed to the
   declared repositories by `ScopedRequest::for_binding` and the response still
   checked by `Granted::verify` — the grant is the authorisation above that
   scoping, never a replacement for it.

The connector registry owns callback dispatch; provider ingress handlers own event routing once the route segment has selected them. Detailed auth behavior and refusal codes live in [`../AUTH.md`](../AUTH.md) §"OAuth connectors (separate surface — M106, generalized by the M108 registry)".

## GitHub App: platform setup to fleet execution

One GitHub App serves every tenant in an environment. The platform operator configures two different URLs on that App:

```
browser install callback                    machine event ingress
/api/connectors/github/callback             /v1/ingress/github
          │                                      │
          │ relays authenticated completion       │ wakes subscribed fleets
          ▼                                      ▼
identity-bound signed state               GitHub App signature
```

The platform identity lives only in the `agentsfleet-admin` workspace:

```
github-app
├── app_id              public App identifier
├── app_slug            public install-page handle (unread by agentsfleetd)
├── client_id           public user-authorization client identifier
├── client_secret       exchanges one-time user authorization codes
├── private_key_pem     signs App JSON Web Tokens for outbound token minting
└── webhook_secret      verifies inbound App deliveries
```

A workspace administrator selects **Connect**. GitHub authorizes the user first. If the App is already installed and exactly one installation is accessible, `agentsfleet` restores the missing internal binding. If none exists, the connect REFUSES with `UZ-CONN-008`: the App must be installed on GitHub first, and the daemon cannot send the browser there because it reads no `app_slug`.

```
signed state ──────────────────────────────── proves intended workspace and starter identity
one-time code → GitHub user token
              ├─ claimed id → GET /user/installations/{id}/repositories
              └─ no claim   → GET /user/installations?per_page=2
                               0 → 403 UZ-CONN-008 (install on GitHub first)
                               1 → restore internal binding
                              >1 → 403, no arbitrary organisation choice
                                          │
                                          ▼
                         conditional datastore write
                         same workspace: create/reconnect
                         other workspace: 403, no mutation
```

A callback completion writes the vault handle and the reverse-routing rows in one transaction. **Disconnect** deletes both in one transaction too, and the two serialise on the workspace row, which exists before a first Connect writes a handle (§"The registry: a provider is a data entry"). A handle a model entry still names is not deleted: Disconnect answers 409 `UZ-VAULT-004` with `current_state: "referenced"`. Disconnect leaves the GitHub App and repository access installed. A later **Connect** can therefore reconcile external and internal state after a datastore rebuild.

The user token is discarded after the current callback, always: the Rust daemon carries no App slug, so there is no App-install continuation to hold it for. Zero reachable installations is a refusal (`UZ-CONN-008`), not a redirect to GitHub's install page, as M187_001's Discovery records. After the identity, workspace, and installation checks pass, the callbacks endpoint writes both records on one database connection:

```
workspace vault                          connector.installs
github = {                               provider = github
  integration: github,                   external_account_id = installation_id
  installation_id                       workspace_id = connected workspace
}                                        credentials = NONE
```

The encrypted handle supports outbound token minting. The connector-install row is deliberately non-secret and supports inbound `installation.id → workspace` routing. Neither row alone is sufficient. A callback completion failure rolls back both rows and leaves the workspace disconnected.

### Repository and event subscriptions belong to fleets

The App installation chooses the maximum repository set GitHub will permit. Each fleet then declares the smaller set it wants to receive:

```yaml
triggers:
  - type: webhook
    source: github
    events: [pull_request]
    repositories: [acme/payments]
```

For App traffic, `repositories` is fail-closed: omission means the fleet receives no App delivery. The omission remains valid for the existing manual per-fleet GitHub route, whose URL already identifies the target fleet. This distinction prevents an App installed across an organisation from waking every GitHub fleet for every repository. The repository match is case-insensitive, as GitHub's is. `events` answers the other way from `repositories`: a trigger with no `events` list admits every event (`rustd/crates/afd_ingress/src/binding/mod.rs`).

### What the event belongs to

A GitHub App delivery belongs to a GitHub installation and repository. It does **not** arrive carrying an `agentsfleet` user, workspace, or fleet identifier. Those are derived inside `agentsfleet`:

```
GitHub account or organisation
└── App installation 10250042
    ├── repository: acme/payments
    │   └── event: pull_request opened
    └── repository: acme/catalog

installation 10250042
        │ callback-created reverse map
        ▼
agentsfleet workspace W
        │ repository + event + approved-grant intersection
        ├── fleet A subscribes to acme/payments + pull_request  → receives it
        ├── fleet B subscribes to acme/payments + pull_request  → receives it
        ├── fleet C subscribes to acme/catalog + pull_request   → does not receive it
        └── fleet D has no approved GitHub grant                → does not receive it
```

The user matters during setup: they choose a workspace, authorize the App installation, install or configure a fleet, and approve its GitHub grant. Once setup is complete, event routing is machine-to-machine and uses persisted relationships rather than the user's browser session.

This gives each layer one job:

| Layer | Owns |
|---|---|
| GitHub App installation | Maximum repositories GitHub permits the App to see |
| Workspace connection | Installation-to-workspace identity and the outbound credential handle |
| Fleet trigger | Explicit repository and event subscription |
| Integration grant | Whether that fleet may use GitHub |
| Admission row (`fleet.admissions`) | Exactly-once enqueue per matching fleet for one GitHub delivery |

### Where a grant comes from

A grant is **requested at Save**: when a saved fleet's readiness review finds a
declared credential's grant missing, setup activation asks for it
(`rustd/crates/afd_authoring/src/creation/activate.rs`). The request writes a
`pending` `connector.integration_grants` row and raises an approval card in one
transaction (`rustd/crates/afd_approval/src/grant_sql/mod.rs`), and the fleet stays
`installing` until every readiness check, the grant included, is ready
(`rustd/crates/afd_authoring/src/creation/progress.rs`). The lease path asks the
same way the first time a delivery reaches a credential with no grant
(`rustd/crates/afd_fleet/src/lease/deliver/mod.rs`).

Either way, the decision belongs to the approval-gate machine this codebase already ships: an inbox, a detail page with an evidence tree, resolve buttons, a webhook,
a timeout sweeper, and an append-only audit. **A gate is a per-event decision; a
grant is the standing answer that outlives the run.** The gate asks; the grant
remembers. Resolving the gate as approved flips the grant and the gate in one
statement, so the two cannot disagree; any non-approval outcome drives the grant
to `revoked` rather than back to `pending`, which nothing would re-raise.

Where origination runs is load-bearing. The App ingress query inner-joins on `status = 'approved'`, so a
fleet that cannot obtain a grant writes no event, takes no lease, and reports
nothing — it goes silently inert rather than failing visibly. An origination
path reachable only with a credential the fleet does not hold produces exactly
that silence.

A lease is the last checkpoint. A credential with a mintable handle but no
approved grant **parks the event**.

Parked admissions stay out of runner claims until gate resolution clears the
park stamp. A revoked grant ends the event with `grant_denied`.

An incoming delivery follows this order:

```
GitHub App delivery
  installation.id + repository.full_name + event + diagnostic delivery identifier
        │
        ▼
verify platform webhook signature BEFORE reading routing fields
  (a `ping` answers `pong` only after the signature verifies)
        │
        ▼
installation.id → connector.installs → workspace
        │
        ▼
active fleets in that workspace
  ∩ source=github
  ∩ repositories contains repository.full_name
  ∩ events admits the incoming event
  ∩ approved GitHub integration grant
        │
        ▼
fleet.admissions row (body digest + fleet) → XADD fleet:{id}:events
```

Multiple fleets may intentionally subscribe to the same repository and event. Replay protection is therefore per authenticated payload body and fleet, not global. The signature-covered body digest is the replay identity; the unsigned delivery header is diagnostic only.

If one fan-out leg fails before its admission commits, redelivery retries that leg. Fleets already admitted answer `replayed`; redelivery admits only the missing fleets. More than 100 matching fleets (`MAX_FANOUT`) refuses the whole delivery rather than waking an arbitrary hundred (`rustd/crates/afd_ingress/src/app/mod.rs`).

**That redelivery is not GitHub's.** GitHub states plainly that it "does not automatically redeliver failed webhook deliveries": a delivery fails when the receiver is down or takes longer than **ten seconds** to answer, and recovering it is a manual click in the App's delivery log or an operator script walking the REST API for failed deliveries. This page previously credited the recovery to "GitHub's retry", which does not exist, and the correction matters because it moves the boundary of what is recoverable:

| Where the fan-out fails | What survives |
|---|---|
| After a leg's admission row commits | Durable. The committed ledger row remains available for the next eligible runner claim; wake notifications are best effort. |
| Before it commits — the failing leg, and every leg after it in the loop | **Nothing.** No row, no entry, and no sender that will ask again. |

So the durable-acceptance guarantee covers work this deployment ACCEPTED, and the window before acceptance is the one place a GitHub event can be lost outright. Two consequences follow. The ten-second budget is a hard deadline rather than a target, and it is shared across signature verification, fleet resolution, and one admission per subscribed fleet — fan-out width spends it. And deduplication is still worth every line, because the redelivery it absorbs is a *human* clicking Redeliver with no idea whether the first attempt landed, which is exactly when a duplicate review would otherwise appear on a pull request.

`deployment_status` and repair-branch deliveries are classified unsupported and dropped until the repair-evidence writer is ported; the repair sweeper waits rather than acting on evidence it never received (`rustd/crates/afd_api_ingress/src/handler/webhook/app_route.rs`).

Slack is the contrast and the reason this page cannot generalise: its retry semantics are load-bearing precisely because Slack does retry.

### Credential use remains separate from event receipt

Receiving a signed event does not hand GitHub credentials to a fleet. When a leased fleet later calls the GitHub API through `${secrets.github.token}`, the runner-token plane asks `agentsfleetd` to mint. The daemon derives the fleet and workspace from the lease, rechecks the approved integration grant, loads the workspace installation handle, signs with the platform private key, exchanges for a short-lived installation token, and returns that token for the tool call. The runner keeps the token for the lease, mints again 30 s before it expires, and masks it out of every response a tool returns (`rustd/crates/afr_egress/src/vault/mod.rs`). The App private key and webhook secret never enter the lease, runner environment, sandbox, logs, or response frames. The minted token's scope follows the binding (`afd_credential/src/credential/github/request.rs`): a read binding gets `contents: read` and no `pull_requests` at all; a repair binding adds what a push and a draft Pull Request need, never `workflows`; the `actions` and `checks` reads are requested only where the installation holds them, so a missing one answers 403 to the fleet. The runner's egress client follows no redirect (`afr_egress/src/network/mod.rs`), so a job log that GitHub serves by redirect is not fetched.

### Provider impact

| Provider | Connect credential | Inbound events after M102_005 |
|---|---|---|
| <img src="https://cdn.simpleicons.org/github" width="14" alt="" /> GitHub | App installation handle | App ingress routes by installation + repository + event + grant; manual per-fleet webhook remains available |
| <img src="https://api.iconify.design/logos/slack-icon.svg" width="14" alt="" /> Slack | bot token from OAuth | specialized events route: signature, challenge, and mention routing to one `slack_mention` event (`rustd/crates/afd_api_ingress/src/handler/mention/mod.rs`) |
| <img src="https://cdn.simpleicons.org/zoho" width="14" alt="" /> Zoho Desk | OAuth refresh handle, multi-data-center token endpoint | no inbound integration in this workstream |
| <img src="https://cdn.simpleicons.org/jira" width="14" alt="" /> Jira | OAuth refresh handle | no inbound integration in this workstream |
| <img src="https://cdn.simpleicons.org/linear" width="14" alt="" /> Linear | OAuth refresh handle | no inbound integration in this workstream |

## Bounded outbound: every vendor call has a deadline

Every connector vendor call runs under a deadline set at its call site. A callback exchange past its deadline, or one that cannot reach the vendor, answers `UZ-CONN-003` (502); background paths log the same code and retry.

- **Deadlines are named per call class.** Token exchange and credential mint: 10 s, a client-wide total timeout that covers connect and the Transport Layer Security (TLS) handshake (`rustd/crates/agentsfleetd/src/credentials.rs`, `EXCHANGE_TIMEOUT`). Slack post: 5 s (`rustd/crates/afd_outbound/src/slack/mod.rs`, `POST_DEADLINE`). Slack thread re-read: 1.5 s (`rustd/crates/afd_connector/src/slack/replies/mod.rs`, `READ_DEADLINE`). Slack answer check: 3 s (`rustd/crates/afd_connector/src/slack/answered/mod.rs`, `ANSWER_CHECK_DEADLINE`).
- **The exchange and the mint hold separate clients.** A slow connect exchange cannot consume the connection slots a credential mint needs (`rustd/crates/agentsfleetd/src/credentials.rs`).
- **No pool slot rides a vendor call.** Credentials load under a short acquire released before the exchange; the events ingress pre-loads the bot token and returns its slot before the thread re-read (closes a priority-1 review finding on merged Pull Request #468).

## Unknown vs unconfigured (the two front-door failures)

An unknown provider (`UZ-CONN-004`) and a registry id with no `<provider>-app` bag (`UZ-CONN-001`) are different failures ([error codes](https://docs.agentsfleet.net/api-reference/error-codes#UZ-CONN-004)).

## Adding a provider (the recipe)

1. One `Provider` variant in the `catalogue!` macro, with its id and display name (RULE UFS) — the id is simultaneously the route segment, the vault-key stem (`<provider>-app` for the platform app, the bare `<provider>` for the workspace grant: `Provider::app_key` and `Provider::grant_key` in `rustd/crates/afd_connector/src/provider.rs`), and the registry id.
2. One arm in `Provider::archetype()` — `Oauth2Flow` (endpoints, scopes, delimiter, extra query, refresh) or `AppInstall` (authorize + token endpoints) — plus arms in `state_binding()` and `event_ingress()`.
3. The provider's arm in `complete::read`, which is where its answer is parsed into a grant. The registry holds no per-provider hook functions: it dispatches on the archetype enum and matches per provider.
4. Provision the `<provider>-app` bag in the admin vault. (An operator-supplied vendor key with no browser round-trip — Datadog/Grafana/Fly's shape — isn't a connector at all; it's a plain workspace secret, `agentsfleet secret create`, never a registry entry.)
5. Tests: the generic-route suites already cover the flow; add hook-level tests for the provider's parse/persist deltas.

No route, matcher, scope, invoke, or OpenAPI edit — the `{provider}` form already covers the new id.
