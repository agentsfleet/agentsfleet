# Connectors — the registry-driven connector platform

> Parent: [`README.md`](./README.md) · Sibling: [`../AUTH.md`](../AUTH.md) §OAuth connectors (flow behavior, trust-anchor mechanics, error taxonomy of the shipped providers). · User-facing setup: [docs.agentsfleet.net/fleets/connectors](https://docs.agentsfleet.net/fleets/connectors).
>
> Scope: the platform shape — the comptime registry + archetype dispatch that makes a new provider a data entry, the callback and event-ingress trust anchors, the bounded-outbound rule for vendor calls, and the connector-vs-integration terminology. Read this before adding a provider or writing any connector outbound call. Flow behavior stays in AUTH.md; this doc owns the invariants that make the flows generic.

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Vocabulary | connector ≠ integration | connector = auth + credential plumbing; integration = a capability built ON the credential | §Terminology |
| Registry | comptime `[]const ConnectorSpec`, `REGISTRY.len` pinned at 5 | adding a provider is one entry + a small hook file; never new route or flow code | §The registry |
| Dispatch | on archetype SHAPE, never provider id | exhaustive switch on the tagged union; registry invariants are `@compileError`s | §The registry |
| Archetypes | 2 — `oauth2` · `app_install` | `slack` / `zoho` (multi-DC) / `jira` / `linear` · `github`; the `api_key` archetype was considered and dropped (M108_002) | §Archetypes |
| Trust anchors | 4 | signed single-use `state` (`UZ-CONN-002`) · user-authorization installation proof (`UZ-CONN-008`) · admin-vault `<provider>-app` bags (`UZ-CONN-001`) · provider signatures; no Bearer fallback inbound | §Trust anchors |
| GitHub App URLs | 2, different jobs | `/v1/connectors/github/callback` (browser install) vs `/v1/ingress/github` (machine events) | §GitHub App |
| App replay identity | authenticated body digest, per fleet | the unsigned delivery header is diagnostic only; failed fan-out legs retry without duplicating others | §GitHub App |
| Outbound HTTP | `bounded_fetch.zig` only, grep-gated | pin → arm → fetch → disarm; refusal is `UZ-CONN-003` (502); deadlines named per call class (10 s / 10 s / 1.5 s) | §Bounded outbound |
| Residual unbounded window | the TLS handshake | `std.http.Client.connect` does TCP+TLS atomically; tracked follow-up | §Bounded outbound |
| Front-door failures | 404 vs 503 | unknown provider → `UZ-CONN-004`; registry id with no `<provider>-app` bag → `UZ-CONN-001`, fail-loud | §Unknown vs unconfigured |

## Traps

Each trap is enforced in its owner section; this list is the index.

- "Slack integration is broken" and "Slack connector is broken" name different layers — keep the vocabulary straight (§Terminology).
- No `if provider == "slack"` exists anywhere in the flow; adding one is a design regression (§The registry).
- A static vendor key (Datadog, Grafana, Fly) is a plain workspace secret, never a registry entry (§Archetypes).
- Generic connect plumbing does not imply generic event behavior — inbound routing follows the provider's real shape (§The registry).
- A watchdog arms exactly ONE call at a time; sharing an instance across concurrent requests leaves one call unbounded (§Bounded outbound).
- No pool slot rides a vendor call — credentials load under a short acquire released before the exchange (§Bounded outbound).
- The App private key and webhook secret never enter the lease, runner environment, sandboxed child, logs, or response frames (§GitHub App).

## Terminology (binding)

| Term | Means | Lives in |
|---|---|---|
| **connector** | auth + credential plumbing for a third-party provider: the connect/callback/status routes, the vaulted per-workspace credential handle, the platform app secrets | `src/agentsfleetd/http/handlers/connectors/` |
| **integration** | a product-facing capability built ON a connector's credential (the Slack resident bot, GitHub fleet triggers, future Zoho/Jira/Linear surfaces) | feature code that consumes the vault handle |

A workspace *connects* a provider once (connector); everything fleets then do with that credential is *integration*. Specs, UI copy, and code comments follow this split — "Slack integration is broken" and "Slack connector is broken" name different layers.

## The registry: a provider is a data entry

`handlers/connectors/registry.zig` holds a comptime `[]const ConnectorSpec`. Adding a provider is ONE entry (plus a small per-provider hook file) — never new route or flow code:

```
            ┌──────────────────────────────────────────────────────────────────────┐
            │ REGISTRY = [_]ConnectorSpec{                                         │
            │   { provider, display_name, archetype: union(enum){                  │
            │       oauth2:      {flow, refresh, exchange_failed_code, post_auth}, │
            │       app_install: {state, build_install_url, complete},             │
            │   }, respond_status }                                                │
            │ }  + comptime validation (dup ids, scopes, id agreement…)            │
            └──────────────────────────────────────────────────────────────────────┘
   runtime lookup(provider) ── null → 404 UZ-CONN-004 (body names the id)
                              ── hit  → exhaustive switch on ARCHETYPE
            ┌───────────────────────────────────────────────────────────────────────────┐
            │ generic {provider} handlers: connect.zig · callback.zig · status.zig      │
            │ per-provider deltas: slack/{spec,callback,status}.zig,                    │
            │                      github/{spec,connect,callback,status}.zig,           │
            │                      zoho/{spec,callback,multi_dc}.zig,                   │
            │                      jira/{spec,callback}.zig, linear/{spec,callback}.zig │
            └───────────────────────────────────────────────────────────────────────────┘
```

- **Routes are generic.** `POST /v1/workspaces/{ws}/connectors/{provider}/connect`, `GET /v1/workspaces/{ws}/connectors/{provider}`, `GET /v1/connectors/{provider}/callback` — three matchers serve every provider (`route_matchers_connectors.zig`); scopes stay `connector:write`/`connector:read` on the generic variants. The shipped Slack/GitHub URLs are preserved verbatim because `slack`/`github` are registry ids.
- **Dispatch is on SHAPE, never on provider id.** The archetype tagged-union owns which flow runs; handlers switch exhaustively on it (a new archetype cannot land half-wired — the compiler forces every arm). No `if provider == "slack"` exists anywhere in the flow.
- **Invariants are compile-time facts.** Duplicate/empty provider ids, an oauth2 entry without scopes or an exchange-failed code, or a flow whose embedded provider id disagrees with its entry — all `@compileError`, not review vigilance.
- **Inbound routing follows the provider's real shape.** App-level webhooks whose payload carries a stable routing key use `POST /v1/ingress/{provider}`, but the shipped implementation is provider-owned: GitHub lives in `handlers/ingress/github.zig`, and its routing statements live with the GitHub connector in `handlers/connectors/github/sql.zig`. Slack keeps `POST /v1/connectors/slack/events` because its challenge, retry, timestamp, channel, and thread semantics are load-bearing. Jira and Linear have connected credentials but no inbound integration yet. Generic connect plumbing does not imply generic event behavior.

## Archetypes

| Archetype | Flow | Callback carries | Writes | Shipped instances |
|---|---|---|---|---|
| `oauth2` | authorize-redirect → code exchange (deadline-armed) → `post_auth` hook parses + persists | `code` + `state` | vault handle (+ provider-specific rows, e.g. Slack's `connector_installs`) | `slack`, `zoho` (multi-DC — the callback's `location` resolves the effective token endpoint), `jira`, `linear` |
| `app_install` | vendor install page → user authorization-code exchange → provider installation-access check → `complete` hook | `installation_id` + `code` + `state` | vault handle + non-secret connector-install routing row | `github` |

**There is no `api_key` archetype.** One was considered for operator-pasted vendor keys (Datadog, Grafana, Fly) and dropped (M108_002). A static vendor key is just a workspace secret referenced as `${secrets.<name>.<field>}`, not a connector: it never had a connect/callback round-trip or a platform app secret to protect. Those three providers are plain `agentsfleet secret create` entries, never registry entries. `REGISTRY.len` is pinned at 5 (`registry.zig`'s own pin test) — five OAuth/app-install connectors, not eight.

## Trust anchors

1. **The signed single-use `state` binds workspace intent.** It is keyed with the approval signing secret, workspace-bound, verified constant-time, and consumed exactly once. Forged, expired, or replayed state returns 400 `UZ-CONN-002`. State does not prove ownership of a provider installation.
2. **GitHub user authorization proves installation access.** The callback exchanges the one-time `code` with the platform `client_id` and `client_secret`, then calls GitHub's user-installation repository endpoint for the claimed `installation_id`. A denial returns 403 `UZ-CONN-008`; no vault or routing row changes. The datastore also refuses to move an installation already bound to another workspace.
3. **Platform app secrets live in the admin-workspace vault** as per-provider `<provider>-app` bags (`slack-app`, `github-app`, …) — one app per provider shared across all tenants, catastrophic-if-leaked, never on a per-tenant surface. GitHub's bag carries its App identity, user-authorization client credentials, and App-level webhook secret; an unprovisioned bag fails loud: 503 `UZ-CONN-001`.
4. **Provider signatures authenticate inbound events.** GitHub App traffic is verified against the platform `github-app.webhook_secret`; manual per-fleet webhooks still use the workspace `<source>.webhook_secret`; Slack App events use the platform `slack-app.signing_secret`. No inbound route falls back to Bearer authentication.

The connector registry owns callback dispatch; provider ingress handlers own event routing once the route segment has selected them. Detailed auth behavior and refusal codes live in [`../AUTH.md`](../AUTH.md) §OAuth connectors.

## GitHub App: platform setup to fleet execution

One GitHub App serves every tenant in an environment. The platform operator configures two different URLs on that App:

```
browser install callback              machine event ingress
/v1/connectors/github/callback        /v1/ingress/github
          │                                      │
          │ connects one workspace               │ wakes subscribed fleets
          ▼                                      ▼
 signed single-use state                 GitHub App signature
```

The platform identity lives only in the `agentsfleet-admin` workspace:

```
github-app
├── app_id              public App identifier
├── app_slug            public install-page handle
├── client_id           public user-authorization client identifier
├── client_secret       exchanges one-time user authorization codes
├── private_key_pem     signs App JSON Web Tokens for outbound token minting
└── webhook_secret      verifies inbound App deliveries
```

A workspace administrator connects GitHub once, chooses the GitHub account or organisation and the repositories the installation may access, and returns through the callback with `installation_id`, one-time `code`, and signed `state`. The callback accepts the connection only after both independent claims hold:

```
signed state ──────────────────────────────── proves intended workspace
one-time code → GitHub user token
              → GET /user/installations/{id}/repositories
              ─────────────────────────────── proves user can access installation
                                          │
                                          ▼
                         conditional datastore write
                         same workspace: create/reconnect
                         other workspace: 403, no mutation
```

The user token is request-local and discarded. After the checks pass, the callback writes both records on one database connection:

```
workspace vault                          core.connector_installs
github = {                               provider = github
  integration: github,                   external_account_id = installation_id
  installation_id                       workspace_id = connected workspace
}                                        credentials = NONE
```

The encrypted handle supports outbound token minting. The connector-install row is deliberately non-secret and supports inbound `installation.id → workspace` routing. Neither row alone is sufficient; callback failure leaves the workspace disconnected rather than half-connected.

### Repository and event subscriptions belong to fleets

The App installation chooses the maximum repository set GitHub will permit. Each fleet then declares the smaller set it wants to receive:

```yaml
triggers:
  - type: webhook
    source: github
    events: [pull_request]
    repositories: [acme/payments]
```

For App traffic, `repositories` is fail-closed: omission means the fleet receives no App delivery. The omission remains valid for the existing manual per-fleet GitHub route, whose URL already identifies the target fleet. This distinction prevents an App installed across an organisation from waking every GitHub fleet for every repository.

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
| Delivery replay slot | Exactly-once enqueue per matching fleet for one GitHub delivery |

### Where a grant comes from

A grant is **originated at install**, from the bundle fields the catalogue already
stores: installing a fleet that declares a required credential writes a `pending`
`core.integration_grants` row and raises an approval gate carrying the bundle's
stated reason. The seed runs synchronously in the create handler beside
`INSERT core.fleets` — deliberately not in the install-step progression, whose
every sub-step is best-effort by design, and where a failed seed would flip the
fleet to `active` carrying no grant.

The decision then belongs to the approval-gate machine this codebase already
ships: an inbox, a detail page with an evidence tree, resolve buttons, a webhook,
a timeout sweeper, and an append-only audit. **A gate is a per-event decision; a
grant is the standing answer that outlives the run.** The gate asks; the grant
remembers. Resolving the gate as approved flips the grant and the gate in one
statement, so the two cannot disagree; any non-approval outcome drives the grant
to `revoked` rather than back to `pending`, which nothing would re-raise.

Origination sits inside the middleware chain, and that placement is
load-bearing. The App ingress query inner-joins on `status = 'approved'`, so a
fleet that cannot obtain a grant writes no event, takes no lease, and reports
nothing — it goes silently inert rather than failing visibly. An origination
path reachable only with a credential the fleet does not hold produces exactly
that silence.

A lease is the last checkpoint: a credential that resolves to a mintable handle
with no approved grant **parks the event** rather than dropping the credential
and issuing a lease that can never mint. The delivery stays leasable, so the
next poll re-evaluates it and an approval takes effect with no redeploy.

An incoming delivery follows this order:

```
GitHub App delivery
  installation.id + repository.full_name + event + diagnostic delivery identifier
        │
        ▼
verify platform webhook signature BEFORE reading routing fields
        │
        ▼
installation.id → core.connector_installs → workspace
        │
        ▼
active fleets in that workspace
  ∩ source=github
  ∩ repositories contains repository.full_name
  ∩ events admits the incoming event
  ∩ approved GitHub integration grant
        │
        ▼
authenticated-body-digest/fleet replay slot → XADD fleet:{id}:events
```

Multiple fleets may intentionally subscribe to the same repository and event. Replay protection is therefore per authenticated payload body and fleet, not global. The signature-covered body digest is the replay identity; the unsigned delivery header is diagnostic only. If one fan-out leg fails, its slot is released and GitHub's retry completes that leg without duplicating successful fleets.

### Credential use remains separate from event receipt

Receiving a signed event does not hand GitHub credentials to a fleet. When a leased fleet later calls the GitHub API through `${secrets.github.token}`, the runner-token plane asks `agentsfleetd` to mint. The daemon derives the fleet and workspace from the lease, rechecks the approved integration grant, loads the workspace installation handle, signs with the platform private key, exchanges for a short-lived installation token, and returns that token for the tool call. The App private key and webhook secret never enter the lease, runner environment, sandboxed child, logs, or response frames.

### Provider impact

| Provider | Connect credential | Inbound events after M102_005 |
|---|---|---|
| <img src="https://cdn.simpleicons.org/github" width="14" alt="" /> GitHub | App installation handle | App ingress routes by installation + repository + event + grant; manual per-fleet webhook remains available |
| <img src="https://api.iconify.design/logos/slack-icon.svg" width="14" alt="" /> Slack | bot token from Open Authorization (OAuth) | unchanged specialized events route with team/channel routing |
| <img src="https://cdn.simpleicons.org/zoho" width="14" alt="" /> Zoho Desk | OAuth refresh handle, multi-data-center token endpoint | no inbound integration in this workstream |
| <img src="https://cdn.simpleicons.org/jira" width="14" alt="" /> Jira | OAuth refresh handle | no inbound integration in this workstream |
| <img src="https://cdn.simpleicons.org/linear" width="14" alt="" /> Linear | OAuth refresh handle | no inbound integration in this workstream |

## Bounded outbound: every vendor call is armed

`handlers/connectors/bounded_fetch.zig` is the **only sanctioned outbound HTTP entry** for connector code — grep-gated (spec eval E8): no raw `std.http.Client` elsewhere under `handlers/connectors/`. It mirrors the runner's control-plane client: pin the pooled socket → `arm` the watchdog → fetch → `disarm`, with the shared `Watchdog` promoted to the named module `src/lib/call_deadline/` (both build graphs consume it — the runner's deadlines are unchanged).

- **Fail-closed, no unbounded branch.** A call either runs armed or is refused: watchdog-unavailable (thread spawn failure) and pin failure both refuse the call (`UZ-CONN-003`, 502) instead of falling through to an unarmed fetch. The invariant is code-path-true — there is no fallback branch to take.
- **Deadlines are named per call class**, once: token exchange (10 s), outbound post (10 s), thread re-read (1.5 s — M106's ingress bound, kept).
- **Watchdog ownership follows the concurrency of the path.** A watchdog arms exactly ONE call at a time. The serialized outbound worker owns one across its loop; the request-concurrent paths (OAuth exchange, mention-ingress thread re-read) hold one per request — sharing an instance across concurrent requests would let two arms clobber each other and leave one call unbounded.
- **Residual window: connection setup.** Name resolution, the TCP dial, **and the TLS handshake** happen before a pooled handle exists to arm. DNS + dial are OS-bounded (connect timeouts); the TLS handshake read is **not** — `std.http.Client.connect` does TCP+TLS atomically, so we cannot arm between them without a setup deadline mechanism that does not exist yet. So a vendor that completes TCP then stalls the TLS handshake is the one unbounded branch left (tracked as a follow-up, together with bounding the outbound callers that are not connectors — JWKS, Clerk, OTLP, fleet-bundle fetches, and the credential broker's GitHub mint — which M108_001 deferred). The armed surface is the post-handshake read stage, where the M100/M106 incidents actually lived (vendor accepts + handshakes, then stalls the response). This is a strict improvement, not a regression: pre-M108 the *entire* call — connect, handshake, and read — was unbounded.
- **No pool slot rides a vendor call.** Credentials load under a short acquire released before the exchange; the events ingress pre-loads the bot token and returns its slot before the thread re-read (closes merged-PR #468's P1).

Deadline fired, watchdog unarmable, or vendor unreachable → `UZ-CONN-003` (502) + a `connector_vendor_call_refused` warn naming provider, call class, and `reason` (the per-class distinction) — never URL query or token material.

## Unknown vs unconfigured (the two front-door failures)

| Case | Meaning | Response |
|---|---|---|
| Unknown provider | `{provider}` not in the registry — nothing to configure | 404 `UZ-CONN-004`, body names the id, no side effects |
| Unconfigured provider | registry id whose `<provider>-app` bag is absent on this deployment | 503 `UZ-CONN-001`, fail-loud, no partial state |

## Adding a provider (the recipe)

1. Provider id as a `common` constant (RULE UFS) — it is simultaneously the route segment, the vault-key stem (`<provider>-app`, `fleet:<provider>`), and the registry id.
2. A `<provider>/spec.zig` data file (oauth2: endpoints/scopes; app_install: state binding) + the archetype's hook functions (oauth2: `post_auth` body parse + rows; app_install: `build_install_url` + `complete`).
3. One `ConnectorSpec` entry in `registry.zig`.
4. Provision the `<provider>-app` bag in the admin vault. (An operator-supplied vendor key with no browser round-trip — Datadog/Grafana/Fly's shape — isn't a connector at all; it's a plain workspace secret, `agentsfleet secret create`, never a registry entry.)
5. Tests: the generic-route suites already cover the flow; add hook-level tests for the provider's parse/persist deltas.

No route, matcher, scope, invoke, or OpenAPI edit — the `{provider}` form already covers the new id.
