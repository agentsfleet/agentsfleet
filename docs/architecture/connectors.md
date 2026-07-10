# Connectors — the registry-driven connector platform

> Parent: [`README.md`](./README.md) · Sibling: [`../AUTH.md`](../AUTH.md) §OAuth connectors (flow behavior, trust-anchor mechanics, error taxonomy of the shipped providers).
>
> Scope: the platform SHAPE — the comptime registry + archetype dispatch that makes a new provider a data entry, the two trust anchors every flow hangs off, the bounded-outbound rule for vendor calls, and the connector-vs-integration terminology. Read this before adding a provider or writing any connector outbound call. Flow behavior stays in AUTH.md; this doc owns the invariants that make the flows generic.

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
- **Inbound event ingress stays bespoke on purpose.** Each provider's inbound surface has its own shape (Slack's signed events ingress at `POST /v1/connectors/slack/events` is load-bearing for Slack's retry semantics); the registry generalizes the *connect* plumbing only.

## Archetypes

| Archetype | Flow | Callback carries | Writes | Shipped instances |
|---|---|---|---|---|
| `oauth2` | authorize-redirect → code exchange (deadline-armed) → `post_auth` hook parses + persists | `code` + `state` | vault handle (+ provider-specific rows, e.g. Slack's `connector_installs`) | `slack`, `zoho` (multi-DC — the callback's `location` resolves the effective token endpoint), `jira`, `linear` |
| `app_install` | vendor install page → callback validates via `complete` hook | `installation_id` + `state` | vault handle only | `github` |

**There is no `api_key` archetype.** One was considered for operator-pasted vendor keys (Datadog, Grafana, Fly) and dropped (M108_002): a static vendor key is just a workspace secret referenced as `${secrets.<name>.<field>}`, not a connector — it never had a connect/callback round-trip or a platform app secret to protect. Those three providers are plain `agentsfleet secret create` entries, never registry entries. `REGISTRY.len` is pinned at 5 (`registry.zig`'s own pin test) — five OAuth/app-install connectors, not eight.

## Trust anchors (two — unchanged from M102/M106)

1. **The signed single-use `state`** is the only trust anchor on the Bearer-less callback: HMAC'd with the approval signing secret, workspace-bound, verified constant-time and consumed exactly once. Forged/expired/replayed → 400 `UZ-CONN-002`.
2. **Platform app secrets live in the admin-workspace vault** as per-provider `<provider>-app` bags (`slack-app`, `github-app`, …) — one app per provider shared across all tenants, catastrophic-if-leaked, never on a per-tenant surface. An unprovisioned bag fails loud: 503 `UZ-CONN-001`.

The registry refactor moved neither anchor: mechanics and behavior prose live in [`../AUTH.md`](../AUTH.md) §OAuth connectors.

## Bounded outbound: every vendor call is armed

`handlers/connectors/bounded_fetch.zig` is the **only sanctioned outbound HTTP entry** for connector code — grep-gated (spec eval E8): no raw `std.http.Client` elsewhere under `handlers/connectors/`. It mirrors the runner's control-plane client: pin the pooled socket → `arm` the watchdog → fetch → `disarm`, with the shared `Watchdog` promoted to the named module `src/lib/call_deadline/` (both build graphs consume it — the runner's deadlines are unchanged).

- **Fail-closed, no unbounded branch.** A call either runs armed or is refused: watchdog-unavailable (thread spawn failure) and pin failure both refuse the call (`UZ-CONN-003`, 502) instead of falling through to an unarmed fetch. The invariant is code-path-true — there is no fallback branch to take.
- **Deadlines are named per call class**, once: token exchange (10 s), outbound post (10 s), thread re-read (1.5 s — M106's ingress bound, kept).
- **Watchdog ownership follows the concurrency of the path.** A watchdog arms exactly ONE call at a time. The serialized outbound worker owns one across its loop; the request-concurrent paths (OAuth exchange, mention-ingress thread re-read) hold one per request — sharing an instance across concurrent requests would let two arms clobber each other and leave one call unbounded.
- **Residual window: the connect phase.** Name resolution, the TCP dial, **and the TLS handshake** happen before a pooled handle exists to arm. DNS + dial are OS-bounded (connect timeouts); the TLS handshake read is **not** — `std.http.Client.connect` does TCP+TLS atomically, so we cannot arm between them without a connect-phase deadline mechanism that does not exist yet. So a vendor that completes TCP then stalls the TLS handshake is the one unbounded branch left (tracked as a follow-up alongside the non-connector-caller bounding in §Out of Scope). The armed surface is the post-handshake read phase, where the M100/M106 incidents actually lived (vendor accepts + handshakes, then stalls the response). This is a strict improvement, not a regression: pre-M108 the *entire* call — connect, handshake, and read — was unbounded.
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
