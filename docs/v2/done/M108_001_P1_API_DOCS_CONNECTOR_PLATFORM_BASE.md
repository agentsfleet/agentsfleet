<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M108_001: Connector platform base — registry-driven connector routes + bounded outbound HTTP

**Prototype:** v2.0.0
**Milestone:** M108
**Workstream:** 001
**Date:** Jul 02, 2026
**Status:** DONE
**Priority:** P1 — every upcoming customer-facing connector (Grafana, Zoho Desk, Jira, Linear, Fly, Datadog) blocks on this base; today each new provider re-implements routes/handlers by hand and every outbound vendor call is unbounded.
**Categories:** API, DOCS
**Batch:** B1 — base lands before M108_002 provider entries
**Branch:** feat/m108-connector-platform
**Test Baseline:** unit=2244 integration=227
**Depends on:** M106_001 (Slack connector — the second concrete connector this base generalizes), M102_001 (GitHub App connector — the app_install archetype's concrete instance), M99_001 (credential broker — the mint surface the platform extends, DONE)
**Provenance:** LLM-drafted (Claude Fable 5, Jul 02, 2026) — implementing agent cross-checks every claim against the tree

**Canonical architecture:** `docs/AUTH.md` §OAuth connectors is today's source of truth for the connector trust model; this workstream CREATES `docs/architecture/connectors.md` as the platform-shape home (registry, archetypes, bounded-outbound rule, connector-vs-integration terminology) and re-points AUTH.md cross-references at it.

---

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/connectors/oauth2.zig` + `connectors/slack/spec.zig` — the proto-registry this workstream formalizes: `oauth2.Spec` is already pure data; slack/spec.zig's header says "Adding Zoho/Jira/Linear is a sibling Spec like this one, not new flow code". The registry makes that sentence structurally true.
2. `src/runner/daemon/call_deadline.zig` + `control_plane_client.zig` (`pooledHandle`/`arm`/`disarm` around `fetch`) — the shipped `CallWatchdog` this workstream PROMOTES to `src/lib/`. Do NOT re-author the reverted thread-abandon watchdog; socket-shutdown-at-deadline is the house mechanism.
3. `src/agentsfleetd/http/route_matchers_connectors.zig` + `route_table_invoke_connectors.zig` + `routes.zig` (connector entries) — the per-provider route duplication the registry collapses.
4. `docs/AUTH.md` §OAuth connectors — trust anchors (signed single-use state; `<provider>-app` admin-vault bags; `fleet:<provider>` per-install handles; error taxonomy UZ-CONN-001/002, UZ-SLK-01x/02x).
5. `dispatch/write_zig.md` §Module Boundaries & Shared Modules — the `src/lib/` promotion rules (named module in both build graphs, no relative reach-across) and §Bun-Inspired Conventions (comptime registry + comptime validation; strategy tagged-union).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** part of PR #468 — `feat(m106+m108): Slack-resident bot + connector platform base`
- **Intent (one sentence):** adding the next connector to agentsfleet becomes a data entry (a registry `Spec` + a thin hook) instead of a hand-rolled route/handler set, and no connector call can hang the server on a silent vendor.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`; mismatch → STOP.

---

## Product Clarity

1. **Successful user moment** — an operator opens the dashboard connectors page after M108_002 and sees six new providers connectable; behind the scenes each of those cards cost one registry entry, and a Slack outage during a mention degrades that one mention instead of freezing ingress workers.
2. **Preserved user behaviour** — GitHub App install and Slack connect/callback/status/events flows keep working byte-identically at their existing URLs; existing vault handles and `connector_installs` rows stay valid; the runner's control-plane deadlines are unchanged.
3. **Optimal-way check** — unconstrained optimal is a full plugin system with per-connector packages; the gap (comptime in-tree registry instead) is deliberate: we control every connector, comptime validation beats dynamic loading, and the binary stays auditable. Acceptable until third parties author connectors.
4. **Rebuild-vs-iterate** — iterate: `oauth2.Spec` already exists as data; this promotes shape that shipped twice (M102, M106) into the registry. A rebuild (new connector service) would trade away run-to-run determinism and the single-binary posture for nothing at this scale.
5. **What we build** — comptime `ConnectorSpec` registry + archetype dispatch; generic `{provider}` connector routes; `CallWatchdog` promoted to `src/lib/`; connector outbound calls bounded; Slack+GitHub migrated onto the registry; `docs/architecture/connectors.md`.
6. **What we do NOT build** — new providers (M108_002); UI changes (M108_002 §UI); genericized inbound *events* ingress (each provider's inbound surface is bespoke by nature — Slack's stays as shipped); bounding of non-connector outbound callers (JWKS/Clerk/OTLP/fleet_bundle — follow-up, see Out of Scope).
7. **Fit with existing features** — compounds with the credential broker (M99/M102: mint surface gains providers cheaply in M108_002) and the fleet-trigger webhook surface (unchanged, stays distinct per AUTH.md). Must not destabilize: the Slack events ingress hot path (its zero-DB signature verify and 200-ack semantics are load-bearing for Slack retry behavior).
8. **Surface order** — API only in this workstream. Dashboard cards/forms land with providers in M108_002; CLI has no connector surface today and gains none here.
9. **Dashboard restraint** — nothing new rendered in this workstream; M108_002's catalog endpoint is where UI truth comes from (no hand-maintained provider lists in the app).
10. **Confused-user next step** — hitting a connector URL for an unknown provider returns 404 with a body naming the provider as unknown; an unconfigured provider returns 503 `UZ-CONN-001` (existing taxonomy) whose docs URL explains platform-app provisioning.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — **UFS** (provider ids/deadline constants are named once), **NDC/NLR** (per-provider route+handler dupes deleted in the same diff that generalizes them), **TGU** (archetype is a tagged union, never optional-field structs), **VLT** (tokens only in vault), **CTM/CTC** (state HMAC compare stays constant-time — existing code, do not regress), **ORP** (route-constant renames swept across matchers/table/OpenAPI/tests), **FLL**, **WAUTH** (workspace-scoped connect/status keep `authorizeWorkspace`).
- `dispatch/write_zig.md` — §Module Boundaries (src/lib named-module promotion), §Bun-Inspired Conventions (comptime registry + validation; strategy tagged-union owning its dispatch), §Concurrency (watchdog atomics/mutex commentary), §Tagged Unions for Result Types.
- `docs/REST_API_DESIGN_GUIDELINES.md` — generic route design, 404-vs-503 semantics, OpenAPI parity for the `{provider}` parameterization.
- `docs/AUTH.md` — §OAuth connectors (trust anchors this refactor must not weaken); auth-flow edits trigger `dispatch/write_auth.md`.
- `docs/LOGGING_STANDARD.md` — new `error_code=UZ-CONN-003` rows registered same-commit (ERROR REGISTRY).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — all-Zig diff | read façade; cross-compile x86_64+aarch64-linux per commit |
| PUB / Struct-Shape | yes — new `src/lib` module + registry pub surface | shape verdict per new pub; registry is a function-namespace module (conventional); consumers named per symbol |
| File & Function Length | yes | registry as its own file; route tables shrink (dupes deleted); split-before-cap if any file approaches 350 |
| UFS | yes | provider ids from `common` constants; per-class deadline constants named once; no re-spelled route literals |
| LOGGING / ERROR REGISTRY | yes | `UZ-CONN-003` (vendor call exceeded deadline / watchdog unavailable) registered in `error_registry.zig` same commit; `cp_call_deadline_fired`-style warn on fire |
| LIFECYCLE | yes — watchdog owns a thread | arm/disarm/deinit lifecycle tests move with the module; deinit joins the thread |
| SCHEMA GUARD | no | no schema changes |
| UI / DESIGN TOKEN | no | no UI files in this workstream |

---

## Overview

**Goal (testable):** `POST /v1/workspaces/{ws}/connectors/{provider}/connect`, `GET /v1/connectors/{provider}/callback`, and `GET /v1/workspaces/{ws}/connectors/{provider}` resolve any provider in the comptime registry through one generic route set (unknown provider → 404), Slack and GitHub keep passing their existing integration suites unmodified in behavior, and a connector outbound HTTP call against a hung vendor returns within its named deadline instead of parking the worker thread.

**Problem:** two connectors in, every provider costs a hand-rolled route trio + handler set (route_matchers, route_table, routes.zig, per-provider connect/status), and six more providers are queued. Separately, all nine `agentsfleetd` outbound `std.http.Client` call sites are unbounded — a vendor that accepts a connection and stalls parks a server worker forever (`fleet_bundle/github_net.zig` documents the hole; the runner solved it in M100 with `call_deadline.zig`, but only for its own client).

**Solution summary:** formalize the existing `oauth2.Spec`-as-data insight into a comptime `ConnectorSpec` registry with an archetype tagged-union (`oauth2` — with refresh flag + post-auth hook, `app_install`, `api_key`), collapse the per-provider connector routes into `{provider}`-parameterized matchers that resolve against the registry, and promote the runner's `CallWatchdog` to `src/lib/` so connector outbound calls arm a socket-shutdown deadline the way the runner already does.

---

## Prior-Art / Reference Implementations

- **Registry + comptime validation** → Bun's declarative `[]const Spec` tables with `comptime` duplicate/coverage assertions (`dispatch/write_zig.md` §Bun-Inspired Conventions); in-tree: `oauth2.Spec` + `slack/spec.zig` (the data shape already shipped).
- **Bounded outbound call** → `src/runner/daemon/call_deadline.zig` (`CallWatchdog`) + `control_plane_client.zig` `pooledHandle→arm→fetch→disarm`. Alignment: reuse verbatim semantics (fail-closed on `watchdog_unavailable`, fire-under-lock, per-verb named deadlines). Divergence: none — promotion, not reinvention.
- **API** → `docs/REST_API_DESIGN_GUIDELINES.md` + the shipped connector handlers under `handlers/connectors/`.
- **Shared module mechanics** → `src/lib/common/` (the existing named-module precedent for both build graphs).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/lib/call_deadline/call_deadline.zig` | CREATE (moved) | `CallWatchdog` + `Deadlines` promoted from `src/runner/daemon/call_deadline.zig`; consumed by both build graphs as named module `call_deadline` |
| `src/runner/daemon/call_deadline.zig` | DELETE | superseded by the `src/lib` module (Dead Code Sweep) |
| `src/runner/daemon/control_plane_client.zig` (+ `config.zig`, tests) | EDIT | import the named module; behavior unchanged |
| `build.zig`, `build_runner.zig` | EDIT | declare the `call_deadline` named module in both graphs |
| `src/agentsfleetd/http/handlers/connectors/registry.zig` | CREATE | comptime `ConnectorSpec` table + archetype tagged-union + comptime validation + lookup |
| `src/agentsfleetd/http/handlers/connectors/bounded_fetch.zig` | CREATE | the arm→fetch→disarm wrapper (mirrors `pooledHandle`); the ONLY sanctioned outbound HTTP entry for connector code |
| `src/agentsfleetd/http/handlers/connectors/oauth2.zig` | EDIT | exchange goes through `bounded_fetch`; `Spec` becomes the oauth2 archetype's payload (refresh flag + post-auth hook added for M108_002) |
| `src/agentsfleetd/http/handlers/connectors/{connect,callback,status}.zig` | CREATE | generic `{provider}` handlers dispatching on the registry archetype; per-provider deltas live in hooks |
| `src/agentsfleetd/http/handlers/connectors/slack/{connect,callback,status}.zig` | EDIT/DELETE | reduce to the slack hook (exchange-response parse, installs row, handle shape); dupes deleted per NLR |
| `src/agentsfleetd/http/handlers/connectors/github/{connect,callback,status}.zig` | EDIT/DELETE | reduce to the app_install hook (installation_id capture, install URL) |
| `src/agentsfleetd/http/handlers/connectors/slack/{post,thread}.zig` | EDIT | outbound calls through `bounded_fetch` with named deadlines |
| `src/agentsfleetd/http/route_matchers_connectors.zig`, `route_table_invoke_connectors.zig`, `routes.zig`, `route_scopes.zig` | EDIT | per-provider entries collapse to `{provider}` matchers resolved against the registry |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | add `UZ-CONN-003` (vendor deadline exceeded / watchdog unavailable) |
| `public/openapi.json` | EDIT | connector paths gain the `{provider}` parameterized form |
| `docs/architecture/connectors.md` | CREATE | platform shape: registry, archetypes, bounded-outbound rule, connector-vs-integration terminology |
| `docs/AUTH.md` | EDIT | §OAuth connectors points at the registry + generic routes; cross-reference the new architecture doc |
| existing Slack/GitHub connector integration tests | EDIT | only where imports/route constants moved — assertions unchanged (they are the migration proof) |

---

## Decomposition & alternatives

- **Chosen shape:** two workstreams — 001 mechanism (registry + bounded outbound + migration), 002 data (six providers + broker minting + UI). Mechanism must prove itself against the two shipped connectors before new providers pile on.
- **Alternatives considered:** (a) per-provider copy-paste continuation — rejected: six queued providers × four files each is exactly the rot the proto-registry comment warns against; (b) runtime plugin/config-file registry — rejected: loses comptime validation and the single-auditable-binary posture; (c) re-author the reverted thread-abandon watchdog — rejected: `CallWatchdog`'s socket-shutdown mechanism already shipped, is leak-free by construction (no abandoned threads), and is battle-tested by the runner.
- **Patch-vs-refactor verdict:** **refactor** (route/handler generalization) because the third-through-eighth consumers are already committed; the alternative is six mud-patches.

---

## Sections (implementation slices)

### §1 — Milestone credential enumeration (credential gate)

M108 as a milestone needs, at go-live of M108_002's OAuth providers: `zoho-app`, `jira-app`, `linear-app` bags `{client_id, client_secret}` in the admin-workspace vault (same `<provider>-app` convention as `slack-app`/`github-app`; source of record: 1Password `ops` vault items of the same names, provisioned by Indy before those providers leave "not configured"). The api_key archetype providers (`datadog`, `grafana`, `fly`) need **no** platform credential — the user supplies their own key at connect. This workstream itself ships with **zero new external credentials**: integration tests run against loopback fakes (M106 pattern), and an unprovisioned provider fails loud with 503 `UZ-CONN-001`.

- **Dimension 1.1** — ✅ DONE — connect for a registry provider whose `<provider>-app` bag is absent returns 503 `UZ-CONN-001` (fail-loud, no partial state) → Test "integration: unconfigured provider fails loud 503, no partial state" (`registry_integration_test.zig`; asserts 503 + `UZ-CONN-001` + no vault handle written) — `make test-integration` green (1848 pass / 0 fail)

### §2 — Bounded outbound HTTP (`CallWatchdog` promotion)

Move `call_deadline.zig` to `src/lib/call_deadline/` as the named module `call_deadline`, consumed by both build graphs (`src/lib` promotion approved — Discovery). Runner behavior is unchanged (its tests move with it). On the `agentsfleetd` side, `connectors/bounded_fetch.zig` wraps pin-pooled-handle → `arm` → `fetch` → `disarm` (mirror `control_plane_client.zig`), fail-closed on `watchdog_unavailable`. Per-class deadlines are named constants (token exchange, outbound post, thread re-read — thread re-read keeps M106's 1.5 s intent). **Implementation default:** one `CallWatchdog` per long-lived client context (events ingress ctx, outbound worker, broker), not per call.

- **Dimension 2.1** — ✅ DONE — `call_deadline` module compiles into both graphs; runner tests pass unchanged → evidence: `make test-unit-agentsfleet-runner` green (355 pass) + `zig build test-lib` (`agentsfleet-call-deadline-tests` 4/4 — the moved suite runs in its module's own compilation, the logging-module precedent) + all four linux cross-targets build
- **Dimension 2.2** — ✅ DONE — a hung vendor (fake that accepts then stalls) makes `bounded_fetch` return a deadline error within the named bound; caller surfaces `UZ-CONN-003`; worker thread is free (no park) → Test "bounded_fetch: deadline fires on a stalled vendor and surfaces DeadlineExceeded" (real loopback socket, unit lane — the house tier for socket-deadline proofs, mirroring the runner's client stall test; the listen-backlog trick makes "accepts then stalls" need no fake-server thread)
- **Dimension 2.3** — ✅ DONE — oauth2 exchange, slack post, and slack thread re-read all route through `bounded_fetch`; a repo grep proves no raw `std.http.Client` remains under `handlers/connectors/` outside `bounded_fetch.zig` → eval E8 empty + `make test-integration` green with the shipped M106 suites' assertions unchanged; the merged-PR P1 is closed: `post.deliver` loads row+token under one short acquire then releases before the vendor POST, and the events ingress pre-loads the bot token and hands its pool slot back before the thread re-read
- **Dimension 2.4** — ✅ DONE — watchdog-unavailable (forced spawn fail) refuses the call loud (502 `UZ-CONN-003`, reason logged), never runs unbounded → Test "bounded_fetch: watchdog unavailable refuses the call fail-closed (no unbounded run)" (+ the moved spawn-fail suite in `call_deadline`); amended 503→502: the registry binds ONE `http_status` per code (`hx.fail` — "HTTP status is owned by the error code table"), and `.bad_gateway` is the honest shared status for both sub-cases on the only HTTP-surfaced path (the exchange) — see Discovery

### §3 — `ConnectorSpec` registry + archetypes

`connectors/registry.zig`: a comptime `[]const ConnectorSpec` where `ConnectorSpec = {provider, archetype}` and archetype is `union(enum){ oauth2: Oauth2Data (endpoints, scopes, refresh: bool, post_auth hook), app_install: AppInstallData, api_key: ApiKeyData }` — the strategy tagged-union owns its dispatch; callers never switch on provider id. Comptime validation: unique provider ids; ids sourced from `common` constants (UFS); every oauth2 entry has nonempty scopes. Registry entries at the end of this workstream: `slack` (oauth2, refresh=false, slack hook), `github` (app_install). `api_key` variant lands with a comptime-asserted shape but no entries until M108_002 — no dead runtime code, the variant is data.

- **Dimension 3.1** — ✅ DONE — registry lookup resolves `slack`/`github`; unknown id returns null → 404 on every generic route → Tests "registry: lookup resolves the shipped providers to their archetypes" + "registry: unknown or empty provider resolves to null (the 404 path)" (unit) + "integration: unknown provider callback is a 404 whose body names it" (end-to-end, `UZ-CONN-004`) + router-shape test pinning the generic trio's captures
- **Dimension 3.2** — ✅ DONE — comptime validation rejects duplicate provider ids (also: empty id/display_name, oauth2 without scopes or exchange-failed code, flow/entry id disagreement, any api_key entry pre-implementation) → comptime block in `registry.zig` (violations are compile errors — documented as such per the doc-comment; runtime pin test "registry: exactly the shipped entries" guards the table length)

### §4 — Generic `{provider}` routes + Slack/GitHub migration

The three per-provider route sets collapse into `{provider}`-parameterized matchers resolving against the registry; scopes (`connector:read`/`connector:write`) unchanged (`route_scopes.zig` follows the generic form). Slack keeps its bespoke events ingress route untouched. Per-provider handlers shrink to hooks: slack = exchange-body parse + `connector_installs` row + handle shape; github = install URL + `installation_id` handle. Deleted dupes swept per ORP/NLR. **The shipped M106/M102 integration suites are the migration proof: their assertions do not change.**

- **Dimension 4.1** — ✅ DONE — Slack connect/callback/status integration suites pass against the generic routes with unchanged assertions → existing `oauth_callback_integration_test` + `events_integration_test` green with unmodified behavior assertions (`make test-integration`, 1848 pass / 0 fail)
- **Dimension 4.2** — ✅ DONE — GitHub connect/callback/status keep their shipped behavior (state-forged rejection, handle write, no installs row) → existing github connector suites green; `isNumericId` validation unit-pinned in the shrunk hook
- **Dimension 4.3** — ✅ DONE — OpenAPI documents the `{provider}` form; `make check-openapi` passes → eval E9 green (54 paths, REST §1 compliant; see Discovery — the connector surface was previously absent from OpenAPI entirely, so this landed as net-new documentation of all four routes)

### §5 — Architecture doc + terminology

CREATE `docs/architecture/connectors.md`: the registry/archetype shape, the two trust anchors (signed single-use state; platform `<provider>-app` bags), the bounded-outbound rule (every connector vendor call is armed), and the binding terminology — **connector** = auth + credential plumbing (this platform); **integration** = product-facing capability built on a connector. AUTH.md §OAuth connectors updates its cross-references; behavior prose stays in AUTH.md.

- **Dimension 5.1** — ✅ DONE — architecture doc exists (`docs/architecture/connectors.md`, indexed in the architecture README), AUTH.md §OAuth connectors cross-links it (heading, exchange prose, taxonomy rows `UZ-CONN-003/004`, Cross-references block, scope-catalogue row — stale per-provider route names swept), and the terminology table appears exactly once (grep: "product-facing capability built ON a connector" → 1 hit) → `make lint` green

---

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `connector_vendor_call_refused` (warn log, `error_code=UZ-CONN-003`) | ops | bounded_fetch refuses/fails a vendor call — deadline fired, watchdog unavailable, or vendor unreachable (`reason` distinguishes) | provider, call class, reason, deadline_ms | no URL query/token material | `test_bounded_fetch_deadline_fires` |

No product analytics change in this workstream (mechanism only; M108_002 adds the product events). Funnel playbook: no update required — reason recorded in Discovery.

---

## Interfaces

```
POST /v1/workspaces/{ws}/connectors/{provider}/connect   (Bearer, connector:write)
GET  /v1/workspaces/{ws}/connectors/{provider}           (Bearer, connector:read)  — status
GET  /v1/connectors/{provider}/callback                  (Bearer-less; signed state is the trust anchor)
POST /v1/connectors/slack/events                         (unchanged, bespoke)

registry.lookup(provider: []const u8) ?*const ConnectorSpec   (comptime table; runtime lookup)
bounded_fetch: arm(handle, deadline_ms) → fetch → disarm      (mirrors control_plane_client; fail-closed)
```

Existing per-provider URLs are preserved verbatim by the generic form (slack/github are registry ids). Response shapes of connect/callback/status are unchanged from M102/M106.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unknown provider | `{provider}` not in registry | 404, body names the unknown provider; no side effects |
| Provider unconfigured | `<provider>-app` bag missing | 503 `UZ-CONN-001` (existing), fail-loud, no partial state |
| Vendor hangs mid-exchange | upstream accepts then stalls | watchdog fires at deadline → socket shutdown → verb returns transport error → 502 `UZ-CONN-003`; no vault write (exchange precedes write) |
| Watchdog unavailable | thread spawn failure | call refused (502 `UZ-CONN-003`, reason=watchdog_unavailable — one registry status per code; see Discovery) — never unbounded |
| Forged/replayed state | attacker-supplied callback | 400 `UZ-CONN-002` (existing, constant-time verify) — regression-guarded by shipped tests |
| First-dial hang | dial itself stalls before a pooled handle exists | connect-phase semantics (not the read watchdog); documented in the architecture doc — read-phase is the bounded surface |
| Deadline fires during pool reuse | fd recycled between disarm and next arm | fire-under-lock discipline (shipped in CallWatchdog) prevents cross-call shutdown — covered by moved unit tests |

---

## Invariants

1. Registry ids are unique and drawn from `common` constants — **comptime assertion** (compile fails on violation).
2. No raw `std.http.Client` under `handlers/connectors/` outside `bounded_fetch.zig` — **grep gate** (eval E8; empty output required).
3. A connector outbound call either runs armed or is refused — **code path** (`watchdog_unavailable` returns error; no unbounded fallback branch exists to take).
4. Callback trust anchor stays the signed single-use state; no Bearer added to callback routes — **route table + shipped negative tests**.
5. Tokens/secrets appear only in vault rows (VLT) — **shipped tests** (oauth_callback suite asserts no token outside vault) remain green.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_connect_unconfigured_provider_503` | registry provider without vault bag → 503 `UZ-CONN-001`, no state minted |
| 2.1 | unit (existing) | runner `call_deadline` suite | arm/disarm/deinit, spawn-fail fail-closed, cross-thread mutex — pass unchanged from the new module path |
| 2.2 | integration | `test_bounded_fetch_deadline_fires` | loopback fake accepts + stalls → bounded_fetch errors within deadline+slack; `UZ-CONN-003` logged |
| 2.3 | integration + eval | `test_connectors_outbound_all_bounded` | E8 grep empty; slack post/thread/exchange behavior preserved via existing suites |
| 2.4 | unit | `test_watchdog_unavailable_fail_closed` | forced spawn fail → verb refused, no HTTP attempt |
| 3.1 | integration | `test_registry_unknown_provider_404` | `/v1/workspaces/{ws}/connectors/nope/connect` → 404; callback + status likewise |
| 3.2 | unit | `test_registry_comptime_validated` | registry length/coverage assertions compile; duplicate-id case documented as compile-error |
| 4.1 | integration (existing, e2e-shaped) | M106 slack suites | unchanged assertions pass on generic routes — the real-HTTP user-centric path |
| 4.2 | integration (existing) | github connector tests | unchanged assertions pass |
| 4.3 | eval | `make check-openapi` | OpenAPI matches the parameterized routes |
| 5.1 | eval | `make lint` docs/link checks | architecture doc present + linked |

Regression: the entire shipped connector test surface (M102 + M106) is the regression suite — assertions unmodified. Idempotency/replay: state single-use replay tests (shipped) stay green.

---

## Acceptance Criteria

- [x] Generic routes resolve slack + github; unknown provider 404 — verify: `make test-integration` ✅
- [x] Hung-vendor deadline fires; fail-closed on watchdog unavailable — verify: `make test-integration` + the §2 unit stall/spawn-fail suite ✅
- [x] No raw outbound client in connectors — verify: eval E8 (empty) ✅
- [x] Runner unchanged — verify: `make test-unit-agentsfleet-runner` green (355 pass, §2 evidence); runner graph cross-compiles both targets ✅
- [x] `make lint-all` clean · `make test-unit-all` passes · `make test-integration` passes ✅
- [x] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` (+ runner graph both targets) ✅
- [x] `gitleaks detect` clean · no non-test file over 350 lines added (generated `openapi.json` bundle exempt — build artifact) ✅
- [x] `make check-openapi` passes ✅

---

## Eval Commands (post-implementation)

```bash
# E1: Build — zig build
# E2: Unit — make test
# E3: Integration — make test-integration
# E4: Lint — make lint 2>&1 | tail -3
# E5: Cross-compile — zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo PASS
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate — git diff --name-only origin/main | grep -v -E '\.md$|_test\.zig$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2}'
# E8: Bounded-outbound invariant (empty = pass)
grep -rn "std.http.Client" src/agentsfleetd/http/handlers/connectors/ --include="*.zig" | grep -v bounded_fetch.zig | grep -v "_test.zig"
# E9: OpenAPI parity — make check-openapi
# E10: Old module gone (empty = pass) — grep -rn "daemon/call_deadline" src/ | head
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `src/runner/daemon/call_deadline.zig` | `test ! -f src/runner/daemon/call_deadline.zig` |
| per-provider connect/status handler files fully absorbed by generic handlers + hooks (exact set per PLAN) | `test ! -f …` per file |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `daemon/call_deadline` import path | `grep -rn "daemon/call_deadline" src/ \| head` | 0 matches |
| retired per-provider route constants | `git grep -rn -w '<route_const>'` (blast-radius form) | 0 matches outside history |

---

## Discovery (consult log)

- **Fold decision (Indy-acked):**
  > Indy (2026-07-02): "I prefer to have the M108 connector in this PR not a new one." — context: M108 rides PR #468 on `feat/m106-slack-resident`; no separate PR/worktree.
  > Indy (2026-07-02): "I need the M108 connector platform to be laid a based now, since you are shipping not just GH, an addtional Slack, which is multiple and we must be prepare and having a connector platform." — context: platform base greenlit ahead of the six providers.
- **Watchdog disposition (Indy-acked):**
  > Indy (2026-07-02): "Yes i agree on building watchdog, i am sure we would have fixed this before?" — context: confirmed prior art found — runner `call_deadline.zig` (M100); decision recorded to PROMOTE it to `src/lib/` rather than re-author the reverted thread-abandon patch (which was preserved only as wiring; its module file was never saved).
- **`src/lib/` promotion (gated):** reason — consumed by ≥2 build graphs (runner + agentsfleetd). Proposed to Indy in-session alongside the inventory table; treated as approved with this spec unless he objects at review.
- **Fold superseded by merge (Jul 02, 2026):** Indy merged PR #468 before M108 implementation began ("merged, git pull origin main, git prune" — 05:10 UTC), so the fold-into-#468 direction above is moot; this workstream proceeds on its own branch `feat/m108-connector-platform` (M108_002 follows on the same branch/PR, preserving the one-PR intent within M108). Greptile's P1 on the merged PR (`events.zig` — pooled connection held across the unbounded Slack fetch) is confirmed valid and is absorbed into §2's Dimension 2.3 acceptance: `bounded_fetch` loads the bot token before any HTTP begins and the connection is released before the fetch.
- **Metrics review:** no analytics/funnel playbook update required — mechanism-only workstream; ops signal added via `UZ-CONN-003` logging.
- **§2 implementation deviations (Jul 02, 2026, recorded at COMMIT):**
  1. **`UZ-CONN-003` is 502 for both sub-cases (spec said 503 for watchdog-unavailable).** The error registry binds exactly one `http_status` per code (`Entry.http_status`; `hx.fail`'s doc line: "HTTP status is owned by the error code table"), so one code cannot carry 502-or-503 by mode. `.bad_gateway` chosen: the only HTTP-surfaced path is the exchange callback, where both "vendor stalled" and "could not arm" read honestly as an upstream-call failure. Splitting into two codes was rejected — the spec's Files-Changed row defines ONE code covering both.
  2. **Watchdog ownership is per-context only where the context is serialized.** The outbound worker owns one across its loop (the spec's stated default). The OAuth exchange and the mention-ingress thread re-read run request-concurrently across server workers, and a `CallWatchdog` arms exactly one call at a time — a shared per-process instance would let two concurrent arms clobber each other and leave one call unbounded. Those two paths therefore hold a request-scoped watchdog (the request IS the client context); correctness forced the deviation from the "per long-lived ctx" wording.
  3. **Dim 2.2/2.4 tests sit in the unit lane** (real loopback sockets, no DB) — the house tier for socket-deadline proofs (the runner's own client stall test lives in its unit suite). The integration lane still covers the migrated paths end-to-end via the unchanged M106 suites.
  4. **`bounded_fetch` refuses on pin failure** (`VendorUnreachable`) instead of copying the runner client's fall-through-to-unarmed-fetch — Invariant 3 ("armed or refused") is code-path true with no unbounded branch to take.
  5. **Bonus NLR cleanups on touch:** `post.classifyStatus` deleted (dead once the body is always owned); the runner test root's inert `_ = @import("common")` line + the "contract + daemon + common" step description corrected (named-module tests never collected there — they run in `test-lib`); `build.zig`'s `test-lib` block extracted to `src/build/lib_tests.zig` when the new compilation pushed it past RULE FLL.
- **§3/§4 implementation deviations (Jul 02, 2026, recorded at COMMIT):**
  1. **`UZ-CONN-004` (404, unknown provider) registered** — the Files-Changed error-registry row listed only `UZ-CONN-003`, but Product Clarity §10's "404 with a body naming the provider" needs its own code (`hx.fail` requires a registry entry; reusing another code would lie about the mode). Registered same-commit per ERROR REGISTRY.
  2. **OpenAPI was net-new, not a parameterization** — the shipped M102/M106 connector routes were never documented in OpenAPI (zero `connector` hits in the bundle; a silent doc gap those milestones left). Dim 4.3 therefore CREATED `public/openapi/paths/connectors.yaml` documenting all four routes (the `{provider}` trio + the bespoke `slack/events` ingress, following the documented-webhook precedent) + a `Connectors` tag.
  3. **URL-shape checker touched on its documented carve-out surface** — `"connect"` added to `NOUN_FINAL_SEGMENT_ALLOW` (the shipped per-provider URLs pin the segment; a `:connect` colon-op rename would break live installs). Same edit removes the provably dead `"install"` + `"interactions"` entries (no OpenAPI path ends in either; no such routes in the router — RULE NLR on touch).
  4. **`github/spec.zig` created** (not in Files-Changed): the GitHub provider descriptor + state-domain binding as pure data, mirroring `slack/spec.zig` — the registry needs a data home per provider; state.zig's github-specific `Config` moved there when `github/state.zig` was deleted.
  5. **Spec test names → house descriptive strings**: the Test Specification's `test_*` identifiers landed as descriptive test names (e.g. `test_connect_unconfigured_provider_503` → "integration: unconfigured provider fails loud 503, no partial state"); mapping recorded per-dimension above.
  6. **api_key future-diff obligations bound in-code** (taste-audit outcome): `connect.zig` documents that the `approval_signing_secret` check must move into the two state-minting arms when api_key lands (an api_key connect has no state, so no secret requirement), and `callback.zig` documents that its `.api_key => unreachable` arm MUST become a 404 in that diff — the URL becomes reachable the moment an api_key id resolves.
  7. **`zig build test` `--listen` teardown notice**: the integration make target prints `failed command: …agentsfleetd-tests --listen=-` while exiting 0 with every test green — the binary run directly (no `--listen`) reports `1848 passed; 10 skipped; 0 failed` and exits 0, so the notice is protocol-mode teardown noise (non-zero process exit after all results transmitted), not a masked failure. Follow-up candidate, not blocking.
  9. **`/review` outcomes (Jul 02, 2026 — 5 specialists + red-team + Claude adversarial subagent; Codex-adv unavailable, stdin hang):** 11 actionable findings; the mechanical + in-scope ones fixed in `100f3680` (registry comptime hardening: oauth2 state-binding non-empty assert + cross-entry domain/nonce uniqueness; `bounded_fetch` redirect-unhandled + VendorUnreachable logging; github app_install callback e2e + cross-workspace IDOR tests + slack-app residue guard; stale route comments; OpenAPI callback-200 + events-413). **Four judgment items surfaced to Indy, disposition recorded:**
     1. **TLS-handshake pre-arm window (perf/red-team, conf 6).** `bounded_fetch`'s pin runs `client.connect` (TCP **and** TLS handshake) before the watchdog arms; the TLS handshake read is not OS-connect-bounded, so a vendor that completes TCP then stalls the handshake is unbounded. **Not a regression** (pre-M108 the whole call was unbounded) — this PR fixes the doc overclaim + adds the missing log; the structural fix (a connect-phase deadline mechanism, since `std.http.Client.connect` does TCP+TLS atomically) is a **follow-up** alongside §Out-of-Scope's non-connector-caller bounding.
     2. **`app_install` consumes state before validating `installation_id` (api-contract, conf 8).** The generic callback runs `verifyConsume` before the `complete` hook, so a GitHub redirect with a malformed `installation_id` now burns the single-use state (retry → 400 `UZ-CONN-002` instead of the old retryable 400). **Intended** — it is *stronger* anti-replay (state is single-shot regardless of downstream input validity), the practical retry path is "restart connect from the dashboard" which mints a fresh state, and no new exposure exists (an attacker holding the victim's state can already burn the flow). Documented here rather than special-casing pre-validation into the archetype dispatch. New test pins the malformed-id → 400 / no-handle behavior.
     3. **Registry doesn't validate hooks belong to their entry's provider (red-team, conf 7).** A future copy-pasted hook could read/write another provider's `fleet:<provider>` vault key and still compile. **Deferred to M108_002** — the clean fix needs each hook to self-identify its provider, and that diff touches every hook anyway (the api_key arms land there). Noted in-code intent.
     4. **GitHub `installation_id` ownership not verified at callback (red-team, conf 5).** Enumerable ids + no owner check → cross-tenant risk *iff* the broker mints from any id. **Pre-existing M102 behavior**, unchanged by M108, and bounding/verification of vendor identity is §Out-of-Scope (broker refresh-minting is M108_002). Follow-up: confirm broker-side re-verification when the six providers land.
  8. **`/write-unit-test` audit outcomes (Jul 02, 2026):** (a) **behavior fix** — `bounded_fetch.VendorUnreachable` (vendor down / dial refused / transport failure) fell through callback.zig's `else` arm to a 500 internal error; an upstream failure now rides `UZ-CONN-003`'s 502 with the other bounded-outbound refusals (registry description amended — one code per failure class). (b) **+11 integration tests** closing ledger rows that shipped uncovered in M102/M106 and moved under this refactor: generic-route scope enforcement 403, connect-side unconfigured 503 (Dim 1.1's literal wording — the prior test proved only the callback side), slack authorize-URL minting, github install-URL + no-slug 503, callback missing-state/missing-code 400s, status flips for both archetypes, vendor-unreachable 502 e2e, vendor-5xx 502 exchange-failed via a loopback fake. (c) **route_scopes_test pins the connector scope rows** (never pinned in any milestone). (d) One pre-existing state-dependence found outside the diff: `event_lifecycle` "consumer identity is stable" assumes flushed Redis (fails on direct re-run against a used instance; the canonical flush-first gate is green) — noted, not M108's.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | ✅ ran Jul 02, 2026 — 25-row diff ledger: 22 tested / 3 won't-test with reasons (2 OOM-wording fallbacks: 3-line static-fallback branches, FailingAllocator-through-full-harness cost ≫ blast radius; 1 e2e deadline-stall: mechanism already unit-proven in §2's loopback stall test, an e2e copy costs a 10 s wall-clock sleep per run). Outcomes in Discovery §3/§4 deviations #8: 1 behavior fix (VendorUnreachable → 502) + 11 integration tests + scope pins |
| With `/write-unit-test` at VERIFY | `/write-integration-test` | ✅ satisfied via the same ledger's integration lane — the 11 additions are service-layer tests through the real router/middleware/Postgres/Redis/loopback vendors with per-suite tenants + explicit in-body cleanup; drain audit via `make check-pg-drain` (in lint-zig, green), leak audit via `std.testing.allocator` + `make memleak` (green) |
| After tests pass, before CHORE(close) | `/review` | ✅ ran Jul 02, 2026 — 5 specialists + red-team + Claude adversarial subagent (Codex-adv unavailable). 11 actionable findings: mechanical/in-scope fixed in `100f3680`, 4 judgment items dispositioned in Discovery #9. Clean after fixes. |
| After PR update | `/review-pr` | per Indy's standing instruction this session: skipped unless he asks — record here |
| After every push | `kishore-babysit-prs` | final report in Discovery |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-all` (repo's tier-1 umbrella; spec's `make test` predates the target names) | all unit lanes green; TS coverage gates 100% | ✅ |
| Integration tests | `make test-integration` ×4 — incl. Tier-3 from clean state (`make down` first) | green every run; direct binary run: 1859 passed / 10 skipped / 0 connector failures (the 1 direct-run failure is a pre-existing flush-dependent event_lifecycle test — Discovery #8d) | ✅ |
| Lint | `make lint-all` (umbrella incl. lint-zig: fmt, ZLint, pg-drain, test-depth, cross-target, line-limit) | all checks passed | ✅ |
| Cross-compile | main + runner + **`test-lib`** graphs × `x86_64-linux` + `aarch64-linux` | all compile (end in "unable to execute binaries from the target" — the PASS signal). **Correction:** pre-`033e2776` the `test-lib` graph was NOT cross-compiled in verification (only `zig build test`); CI caught it — the standalone lib graph doesn't link libc, so `std.c.shutdown` in `call_deadline` was a Linux compile error. Fixed via a raw-syscall shutdown; all three graphs now green both targets. | ✅ |
| Gitleaks | `gitleaks detect` | 3076 commits scanned, no leaks found | ✅ |
| Bounded-outbound grep | eval E8 | 0 matches | ✅ |
| Dead code sweep | eval E10 | 0 matches | ✅ |
| OpenAPI | `make check-openapi` (eval E9) | bundle valid, 54 paths REST §1 compliant, 0 connector-path warnings | ✅ |
| Memleak | `make memleak` | "1418 passed; 440 skipped; 0 failed." → "✓ [agentsfleetd] memleak gate passed" (macOS SIP "not debuggable" line expected per VERIFY_TIERS) | ✅ |
| Bench | `make bench` (request-path touched: route matchers) | Tier-1 zbench passed; Tier-2 loadgen: 114,773 req, 0 fail, p50 2.8 ms / p95 7.2 ms / p99 16.8 ms @ 20-conn 20 s | ✅ |
| Acceptance e2e | `acceptance-e2e` / `cli-acceptance` | N/A — both suites cover auth-session lifecycle (Clerk sign-in, CLI login) against live deployments; no connector surface in their scope and the branch is undeployed | ⚪ |
| Test delta | `make _lint_zig_test_depth` | unit 2244→2270 (+26) · integration 227→243 (+16) vs CHORE(open) baseline (incl. the /review-added github-callback + IDOR tests) | ✅ |

---

## Out of Scope

- The six new providers, broker refresh-minting, catalog endpoint, and UI — **M108_002**.
- Bounding non-connector outbound callers (`auth/jwks.zig`, `auth/clerk_backend.zig`, `observability/otlp/post.zig`, `fleet_bundle/github_net.zig`, `credentials/serve_broker.zig` github mint) — follow-up spec once `bounded_fetch` proves out here; `github_net.zig`'s slot-isolation mitigation stays until then.
- Genericizing inbound event ingress surfaces (Slack events stays bespoke; future providers bring their own inbound shapes).
- Third-party/plugin connectors (registry stays in-tree + comptime).
