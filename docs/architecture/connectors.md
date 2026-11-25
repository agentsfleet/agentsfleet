# Connectors — the registry-driven connector platform

> Parent: [`README.md`](./README.md) · Sibling: [`../AUTH.md`](../AUTH.md) §"OAuth connectors" (flow behavior, trust anchors, error taxonomy). · User-facing setup: [docs.agentsfleet.net/fleets/connectors](https://docs.agentsfleet.net/fleets/connectors).

> Scope: the platform shape. The compile-time registry and archetype dispatch make a new provider a data entry. The callback and event-ingress trust anchors, the bounded-outbound rule for vendor calls and the connector-vs-integration terminology complete it.

> Read this before adding a provider or writing any connector outbound call. Flow behavior stays in AUTH.md; this page owns the invariants that make the flows generic.

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Vocabulary | connector ≠ integration | connector = auth + credential plumbing; integration = a capability built ON the credential | §Terminology |
| Registry | the closed `Provider` enum, 5 variants, declared once by `catalogue!` | adding a provider is one variant + its match arms; never new route or flow code | §The registry |
| Dispatch | on archetype SHAPE, never provider id | exhaustive match on the `Archetype` enum; a missing arm is a compile error | §The registry |
| Archetypes | 2: `oauth2` · `app_install` | `slack` / `zoho` (multi-data-center) / `jira` / `linear` · `github`; the `api_key` archetype was considered and dropped (M108_002) | §Archetypes |
| Trust anchors | 6 | signed single-use state bound to workspace and starter identity (`UZ-CONN-002`) · user-authorization installation proof (`UZ-CONN-008`) · admin-vault `<provider>-app` bags (`UZ-CONN-001`) · provider signatures · signature proves origin, not producer · the standing integration grant | §Trust anchors |
| GitHub App URLs | 2, different jobs | `/api/connectors/github/callback` on the dashboard (browser install) vs `/v1/ingress/github` on the API (machine events) | §GitHub App |
| Disconnect | internal state only | `DELETE` removes the workspace handle and routing rows; provider authorization remains active | §The registry |
| Binding writes | each operation is atomic; concurrent Connect and Disconnect are unordered | each commits its vault and routing changes together; a first-connect race can leave a partial binding | §The registry |
| App replay identity | authenticated body digest, per fleet | the unsigned delivery header is diagnostic only; failed fan-out legs retry without duplicating others | §GitHub App |
| Grants | one row per `(fleet_id, service)`, asked for by the fleet | `request_access` raises the approval card; a signed-in person approves once; a run holds only approved grants | §Where a grant comes from |
| Triggers | rows the fleet adds, kinds `webhook` · `app_event` · `mention` | `fleet.triggers`, unique `(fleet_id, kind, source)`; `trigger_add` / `trigger_list` / `trigger_remove`; every read keeps its `workspace_id` predicate | §Fleet triggers |
| Slack mentions | one fleet per mention, or a fixed notice | the addressed fleet or the sole eligible subscriber gets one `slack_mention` turn; the daemon posts the answer in the thread | §Slack mentions |
| Outbound HTTP | a deadline on every vendor call | refusal is `UZ-CONN-003` (502); token exchange 10 s · Slack post 5 s · thread re-read 1.5 s · answer check 3 s | §Bounded outbound |
| Front-door failures | 404 vs 503 | unknown provider → `UZ-CONN-004`; registry id with no `<provider>-app` bag → `UZ-CONN-001`, fail-loud | §Unknown vs unconfigured |

## Traps

Each trap is enforced in its owner section; this list is the index.

- "Slack integration is broken" and "Slack connector is broken" name different layers; keep the vocabulary straight (§Terminology).

- No `if provider == "slack"` exists anywhere in the flow; adding one is a design regression (§The registry).

- A static vendor key (Datadog, Grafana, Fly) is a plain workspace secret, never a registry entry (§Archetypes).

- Generic connect plumbing does not imply generic event behavior: inbound routing follows the provider's real shape (§The registry).

- No pool slot rides a vendor call; credentials load under a short acquire released before the exchange (§Bounded outbound).

- The App private key and webhook secret never enter the run, runner environment, sandbox, logs, or response frames (§GitHub App).

- An `app_event` row that names no repository receives no App delivery (§GitHub App).

- Receiving an event needs no grant; calling the provider does. A fleet with an `app_event` row but no approved `github` grant is woken and cannot mint a token (§GitHub App).

- A webhook row never takes the source `approval` or `github` (§Fleet triggers).

- A page a run reads can ask the fleet to add a trigger. The row shows under "Wakes on" on the fleet page, where a person removes it (§Fleet triggers).

## Terminology (binding)

| Term | Means | Lives in |
|---|---|---|
| **connector** | auth + credential plumbing for a third-party provider: the connect/callback/status routes, the vaulted per-workspace credential handle, the platform app secrets | `afd_connector` (provider catalogue, connect flow, grant handle), `afd_credential` (token minting from the handle), and the connector routes in `afd_http` |
| **integration** | a product-facing capability built ON a connector's credential (configured Slack fleet mentions, GitHub fleet triggers, future Zoho/Jira/Linear surfaces) | feature code that consumes the vault handle |

A workspace *connects* a provider once (connector); everything fleets then do with that credential is *integration*. Specs, UI copy, and code comments follow this split. "Slack integration is broken" and "Slack connector is broken" name different layers.

## The registry: a provider is a data entry

The connector registry is the closed `Provider` enum in `afd_connector`, declared once by the `catalogue!` macro, which also generates `Provider::ALL`. `Provider::archetype()` says which flow each variant runs. Adding a provider is one variant plus its match arms, never new route or flow code.

Each variant carries its id (the route segment and vault-key stem) and a display name. `Archetype` is a two-variant enum. `Oauth2` carries the authorize and token endpoints, scopes, delimiter, extra query and refresh rule; `AppInstall` carries the authorize and token endpoints.

`event_ingress()` and `state_binding()` complete the per-provider facts.

At runtime `Provider::parse(segment)` answers `None`, a 404 `UZ-CONN-004` whose body names the id, or `Some`, which dispatches on the archetype. The generic `{provider}` handlers are connect, callback, status and disconnect.

The per-provider deltas are each provider's `complete::read` arm and GitHub's probe and ownership check. Zoho's multi-data-center token endpoint and Slack's mention events and replies are deltas too.

- **Routes are generic over `{provider}`** (API reference › Connectors). The dashboard owns `/api/connectors/{provider}/callback`; Disconnect removes only `agentsfleet` state.

- **Dispatch is on SHAPE, never on provider id.** The archetype tagged-union owns which flow runs. Handlers match exhaustively on it, so a new archetype cannot land half-wired. No `if provider == "slack"` exists anywhere in the flow.

- **Invariants are compile-time facts or pinned tests.** Two variants cannot share a name. A variant with a missing arm does not compile.

  Unit tests pin the rest. Every id parses back to its variant, no two display names match, and every OAuth connector asks for at least one scope. No two providers share a signed-state domain or nonce namespace.

- **Each callback and Disconnect commits its own vault and routing changes together.** A failure rolls back that operation's changes. Neither takes a shared workspace lock (`afd_connector`'s grant module).

Concurrent first Connect and Disconnect can leave a vaulted handle without its routing row.
Complete a fresh Connect to restore the binding, or Disconnect again when removal is intended.

- **Inbound routing follows the provider.** GitHub's app webhook uses `POST /v1/ingress/github` and its own routing statements in `afd_ingress`.

  Slack keeps `POST /v1/connectors/slack/events` for its signature, challenge, retry, channel and thread rules.
  Among fleets with a `mention` row for the channel, only the addressed fleet or the sole subscriber receives a `slack_mention` turn.
  A channel no row names returns `channel_not_configured` without creating a fleet or calling a model.

  [Slack mention routing](#routing-a-mention) owns selection and fixed notices.
  Jira and Linear have connected credentials but no inbound integration yet.
  Shared connection storage does not imply shared event behavior.

## Workspace connection reads and browser returns

Connections belong to the workspace. An authorized
`GET /v1/workspaces/{workspace_id}/connectors` returns every registered provider's
identifier, display name, archetype, configured flag, connected flag and public
account label. The service reads deployment app names and a bounded batch of that
workspace's registered grant names concurrently.

It checks each grant's matching
connector marker and returns no tokens. A missing, unrelated or malformed handle
is disconnected. Storage failures remain errors instead of invented disconnected
states (`afd_connector`'s connection and grant catalogue modules).

The connection page streams its heading before this single list finishes. It
makes no separate GitHub or Slack status read. Connected means a saved
authorization; status loading makes no provider call and cannot detect upstream
revocation.

The provider's granted permissions and implemented actions still
limit what a fleet can do.

Browser callbacks return to `/api/connectors/{provider}/callback` on the web app.
The API's `/v1/connectors/{provider}/callback` requires an authenticated POST.
The web relay waits for the API's answer and never retries the spent code.

Failures lead to a safe recovery page with a validated workspace return link;
codes, state and vendor response bodies do not appear there. The workspace cookie
is a navigation hint and grants no access. The API still verifies state, starter
identity and workspace access before saving a grant.

Jira, Linear and Zoho handles retain refresh tokens and account metadata. Their
initial access token and expiry are unused by the credential broker and are not
stored. The broker mints its own access token and caches that token's expiry.

This keeps a large initial vendor token from overflowing the vault's bounded
credential object (`afd_connector`'s grant parser).

## Archetypes

| Archetype | Flow | Callback carries | Writes | Shipped instances |
|---|---|---|---|---|
| `oauth2` | authorize-redirect → code exchange (deadline-armed) → the provider's `complete::read` arm parses, then the grant is sealed | `code` + `state` | vault handle (+ provider-specific rows, e.g. Slack's `connector.installs`) | `slack`, `zoho` (multi-data-center: the callback's `location` resolves the effective token endpoint), `jira`, `linear` |
| `app_install` | user authorization → discover or verify App installation → `complete::read` (GitHub arm); zero reachable installations REFUSE with `UZ-CONN-008` (no install-page continuation in this tree, see §GitHub App) | `code` + `state`; `installation_id` is optional | vault handle + non-secret connector-install routing row | `github` |

**There is no `api_key` archetype.** One was considered for operator-pasted vendor keys (Datadog, Grafana, Fly) and dropped (M108_002).

A static vendor key is a workspace secret referenced as `${secrets.<name>.<field>}`, not a connector. It never had a connect/callback round-trip or a platform app secret to protect.

Those three providers are plain `agentsfleet secrets create` entries, never registry entries. The `catalogue!` macro declares five variants: five Open Authorization (OAuth) or app-install connectors, not eight.

## Trust anchors

1. **The signed single-use `state` binds workspace intent and starter identity**. It is keyed with the approval signing secret and carries a keyed identity tag instead of a raw subject.

   The state verifies in constant time and is consumed exactly once. The authenticated callbacks endpoint checks the same identity, `connector:write`, and current workspace access before consuming state.

   Forged, expired, replayed, or identity-mismatched state returns 400 `UZ-CONN-002`. State does not prove ownership of a provider installation.

2. **GitHub user authorization proves installation access.** The callback exchanges the one-time `code` with the platform `client_id` and `client_secret`. A claimed `installation_id` is probed with the user token before it is saved, without exposing the token to the browser or provider URL.

   With no claim, exactly one accessible installation repairs internal state loss. Zero returns 403 `UZ-CONN-008` (install the App, then connect again), and more than one returns 403 `UZ-CONN-008` too. The datastore also refuses to move an installation already bound to another workspace.

3. **Platform app secrets live in the admin-workspace vault** as per-provider `<provider>-app` bags (`slack-app`, `github-app`, …). One app per provider is shared across all tenants, catastrophic if leaked, never on a per-tenant surface.

   GitHub's bag carries its App identity, user-authorization client credentials and App-level webhook secret. A missing bag fails loud with 503 `UZ-CONN-001`.

4. **Provider signatures authenticate inbound events.** GitHub App traffic is verified against the platform `github-app.webhook_secret`. A fleet's webhook trigger is verified against the `webhook_secret` in the workspace secret its `secret_name` names. Slack App events use the platform `slack-app.signing_secret`, and no inbound route falls back to Bearer authentication.

5. **A signature proves origin, never the producer**: GitHub permits every push-capable identity to create deployment statuses. Every such identity in a mapped repository sits inside the trusted producer boundary. The daemon verifies the App signature and the installation routing; it does not inspect `deployment_status.creator` or attest which system produced the status.

   A scenario that depends on this says so and cites this anchor rather than restating it.

6. **The standing integration grant authorises every mint, including a repository write**. `connector.integration_grants` holds one row per `(fleet_id, service)`, enforced by `UNIQUE (fleet_id, service)`, and only an `approved`, unrevoked row admits a mint. The fleet asks for it with `request_access`, which writes the row `pending` and raises one approval card.

   A person's Approve on that card is the answer ([where a grant comes from](#where-a-grant-comes-from)). The grant's approved `scope` names the repositories and read or write.

   A repository write raises no per-event card. `agentsfleet fleet grants revoke` is the manual stop, taking effect at the next run and the next token mint. The payer's balance bounds the money.

The token is narrowed to the grant's repositories by `ScopedRequest::for_binding`, and `Granted::verify` checks the response. The grant is the authorisation above that scoping, never a replacement for it.

The connector registry owns callback dispatch; provider ingress handlers own event routing once the route segment has selected them. Detailed auth behavior and refusal codes live in [`../AUTH.md`](../AUTH.md) §"OAuth connectors".

## GitHub App: platform setup to fleet runs

One GitHub App serves every tenant in an environment. The platform operator configures two different URLs on that App:

```
browser install callback                    machine event ingress
/api/connectors/github/callback             /v1/ingress/github
          │                                      │
          │ relays authenticated completion       │ wakes subscribed fleets
          ▼                                      ▼
identity-bound signed state               GitHub App signature
```

The platform identity lives only in the `agentsfleet-admin` workspace, as the `github-app` bag:

| Field | Role |
|---|---|
| `app_id` | public App identifier |
| `app_slug` | public install-page handle (unread by `agentsfleetd`) |
| `client_id` | public user-authorization client identifier |
| `client_secret` | exchanges one-time user authorization codes |
| `private_key_pem` | signs App JSON Web Tokens for outbound token minting |
| `webhook_secret` | verifies inbound App deliveries |

A workspace administrator selects **Connect**. GitHub authorizes the user first. If the App is already installed and exactly one installation is accessible, `agentsfleet` restores the missing internal binding.

If none exists, the connect REFUSES with `UZ-CONN-008`: the App must be installed on GitHub first. The daemon cannot send the browser there because it reads no `app_slug`.

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

A callback completion writes the vault handle and reverse-routing rows in one transaction.
**Disconnect** deletes both in one transaction too.
Concurrent operations remain unordered; see §"The registry: a provider is a data entry" for the partial-binding risk and recovery.

A handle a model entry still names is not deleted: Disconnect answers 409 `UZ-VAULT-004` with `current_state: "referenced"`. Disconnect leaves the GitHub App and repository access installed. A later **Connect** can therefore reconcile external and internal state after a datastore rebuild.

The user token is discarded after the current callback, always. The daemon carries no App slug, so there is no App-install continuation to hold it for. Zero reachable installations is a refusal (`UZ-CONN-008`), not a redirect to GitHub's install page, as M187_001's Discovery records.

After the identity, workspace, and installation checks pass, the callbacks endpoint writes both records on one database connection:

```
workspace vault                          connector.installs
github = {                               provider = github
  integration: github,                   external_account_id = installation_id
  installation_id                       workspace_id = connected workspace
}                                        credentials = NONE
```

The encrypted handle supports outbound token minting. The connector-install row is deliberately non-secret and supports inbound `installation.id → workspace` routing. Neither row alone is sufficient.

A callback failure rolls back its changes, preserving the preceding state.

### Repository and event subscriptions belong to fleets

The App installation chooses the maximum repository set GitHub will permit. Each fleet then adds one `app_event` trigger row naming the smaller set it wants to receive ([fleet triggers](#fleet-triggers)):

```text
trigger_add {kind: "app_event", source: "github", events: ["pull_request"], repositories: ["acme/payments"]}
```

`repositories` is fail-closed: a row that names none receives no App delivery. This keeps an App installed across an organisation from waking every GitHub fleet for every repository.

The repository match is case-insensitive, as GitHub's is. `events` answers the other way from `repositories`: a row with no `events` list admits every event (`afd_ingress`'s binding module).

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
        │ app_event rows in W: repository + event match
        ├── fleet A subscribes to acme/payments + pull_request  → receives it
        ├── fleet B subscribes to acme/payments + pull_request  → receives it
        ├── fleet C subscribes to acme/catalog + pull_request   → does not receive it
        └── fleet D has no app_event row                        → does not receive it
```

A person acts once: they connect the workspace and authorize the App installation. The fleet adds its own `app_event` row by tool. From then on, event routing is machine-to-machine and reads saved rows rather than anyone's browser session.

This gives each layer one job:

| Layer | Owns |
|---|---|
| GitHub App installation | Maximum repositories GitHub permits the App to see |
| Workspace connection | Installation-to-workspace identity and the outbound credential handle |
| Fleet trigger (`fleet.triggers`, kind `app_event`) | Explicit repository and event subscription |
| Integration grant | Whether that fleet may call GitHub with a minted token; receiving the event needs none |
| Turn row (`thread.turns`) | Exactly-once enqueue per matching fleet for one GitHub delivery |

### How a delivery is routed

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
app_event rows (fleet.triggers) in that workspace, on active fleets
  ∩ source=github
  ∩ repositories contains repository.full_name
  ∩ events admits the incoming event
        │
        ▼
commit queued thread.turns row (body digest + fleet)
        │
        ├── best-effort wake notification
        ▼
assignment pass → thread row fence → run → Down::Run on the runner connection
```

Multiple fleets may intentionally subscribe to the same repository and event. Replay protection is therefore per authenticated payload body and fleet, not global. The signature-covered body digest is the replay identity; the unsigned delivery header is diagnostic only.

If one fan-out leg fails before its turn commits, redelivery retries that leg. Fleets already holding the turn answer `replayed`; redelivery writes turns only for the missing fleets. Every matching fleet is woken (`afd_ingress`'s App routing).

**That redelivery is not GitHub's.** GitHub states plainly that it "does not automatically redeliver failed webhook deliveries".

A delivery fails when the receiver is down or takes longer than **ten seconds** to answer. Recovering it is a manual click in the App's delivery log or an operator script walking the REST API for failed deliveries.

This page previously credited the recovery to "GitHub's retry", which does not exist. The correction moves the boundary of what is recoverable:

| Where the fan-out fails | What survives |
|---|---|
| After a leg's turn row commits | Durable. The committed queued turn stays available to the next run assignment; wake notifications are best effort. |
| Before it commits (the failing leg, and every leg after it in the loop) | **Nothing.** No row, no entry, and no sender that will ask again. |

So the durable-acceptance guarantee covers work this deployment ACCEPTED. The window before acceptance is the one place a GitHub event can be lost outright. Two consequences follow.

The ten-second budget is a hard deadline rather than a target. It is shared across signature verification, fleet resolution, and one turn write per subscribed fleet, so fan-out width spends it.

Deduplication is still worth every line. The redelivery it absorbs is a *human* clicking Redeliver with no idea whether the first attempt landed. That is exactly when a duplicate review would otherwise appear on a pull request.

No delivery wakes a durable task's wait today ([event delivery](durable_tasks.md#event-delivery)).
`deployment_status` and other repair-branch deliveries are classified unsupported and dropped (`afd_api_ingress`'s App route).
Completion evidence therefore never fans out an unrelated investigation.

Slack is the contrast and the reason this page cannot generalise: its retry semantics are load-bearing precisely because Slack does retry.

### Where a grant comes from

A fleet asks for a credential when it needs one. Every run is offered the supervisor tool `request_access {name, scope?}`, which posts `POST /v1/runners/me/runs/{run_id}/access`. A workspace secret, GitHub and every other connected integration pass through the same card.

```text
run calls request_access {name: "grafana"}
        │
        ▼
daemon checks: a vault secret or a connected integration? a GitHub scope? a readable host?
        │                       │
        │ refused               └─ already covered by the approved grant ─► answers at once, no card
        ▼
[kind] sentence back to the model; the run continues; no row, no card
        │
        ▼ (passes, not covered)
grant pending + requested_scope ─► approval card carrying the asking turn
        │                          the run ends waiting, as ask_user does
        ▼
a signed-in person clicks Approve or Deny
        │
        ▼
resolve_gate.sql flips the grant and writes the answer turn ─► the run resumes
```

The daemon checks a request before it writes anything:

- The name is a workspace vault secret or a connected integration. The grant's `kind` comes from the vault entry, as `afd_credential`'s secrets module routes it.
- A `github` request names a scope: the repositories, and read or write. The repository parser in `af_fleet_runtime` reads it.
- A secret's `host` must be one `af_egress::allowlist_host` can read, because the secret is sent only to that host.

A refused request returns `[kind] sentence` to the model, and the run continues with no row and no card. A request the approved grant already covers answers at once, with no card. A second request while one is pending answers 200 and raises no second card.

Otherwise the daemon sets the grant `pending`, keeps the asked-for scope in `requested_scope` and raises the approval card through `request_grant.sql`. The card carries the asking turn. The tool ends the run waiting, as `ask_user` does, and the turn settles waiting, so the fleet's later turns are not held behind it.

Only a signed-in person answers the card; an API key gets 403. Approve and Deny go through `resolve_gate.sql`, which flips the grant and writes the same answer turn a question's answer writes. Its message reads `approved` or `denied`, and that turn resumes the waiting run, holding the credential or knowing it was refused.

Approve moves `requested_scope` into `scope`. Until then the approved scope stays in force, so a token mint wider than it is refused. Deny sets the grant `revoked`, and a later request for the same name sets it `pending` again with a new card.

`agentsfleet fleet grants revoke`, or `DELETE` on the fleet's `integration-grants/{id}`, revokes a grant. A live run keeps what it was issued. The next run and the next token mint do not.

A run's policy reads only approved grants ([egress model](runner_fleet.md#egress-model-outbound-is-the-only-network-surface)). Their names fill the `secrets_map`, an approved GitHub scope builds the repository binding, and each approved secret's host joins the network `allow` list. Slack's `addressed_only` reads the GitHub scope's write reach.

**A card is a per-turn question; a grant is the standing answer that outlives the run.** The card asks, and the grant remembers. Each request, approval, denial and revoke emits the product event `fleet_access_changed` ([product analytics](product_analytics.md)).

The card is the approval inbox this codebase already ships: an inbox, a detail page, resolve buttons, an approval webhook and an append-only audit. Nothing expires a card. A read reports `timed_out` once its deadline passes, and the card stays answerable (`afd_approval`'s inbox).

### Credential use remains separate from event receipt

Receiving a signed event does not give GitHub credentials to a fleet.
When the fleet calls GitHub through `${secrets.github.token}`, its runner-token plane asks the daemon to mint a token.

The daemon derives the fleet and workspace from the run and reads the fleet's current grant.
It loads the workspace installation handle, signs with the platform key and exchanges for a short-lived token.

The runner caches that token for the run and refreshes it 30 seconds before expiry.
It masks the token in tool responses (`af_egress`'s vault module).
The App private key and webhook secret never enter the run, runner environment, sandbox, logs or response frames.

The repository binding, built from the grant's approved scope, determines token scope (`afd_credential`'s GitHub request builder).
Read bindings receive contents read without Pull Request permissions; write bindings add bounded write permissions, never workflows.
Actions/checks reads require installation permission; a missing permission returns 403 to the fleet.

The shared HTTP client disables automatic redirects, but the tool supports guarded bodyless GET redirects.
Each destination needs a current egress policy check; cross-origin modeled headers are stripped.
See [guarded downloads](runner_fleet.md#guarded-tool-downloads); implementation support alone does not prove a complete fleet-owned log read.

### Provider impact

| Provider | Connect credential | Inbound events after M102_005 |
|---|---|---|
| <img src="https://cdn.simpleicons.org/github" width="14" alt="" /> GitHub | App installation handle | App ingress routes by installation, then by each `app_event` row's repositories and events |
| <img src="https://api.iconify.design/logos/slack-icon.svg" width="14" alt="" /> Slack | bot token from OAuth | specialized events route: signature, challenge, and mention routing to one `slack_mention` event (`afd_api_ingress`'s mention handler) |
| <img src="https://cdn.simpleicons.org/zoho" width="14" alt="" /> Zoho Desk | OAuth refresh handle, multi-data-center token endpoint | no inbound integration in this workstream |
| <img src="https://cdn.simpleicons.org/jira" width="14" alt="" /> Jira | OAuth refresh handle | no inbound integration in this workstream |
| <img src="https://cdn.simpleicons.org/linear" width="14" alt="" /> Linear | OAuth refresh handle | no inbound integration in this workstream |

## Fleet triggers

A trigger is a `fleet.triggers` row that lets something outside wake a fleet. The fleet adds its own rows by tool, and a person sees and removes them. Schedules are separate `fleet.schedules` rows the fleet adds with `cron_add` ([QStash owns the clock](data_flow.md#qstash-owns-the-clock)).

| Kind | `source` | Fields | One per fleet | The turn it writes |
|---|---|---|---|---|
| `webhook` | the sender's lowercase slug, never `approval` or `github` | `events`, `verifier_scheme`, `verifier_header`, `secret_name` | per source | actor `webhook:<source>`, from `POST /v1/webhooks/{fleet_id}/{source}` |
| `app_event` | `github` | `repositories`, `events` | one GitHub App subscription | actor `github-app` |
| `mention` | `slack` | `channel`, a Slack channel identifier rather than its name | one Slack channel | producer `slack_mention`, actor `slack:<user>` |

The row is unique on `(fleet_id, kind, source)` and is deleted with its fleet. Indexes on `(workspace_id, kind, source)` and `(workspace_id, kind, channel)` serve the ingress reads ([Postgres schemas](postgres_schemas.md)).

### Adding, listing and removing

A run adds, lists and removes rows with `trigger_add`, `trigger_list` and `trigger_remove` (`afr_tools`). Each posts a fenced run verb, `POST`, `GET` or `DELETE /v1/runners/me/runs/{run_id}/triggers`, carrying `item_id`, `call_number` and `fencing_token` as the schedule verb does. Each tool's description tells the model to call it only when the person's newest message asks.

An add is refused, and the model reads `[kind] sentence` and may correct the call in the same run, when:

- the workspace has not connected the provider, for an `app_event` or `mention` row;
- `secret_name` names no secret in the workspace vault;
- the row fails the trigger checks in `af_fleet_runtime`'s `config/trigger` module;
- a webhook row's source is `approval`, the approval route's segment, or `github`, which is an `app_event` row;
- the fleet already holds a row of that kind and source.

No count limit applies beyond what the workspace has connected. A person lists a fleet's rows with `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/triggers` (`fleet:read`) and removes one with `DELETE` on `triggers/{id}` (`fleet:write`). The CLI's `agentsfleet fleet triggers list|delete` and the fleet page's "Wakes on" list call the same routes ([web app](web_app.md)).

Each add or remove writes the operator log `trigger_changed` with `fleet_id`, `kind` and `by` (`run` or `person`). It carries no signing secret and no URL ([observability](observability.md)).

### Routing a delivery to its rows

The three ingress statements, for webhooks, App events and mentions, read `fleet.triggers`. Each keeps its `workspace_id` predicate, so a delivery never reaches another workspace's fleet. An App delivery for a repository no row in the install's workspace names writes no turn.

A delivery needs no grant on the fleet; a grant governs only what the fleet may call ([where a grant comes from](#where-a-grant-comes-from)). A signed delivery to a removed row's URL, or to a source no row ever named, answers 404 and writes no turn.

### Webhook signing recipes

The registry sources `linear` and `slack` imply their own signing recipe and refuse a `verifier`. Any other source's `verifier` names its scheme, stored as `verifier_scheme`: `body-hex-sha256`, `body-hex`, `body-base64-sha256`, `slack-v0` or `svix-v1`.

The three body recipes need a header, stored as `verifier_header`: an HTTP token of 1 to 128 characters. `slack-v0` and `svix-v1` read fixed headers and refuse one (`af_fleet_runtime`'s `config/trigger` verifier).

`secret_name` names the workspace secret that signs the deliveries. That secret is a JSON object with a `webhook_secret` string, which holds the `whsec_` value for Svix. One fleet can hold rows on registry and declared recipes side by side.

## Slack mentions

A Slack mention reaches one fleet that holds a `mention` row for the channel.
The daemon posts that fleet's answer in the thread the mention came from.
The [`chatty-parrot` fixture](../../tests/fixtures/fleets/chatty-parrot.json) is the prime set's fleet for this path.

```text
Person -> Slack thread: @agentsfleet-dev why did this fail?
Slack -> agentsfleetd: signed app_mention
agentsfleetd -> connector install: resolve team and current workspace
agentsfleetd -> the channel's mention trigger rows: choose the subscribed fleet
    no subscriber -> channel_not_configured; no fleet or model call
    ambiguous / archived / needs name -> fixed notice through existing outbound ledger
    addressed or sole eligible fleet -> one slack_mention turn
Subscribed fleet -> its approved grants: read what the request needs
Fleet report -> recorded Slack thread: answer through existing outbound poster
```

A channel's fleets are long-lived subscribers.
A mention never creates a fleet, because a fleet created per mention would start with empty memory ([memory](memory.md)).
Each mention gets one run.

Slack's bot token and the GitHub App act as service accounts, with no per-user linking.
No fleet holds the Slack bot token; the daemon posts.

### Connector, notice owner and subscription

| | Slack connector | Channel notice owner | Channel subscription |
|---|---|---|---|
| What it is | Workspace Slack credential and team routing. | Stable owner for fixed routing notices only. | A fleet's trigger row naming the channel. |
| Where it lives | Vault handle and `connector.installs`. | Existing `connector.channels` binding; no new schema. | A `fleet.triggers` row: kind `mention`, source `slack`, one channel identifier. |
| Created by | Workspace administrator connects Slack. | First eligible notice binds an existing subscriber when no valid owner exists. | The fleet, with `trigger_add`, when a person asks; a person removes it from **Wakes on**. |
| Proves | The daemon can use the workspace's connector. | Notice ownership only; never model or turn authority. | This fleet is a candidate for mention routing. |

A connected card proves only the first column.
The catalogue's "Connected" is vault-key membership, and the per-provider status reads a JSON handle with an `integration` field (`afd_connector/src/grant/holding.rs`).
Neither checks that a token is alive, that an event subscription exists or that a channel is bound.

### GitHub App and Slack mention compared

| Question | GitHub App | Slack mention |
|---|---|---|
| Ceiling | the repositories the App installation covers | the channels the bot user is a member of |
| Workspace routing | `installation.id` → `connector.installs` (`SELECT_INSTALL_WORKSPACE`, `afd_ingress/src/sql/mod.rs`) | `team_id` → the same table and statement |
| Subscription | `app_event` trigger row: source `github`, `repositories`, `events` | `mention` trigger row: source `slack`, one `channel` |
| Subscription identity | repository full name, compared case-insensitively (`serves_repository`, `afd_ingress/src/binding/mod.rs`); a rename stops routing | channel identifier (ID); a rename changes nothing |
| Who receives it | every matching fleet (fan-out) | one configured fleet, a fixed notice, or an unconfigured drop |
| Replay key | producer `webhook_app`, `<fleet>:<body digest>` (`afd_ingress/src/deliver.rs`) | producer `slack_mention`, `<team_id>:<event_id>` (`afd_ingress/src/slack/admit.rs`) |
| Credential | the fleet mints an installation token behind an approved `github` grant | the fleet holds no Slack credential; the daemon posts |

The GitHub subscription keys on a repository's name, so renaming a repository silently drops its events (`UZ-WH-022`).
That weakness is recorded here and is not fixed.

### Routing a mention

The handler admits only an `Addressed` or `Sole` subscriber.
An unconfigured channel returns `channel_not_configured` without creating a fleet, scheduling a run or calling a model.
The route does not use historical channel fleets as fallback responders.

1. Validate the signed event and resolve its Slack team to the current workspace.
2. Drop bot messages, unsupported event types and replayed deliveries through the existing ingress rules.
3. Read the channel's `mention` trigger rows, only in the workspace the install belongs to.
4. Match a leading fleet name against each subscriber's name, or choose the sole eligible unaddressed subscriber.
5. Admit the selected fleet or record the fixed routing notice described below.

| Subscription state | Native outcome |
|---|---|
| No subscribers | `Unconfigured`; return `channel_not_configured`, without a model or fleet creation. |
| One matching name, runnable | `Addressed`; admit that fleet. |
| Matching name archived | `Archived` notice from the channel's carrier, its oldest active fleet; no model call. With no active fleet in the channel, the notice is dropped as `no_active_notice_carrier`. |
| Several names match ignoring case | `Ambiguous` notice; no arbitrary selection. |
| No matching name, one eligible unaddressed subscriber | `Sole`; admit that configured fleet. |
| No matching name, no eligible unaddressed subscriber | `AddressIt` notice naming configured candidates. |
| No matching name, several eligible unaddressed subscribers | `Choose` notice; ask which fleet should receive the request. |

A fleet whose approved GitHub scope can write is addressed-only: it needs its name even when it is the channel's only subscriber.
The [native route](../../rustd/crates/daemon/afd_ingress/src/slack/route.rs) owns eligibility and name matching.
The [mention handler](../../rustd/crates/daemon/afd_api_ingress/src/handler/mention/mod.rs) owns turn intake and the unconfigured outcome.

Fixed ambiguity, archived and addressing notices use the existing outbound ledger and Slack poster.
The existing `connector.channels` row supplies a stable notice-only owner.
If no valid binding exists, notice handling binds an existing subscribed candidate and creates no fleet.

The binding must match the current workspace and installed team.
Deleted owners and moved-team bindings cannot supply stale authority.
Concurrent notices converge on the channel's existing unique binding.

An archived fleet is never chosen as a carrier, because it cannot acquire another outbound delivery.
When no active fleet can carry a notice, the notice is dropped as `no_active_notice_carrier`.
That includes a bound carrier that was later archived.

[Notice ownership](../../rustd/crates/daemon/afd_api_ingress/src/handler/mention/notice.rs) owns those checks and the recorded thread destination.
Slack retries reuse the existing deterministic notice identity.
A channel binding carries notices only when current ownership checks permit it, and the route never runs a bound fleet as a fallback.

A subscribed fleet uses its memory tools across threads, and its pinned memory is in every run's prompt.
The thread remains a reply destination; [memory](memory.md#4-slack-threads-and-fleet-memory) owns the memory scope.
Connecting Slack or saving a notice binding proves no memory capture or recall.

### What the fleet is told

Slack's mention event carries the mention and nothing else.
The daemon re-reads the thread before admitting the event, with `conversations.replies` for the thread root (`afd_connector/src/slack/replies/mod.rs`).
It reads every message in thread order, each told whole, under a 1.5 second deadline.

The daemon loads the bot token and releases the pool connection before that call.
Attachment and block text are flattened into the message, so a link in the thread's parent survives.

The runner reads the request's `message` string and falls back to the whole body (`af_wire/src/turn/message.rs`).
The daemon therefore composes one `message`: the mention text, then the thread under a fixed heading that labels it untrusted data.
A failed or forbidden re-read degrades to the mention alone, and the message says so.

A private channel needs `groups:history`, which the app does not request.

### How the answer comes back

The producer that owns a reply surface records the reply destination on the turn.
The provider is `slack`, and the address is `{team_id, channel_id, thread_ts}`; the poster requires `channel_id` and `thread_ts`.
The `thread_ts` is the mention's `thread_ts`, or its `ts` when the mention started the thread.

The destination travels with the turn.
An answer to a question, an access card's included, inherits the destination from the turn it resumes.
The report transaction owes a delivery only when the turn carries a destination, and the Slack poster reads the address from the delivery row.

The report parses the recorded connector once, so the run's model provider never reaches the delivery ledger (`afd_fleet/src/run/delivery.rs`).
A job its destination refuses for good, or one out of delivery cycles, is stamped abandoned.
No scan offers an abandoned job again (`afd_outbound/src/abandon.rs`).

A run may say interim lines before its answer, through `POST /v1/runners/me/runs/{run_id}/messages` (`af_wire/src/message_verb/mod.rs`).
No count or length cap applies.
The channel's own limit refuses what it cannot take, and the run reads that line as undelivered.

The run plane fences each line on the run's standing, read without a lock, and writes nothing to the run (`afd_fleet/src/run/message/mod.rs`).
It names each line by the call's number in its run.
A retried call is therefore the same line, and a reclaimed run numbers its calls afresh.

The daemon posts every Slack text, interim line and answer alike, as literal text.
It sends `&`, `<` and `>` as Slack entities (`afd_outbound/src/slack/escape/mod.rs`).
Each interim line is stamped `agentsfleet_interim` with `{run_id, line}`, so a repeat finds only its own line and never silences the answer (`afd_connector/src/slack/answered/mod.rs`).

### Write reach from a mention

The incident drill in the [drill playbook](../../playbooks/operations/slack_incident_drill/001_playbook.md) uses one read fleet and one repair fleet.
Each stage has its own authority.

| Stage | Who acts | Credential | Approval |
|---|---|---|---|
| Diagnosis | the responder fleet, on a mention | GitHub token with `contents`, `actions` and `checks` read; Grafana Viewer token | one Approve on each access card it raised; its GitHub scope reads and its Grafana token is a Viewer's, so it cannot write |
| Draft fix pull request | the repairer fleet, addressed by name | GitHub token for one repository, writing one daemon-issued branch and one draft pull request against the approved base (`afd_gate/src/policy/egress/write.rs`) | one Approve, when the repairer asked for `github` write; none per request |
| Merge | a person in GitHub | their own | the repository's branch protection |
| Deploy | the repository's pipeline | the pipeline's | the release process; the drill's audit checks that `agentsfleet-repair/*` branches cannot reach deploy secrets |

A Slack-requested run stays within its fleet's approved grants.
The repairer's turn-owned ref permits bounded corrections through `PATCH` with `force:false`.
Other ref updates, merge, deletion and GraphQL remain refused.

The runner rejects unmatched requests on ruled hosts (`allows_origin` in `af_egress/src/policy/mod.rs`), and its token has no workflow permission.
Write-bound fleets never receive unaddressed mentions.
A branch push runs repository workflows, so the drill audits secret paths for `agentsfleet-repair/*` branches.

A scheduled diagnosis needs a configured notification destination and explicit repair authority.
The fleet receives no implicit cross-fleet handoff or arbitrary channel routing.
An explicit handoff is `fleet_steer` ([fleet to fleet steering](user_flow.md#831-reaching-a-fleet-by-name-and-fleet-to-fleet-steering)).

### Evidence a fleet reads

| Source | How a fleet reads it | Known gaps |
|---|---|---|
| GitHub run | the fleet reads the linked run, failed jobs, steps and annotations through `http_request` under the repository's read prefix, as its pinned guidance and the request direct; it also reads commits and compare there (`afd_gate/src/policy/egress/read.rs`) | A mint reads the installation's permissions first and asks for `contents` read plus whichever of `actions` and `checks` read it holds (`for_binding`, `afd_credential/src/credential/github/request.rs`). The App registration lists Checks: read-only and each installation accepts it before the drill (`playbooks/operations/github_app_registration/001_playbook.md`); until one does, the mint succeeds without it and the fleet's annotation read answers 403. Job logs can redirect to storage. The tool follows at most three guarded bodyless GET redirects, readmitting each destination and stripping modeled headers on an origin change. The fleet's network toggles must admit the storage host, and a private fleet reaches only GitHub and its approved secrets' hosts ([egress model](runner_fleet.md#egress-model-outbound-is-the-only-network-surface)). A denied or truncated required log still produces a named source gap. See [guarded downloads](runner_fleet.md#guarded-tool-downloads). |
| Grafana | workspace secret `grafana = {host, token}`, approved once when the fleet asks with `request_access`; `Bearer ${secrets.grafana.token}`; Loki and alert reads over GET | No connector, by design ([connector archetypes](#archetypes)). The host must resolve publicly, because the tool rejects private addresses. No network setting limits Grafana to reads; the Viewer token's role does. |
| Memory | the fleet's own namespace, loaded only under a live run for that fleet that no reclaim has moved past (`Plane::load` in `afd_fleet/src/run/memory.rs`) | none for this path |

### Integration map

| Integration | Connector (auth) | Events in | Actions out |
|---|---|---|---|
| GitHub | App install; per-run installation token, repository-scoped, 1 hour | App ingress: `pull_request`; `workflow_run` completed with `failure`, `timed_out` or `startup_failure`; `deployment_status` dropped (`afd_api_ingress/src/handler/webhook/github/mod.rs`) | fleet `http_request` inside egress rules |
| Slack | OAuth bot token: `app_mentions:read`, `chat:write`, `channels:history` | `app_mention` | daemon `chat.postMessage` |
| Grafana | none; workspace secret | none | fleet reads |
| Zoho Desk | OAuth refresh, multi-data-center, `Desk.*.READ` | none | none |
| Zoho Recruit fleet | static secret `zoho_recruit`, never refreshed | none | fleet `http_request` |
| Zoho Sprints fleet | rides the Desk grant, whose Desk scopes cannot read Sprints | none | fleet `http_request` |
| Linear | OAuth refresh: `read`, `comments:create` | no connector ingress; a per-fleet signed webhook exists | none from the daemon |
| Jira | OAuth refresh plus site lookup | none | none |

### What is built and what is pending

| Capability | Rust daemon today | Still required |
|---|---|---|
| Signed Slack delivery | signature, 5-minute window, `url_verification` echo (`afd_api_ingress/src/handler/events/mod.rs`, `afd_connector/src/registry/mod.rs`) | nothing |
| Mention becomes a turn | one `slack_mention` turn per Slack event, keyed `<team_id>:<event_id>` (`afd_ingress/src/slack/admit.rs`, `afd_turns/src/incoming.rs`) | nothing |
| Channel → fleet | `mention` trigger rows route only Addressed or Sole subscribers. Unconfigured channels return `channel_not_configured`; existing bindings carry fixed notices only. | The combined live drill. |
| Thread context | `conversations.replies` under 1.5 s, parent plus replies, under a fixed untrusted-data heading (`afd_connector/src/slack/replies/mod.rs`, `afd_ingress/src/slack/message.rs`) | nothing |
| Answer delivery | owed only to a recorded destination; a failed run with no text, a refused turn, or a turn ended after its runs lapsed owes a fixed notice (`afd_fleet/src/run/sql/commit_report.sql`, `afd_fleet/src/run/turn.rs`); abandoned when refused or out of cycles | nothing |
| Slack poster | `chat.postMessage` from the job's own address; reads no turn row (`afd_outbound/src/slack/mod.rs`) | nothing |
| Write reach from Slack | one ref and one draft pull request, proven for a Slack-requested run with no per-request approval (`afd_gate/src/policy/egress/tests/slack.rs`) | nothing |
| Continuous Integration (CI) evidence reach | the read mint asks for `actions` and `checks` read where the installation holds them, and the read rule covers the CI paths; guarded GET log redirects exist, and live access depends on the fleet's network toggles | Every installation accepts the required App permissions, and the fleet's network admits the log destinations. |
| External steps | not in code | The playbook: apps, channels, App permissions and the Grafana secret. Each fleet adds its own subscription and asks for its own access. |

Nothing here is proven live.
The Slack registration playbook's live check, a threaded reply to a mention, has not been recorded against the Rust daemon (`playbooks/operations/slack_app_registration/001_playbook.md`).
The [drill playbook](../../playbooks/operations/slack_incident_drill/001_playbook.md) records the development and production drills.

### Drill channels and environments

| Channel | Slack app | Deployment | Bound fleets |
|---|---|---|---|
| `#ci-dev` | `agentsfleet-dev` | `api-dev.agentsfleet.net` | `ci-dev-responder` (read), `ci-dev-repairer` (write, addressed only) |
| `#ci-prod` | `agentsfleet` | `api.agentsfleet.net` | `ci-prod-responder`, `ci-prod-repairer` |
| `#release-dev`, `#release-prod` | not invited | none | none; rollout verdicts and release discussion stay human |

Development runs first.
The production app and `#ci-prod` follow only after the development drill passes, the order the Slack registration playbook sets.
Both channels subscribe to `agentsfleet/linkwarden`: development selects pull requests targeting `dev`, and production selects `main` pushes.

The repairer in either workspace opens a draft against Linkwarden `dev`.

### Slack mention invariants

- One Slack event writes at most one turn or one notice, across retries.
- A fleet answers in a channel only when it holds a `mention` trigger row for that channel's ID.
- A delivery is owed only to a destination its producer recorded; a model provider never reaches the delivery ledger.
- A delivery the destination permanently refuses is abandoned and never re-offered.
- Memory is private by default. A mention grants no shared-memory access; explicit read grants can expose workspace-visible memory from other fleets ([memory](memory.md)).
- A Slack-requested write run reaches one daemon-issued branch and one draft pull request, and never merges or deploys.
- A write-bound fleet never receives an unaddressed mention.
- A fleet reads CI evidence only inside its bound repository.
- A fleet cites a job log line only if its own `http_request` returned that text; a redirect it cannot follow becomes a named gap.

## Bounded outbound: every vendor call has a deadline

Every connector vendor call runs under a deadline set at its call site. A callback exchange past its deadline, or one that cannot reach the vendor, answers `UZ-CONN-003` (502); background paths log the same code and retry.

- **Deadlines are named per call class.** Token exchange and credential mint: 10 s (`EXCHANGE_TIMEOUT` in the daemon binary's credentials module). That is a client-wide total timeout covering connect and the Transport Layer Security (TLS) handshake. Slack post: 5 s (`POST_DEADLINE`, `afd_outbound`); Slack thread re-read: 1.5 s (`READ_DEADLINE`, `afd_connector`); Slack answer check: 3 s (`ANSWER_CHECK_DEADLINE`, `afd_connector`).

- **The exchange and the mint hold separate clients.** A slow connect exchange cannot consume the connection slots a credential mint needs.

- **No pool slot rides a vendor call.** Credentials load under a short acquire released before the exchange. The events ingress pre-loads the bot token and returns its slot before the thread re-read. This closed a priority-1 review finding on merged Pull Request #468.

## Unknown vs unconfigured (the two front-door failures)

An unknown provider (`UZ-CONN-004`) and a registry id with no `<provider>-app` bag (`UZ-CONN-001`) are different failures ([error codes](https://docs.agentsfleet.net/api-reference/error-codes#UZ-CONN-004)).

## Adding a provider (the recipe)

1. One `Provider` variant in the `catalogue!` macro, with its id and display name (RULE UFS). The id is at once the route segment, the vault-key stem and the registry id. The vault-key stem is `<provider>-app` for the platform app and the bare `<provider>` for the workspace grant (`Provider::app_key` and `Provider::grant_key`).
2. One arm in `Provider::archetype()`, either `Oauth2Flow` (endpoints, scopes, delimiter, extra query, refresh) or `AppInstall` (authorize + token endpoints), plus arms in `state_binding()` and `event_ingress()`.
3. The provider's arm in `complete::read`, which is where its answer is parsed into a grant. The registry holds no per-provider hook functions: it dispatches on the archetype enum and matches per provider.
4. Create the `<provider>-app` bag in the admin vault. An operator-supplied vendor key with no browser round-trip (Datadog/Grafana/Fly's shape) is not a connector at all. It is a plain workspace secret, `agentsfleet secrets create`, never a registry entry.
5. Tests: the generic-route suites already cover the flow; add hook-level tests for the provider's parse/save deltas.

No route, matcher, scope, invoke, or OpenAPI edit: the `{provider}` form already covers the new id.
