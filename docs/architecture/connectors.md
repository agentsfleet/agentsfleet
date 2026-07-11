# Connectors — the registry-driven connector platform

> Parent: [`README.md`](./README.md) · Sibling: [`../AUTH.md`](../AUTH.md) §OAuth connectors (flow behavior, trust-anchor mechanics, error taxonomy of the shipped providers).
>
> Scope: the platform shape — the comptime registry + archetype dispatch that makes a new provider a data entry, the callback and event-ingress trust anchors, the bounded-outbound rule for vendor calls, and the connector-vs-integration terminology. Read this before adding a provider or writing any connector outbound call. Flow behavior stays in AUTH.md; this doc owns the invariants that make the flows generic.

## Terminology (binding)

| Term | Means | Lives in |
|---|---|---|
| **connector** | auth + credential plumbing for a third-party provider: the connect/callback/status routes, the vaulted per-workspace credential handle, the platform app secrets | `src/agentsfleetd/http/handlers/connectors/` |
| **integration** | a product-facing capability built ON a connector's credential (the Slack resident bot, GitHub fleet triggers, future Zoho/Jira/Linear surfaces) | feature code that consumes the vault handle |

A workspace *connects* a provider once (connector); everything fleets then do with that credential is *integration*. Specs, UI copy, and code comments follow this split — "Slack integration is broken" and "Slack connector is broken" name different layers.

## The registry: a provider is a data entry

`handlers/connectors/registry.zig` holds a comptime `[]const ConnectorSpec`. Adding a provider is ONE entry (plus a small per-provider hook file) — never new route or flow code:

```
            ┌──────────────────────────── comptime ────────────────────────────┐
            │ REGISTRY = [_]ConnectorSpec{                                      │
            │   { provider, display_name, archetype: union(enum){              │
            │       oauth2:      {flow, refresh, exchange_failed_code, post_auth}, │
            │       app_install: {state, build_install_url, complete},          │
            │   }, respond_status }                                             │
            │ }  + comptime validation (dup ids, scopes, id agreement…)         │
            └──────────────────────────────┬────────────────────────────────────┘
   runtime lookup(provider) ── null → 404 UZ-CONN-004 (body names the id)
                              ── hit  → exhaustive switch on ARCHETYPE
            ┌──────────────────────────────┴────────────────────────────────────┐
            │ generic {provider} handlers: connect.zig · callback.zig · status.zig │
            │ per-provider deltas: slack/{spec,callback,status}.zig,              │
            │                      github/{spec,connect,callback,status}.zig,     │
            │                      zoho/{spec,callback,multi_dc}.zig,             │
            │                      jira/{spec,callback}.zig, linear/{spec,callback}.zig │
            └─────────────────────────────────────────────────────────────────────┘
```

- **Routes are generic.** `POST /v1/workspaces/{ws}/connectors/{provider}/connect`, `GET /v1/workspaces/{ws}/connectors/{provider}`, `GET /v1/connectors/{provider}/callback` — three matchers serve every provider (`route_matchers_connectors.zig`); scopes stay `connector:write`/`connector:read` on the generic variants. The shipped Slack/GitHub URLs are preserved verbatim because `slack`/`github` are registry ids.
- **Dispatch is on SHAPE, never on provider id.** The archetype tagged-union owns which flow runs; handlers switch exhaustively on it (a new archetype cannot land half-wired — the compiler forces every arm). No `if provider == "slack"` exists anywhere in the flow.
- **Invariants are compile-time facts.** Duplicate/empty provider ids, an oauth2 entry without scopes or an exchange-failed code, or a flow whose embedded provider id disagrees with its entry — all `@compileError`, not review vigilance.
- **Inbound routing follows the provider's real shape.** App-level webhooks whose payload carries a stable routing key use `POST /v1/ingress/{provider}` and the verifier/router registry. Slack keeps `POST /v1/connectors/slack/events` because its challenge, retry, timestamp, channel, and thread semantics are load-bearing. Jira and Linear have connected credentials but no inbound integration yet. Generic connect plumbing does not imply generic event behavior.

## Archetypes

| Archetype | Flow | Callback carries | Writes | Shipped instances |
|---|---|---|---|---|
| `oauth2` | authorize-redirect → code exchange (deadline-armed) → `post_auth` hook parses + persists | `code` + `state` | vault handle (+ provider-specific rows, e.g. Slack's `connector_installs`) | `slack`, `zoho` (multi-DC — the callback's `location` resolves the effective token endpoint), `jira`, `linear` |
| `app_install` | vendor install page → callback validates via `complete` hook | `installation_id` + `state` | vault handle + non-secret connector-install routing row | `github` |

**There is no `api_key` archetype.** One was considered for operator-pasted vendor keys (Datadog, Grafana, Fly) and dropped (M108_002): a static vendor key is just a workspace secret referenced as `${secrets.<name>.<field>}`, not a connector — it never had a connect/callback round-trip or a platform app secret to protect. Those three providers are plain `agentsfleet secret create` entries, never registry entries. `REGISTRY.len` is pinned at 5 (`registry.zig`'s own pin test) — five OAuth/app-install connectors, not eight.

## Trust anchors

1. **The signed single-use `state`** is the only trust anchor on the Bearer-less callback: HMAC'd with the approval signing secret, workspace-bound, verified constant-time and consumed exactly once. Forged/expired/replayed → 400 `UZ-CONN-002`.
2. **Platform app secrets live in the admin-workspace vault** as per-provider `<provider>-app` bags (`slack-app`, `github-app`, …) — one app per provider shared across all tenants, catastrophic-if-leaked, never on a per-tenant surface. GitHub's bag carries its App identity and App-level webhook secret; an unprovisioned bag fails loud: 503 `UZ-CONN-001`.
3. **Provider signatures authenticate inbound events.** GitHub App traffic is verified against the platform `github-app.webhook_secret`; manual per-fleet webhooks still use the workspace `<source>.webhook_secret`; Slack App events use the platform `slack-app.signing_secret`. No inbound route falls back to Bearer authentication.

The connector registry owns callback dispatch; the ingress registry owns App-event verification and routing. Detailed auth behavior and refusal codes live in [`../AUTH.md`](../AUTH.md) §OAuth connectors.

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
├── private_key_pem     signs App JSON Web Tokens for outbound token minting
└── webhook_secret      verifies inbound App deliveries
```

A workspace administrator connects GitHub once, chooses the GitHub account or organisation and the repositories the installation may access, and returns through the callback with `installation_id` plus signed `state`. After the state is verified and consumed, the callback writes both records on one database connection:

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

An incoming delivery follows this order:

```
GitHub App delivery
  installation.id + repository.full_name + event + delivery identifier
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
per-delivery/per-fleet replay slot → XADD fleet:{id}:events
```

Multiple fleets may intentionally subscribe to the same repository and event. Replay protection is therefore per delivery and fleet, not global: if one fan-out leg fails, its slot is released and GitHub's retry completes that leg without duplicating successful fleets.

### Credential use remains separate from event receipt

Receiving a signed event does not hand GitHub credentials to a fleet. When a leased fleet later calls the GitHub API through `${secrets.github.token}`, the runner-token plane asks `agentsfleetd` to mint. The daemon derives the fleet and workspace from the lease, rechecks the approved integration grant, loads the workspace installation handle, signs with the platform private key, exchanges for a short-lived installation token, and returns that token for the tool call. The App private key and webhook secret never enter the lease, runner environment, sandboxed child, logs, or response frames.

### Provider impact

| Provider | Connect credential | Inbound events after M102_005 |
|---|---|---|
| GitHub | App installation handle | App ingress routes by installation + repository + event + grant; manual per-fleet webhook remains available |
| Slack | bot token from Open Authorization (OAuth) | unchanged specialized events route with team/channel routing |
| Jira | OAuth refresh handle | no inbound integration in this workstream |
| Linear | OAuth refresh handle | no inbound integration in this workstream |

## Bounded outbound: every vendor call is armed

`handlers/connectors/bounded_fetch.zig` is the **only sanctioned outbound HTTP entry** for connector code — grep-gated (spec eval E8): no raw `std.http.Client` elsewhere under `handlers/connectors/`. It mirrors the runner's control-plane client: pin the pooled socket → `arm` the watchdog → fetch → `disarm`, with the shared `Watchdog` promoted to the named module `src/lib/call_deadline/` (both build graphs consume it — the runner's deadlines are unchanged).

- **Fail-closed, no unbounded branch.** A call either runs armed or is refused: watchdog-unavailable (thread spawn failure) and pin failure both refuse the call (`UZ-CONN-003`, 502) instead of falling through to an unarmed fetch. The invariant is code-path-true — there is no fallback branch to take.
- **Deadlines are named per call class**, once: token exchange (10 s), outbound post (10 s), thread re-read (1.5 s — M106's ingress bound, kept).
- **Watchdog ownership follows the concurrency of the path.** A watchdog arms exactly ONE call at a time. The serialized outbound worker owns one across its loop; the request-concurrent paths (OAuth exchange, mention-ingress thread re-read) hold one per request — sharing an instance across concurrent requests would let two arms clobber each other and leave one call unbounded.
- **Residual window: connection setup.** Name resolution, the TCP dial, **and the TLS handshake** happen before a pooled handle exists to arm. DNS + dial are OS-bounded (connect timeouts); the TLS handshake read is **not** — `std.http.Client.connect` does TCP+TLS atomically, so we cannot arm between them without a setup deadline mechanism that does not exist yet. So a vendor that completes TCP then stalls the TLS handshake is the one unbounded branch left (tracked as a follow-up alongside the non-connector-caller bounding in §Out of Scope). The armed surface is the post-handshake read stage, where the M100/M106 incidents actually lived (vendor accepts + handshakes, then stalls the response). This is a strict improvement, not a regression: pre-M108 the *entire* call — connect, handshake, and read — was unbounded.
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
