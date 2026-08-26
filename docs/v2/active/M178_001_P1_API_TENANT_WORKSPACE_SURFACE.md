<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M178_001: Tenant and workspace surface — the human-facing route tree serves from Rust

**Prototype:** v2.0.0
**Milestone:** M178
**Workstream:** 001
**Date:** Aug 23, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — customer-facing parity; the Zig daemon keeps serving production while this lands
**Categories:** API
**Batch:** B4 — runs concurrently with M179 after M177
**Branch:** `feat/m178-tenant-workspace-surface`
**Test Baseline:** `unit=914 integration=0` — `make test-unit-all` on `main` at 414805429 (Rust workspace, 116 binaries); the integration lane is the M176 rustd suite and is counted at VERIFY, not here
**Depends on:** M177_001 (afd_fleet services, fleet-config parsing); M176_001 (auth, stores, shell)
**Provenance:** LLM-drafted (Claude Fable 5, Aug 23, 2026)
**Canonical architecture:** `docs/architecture/data_flow.md` (§A. INSTALL, §D. WATCH, §Two streams + one pub/sub channel) + `docs/architecture/web_app.md`

---

## Overview

**Goal (testable):** every tenant- and workspace-plane route this milestone owns — the Interfaces inventory below is authoritative; schedules and connector surfaces are M180's — serves from `agentsfleetd-rs` with the route-inventory test green and the existing hypertext-level integration subset passing against the Rust binary (full-route OpenAPI coverage is M181's oracle, once all routes exist).
**Problem:** this is the port's long tail — the largest handler surface (the Zig `http/` tree is ~33k non-test lines across 209 files, ~165 of them handlers) — and it is where quiet behaviour drift would hurt real dashboard and command-line users; it needs an oracle-driven, parallelizable port, not artisanal rewriting.
**Solution summary:** port the tenant/workspace handler groups onto the M176 shell as thin handlers over M176/M177 services, add the vault write path with its metadata projection, the Server-Sent Events (SSE) streaming endpoints over the subscription hub, and the device-flow session surface (relay-only — the key exchange is client-side); grade every group against the route-inventory test and the integration subset (full-route OpenAPI coverage is M181's oracle).

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): tenant + workspace surface with route parity
- **Intent (one sentence):** a dashboard or command-line client pointed at `agentsfleetd-rs` cannot tell the daemons apart across the tenant and workspace planes.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/http/route_template.zig` + `http/route_scopes.zig` — the authoritative route × scope inventory for the groups this milestone ports.
2. `docs/AUTH.md` §The three flows at a glance + `docs/AUTH_DEVICE_LOGIN.md` — the device-flow threat model; the daemon stores/relays `dashboard_public_key` + `ciphertext` and implements no curve (verified: elliptic-curve work lives in `cli/src/lib/cli-flow.ts:25`, P-256 via WebCrypto; the Zig daemon relays `dashboard_public_key`/`ciphertext` and shape-validates only — relay at `http/handlers/auth/session_helpers.zig:136-151`, validation codes at `:225-227`).
3. `src/agentsfleetd/state/vault.zig` — caller-owned opaque `key_name`, non-empty JSON object plaintext, and the metadata projection derived from the exact encrypted bytes in the same statement.
4. `docs/architecture/data_flow.md` §A. INSTALL (stream + group before 201, bounded retries, rollback) and §D. WATCH (SSE sequencing, reconnect backfill).
5. `docs/REST_API_DESIGN_GUIDELINES.md` — fires via the write_http dispatch for every handler file.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_api/**` | EDIT | Route variants + handler modules per group: auth-sessions, tenant, api-keys, cli-credentials, models, workspaces (fleets, secrets, fleet-libraries read, onboarding, preferences, approvals, events, streams, messages, memories, integration-grants) |
| `rustd/crates/afd_state/**` | EDIT | extends the M176-created repository crate: vault, billing/wallet reads, model library + tenant models, tenant provider, preferences, onboarding, signup bootstrap |
| `rustd/crates/afd_observability/**` | EDIT | PostHog product-event emission for the surfaces this milestone ports (§7) |
| `rustd/crates/afd_fleet/**` | EDIT | install flow (ensure-stream retries + rollback), approvals service + gate sweeper, steer message append |
| `rustd/crates/afd_auth/**` | EDIT | CLI-credential mint/revoke service glue; Clerk metadata fetch worker port |
| `rustd/crates/agentsfleetd/**` | EDIT | approval-gate sweeper + Clerk fetch worker join the supervisor |
| `rustd/crates/afd_core/**` | EDIT | the `UZ-AUTH-*` session codes and their problem entries — the registry subset this milestone's refusals need |
| `rustd/crates/afd_crypto/**` | EDIT | HMAC under a variable-length pepper key, which the device-flow code digest is computed with |
| `rustd/crates/afd_redis/**` | EDIT | the approve and owner-checked abort transitions beside M176's verify-and-consume |
| `rustd/crates/afd_wire/**` | EDIT | the request and response shapes this milestone's routes exchange: the device-flow bodies (§1) and the tenant plane's envelopes (§2) |
| `rustd/crates/afd_db/**` | EDIT | one `test-util` constructor, so a suite stubbing a pool-holding service answers with the refusal a real pool with no Postgres behind it gives, rather than inventing afd_db's failures from another crate |
| `rustd/Cargo.toml` + `rustd/Cargo.lock` | EDIT | new member |
| `docs/v2/active/M178_001_P1_API_TENANT_WORKSPACE_SURFACE.md` | EDIT | this spec: status, baseline, Discovery log, and the amendments to this table |
| `make/test-integration.mk` | EDIT | tenant/workspace integration subset runs against the Rust binary |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NSQ (verbatim schema-qualified SQL in `afd_state`), KYS (keyset pagination on list endpoints ports as-is), CTM (credential hash compares), ECL, PRI (prompt-injection resistance: fleet configs and messages are untrusted input), UFS, NDC, TST-NAM, MSID, ERR (existing UZ-* codes referenced), FLL.
- `dispatch/write_http.md` → `docs/REST_API_DESIGN_GUIDELINES.md` — REST rules for every handler.
- `docs/AUTH.md` — auth-flow rule for sessions/api-keys/cli-credentials work.
- `dispatch/write_rust.md` — REVIEW cites Microsoft guideline mnemonics.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes — the biggest surface | one module per route group; handlers thin (parse → service → serialize) |
| LOGGING | yes | scoped events; request bodies on sensitive routes erased after dispatch (port of `http/sensitive_request.zig` behaviour) |
| MILESTONE-ID | yes | none in source/tests |
| UFS | yes | route paths/scope names from single-source constants |
| SCHEMA GUARD | no | no schema change |
| SPEC TEMPLATE GATE | yes — this file | required sections filled |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/http/handlers/**` (Zig daemon) — per-group behaviour source of truth; the nearest existing handler is the pattern for each port.
- **Reference:** `~/Projects/oss/core_api-develop` — the `Result<Option<T>>` output-alias convention for repositories (failure ≠ absence) is adopted in `afd_state`; its 460-line hand-written route registry is the anti-pattern the Route enum already avoids.
- **API rules:** `docs/REST_API_DESIGN_GUIDELINES.md` + `public/openapi/` — the served-vs-documented parity gate is this milestone's outer oracle.

## Sections (implementation slices)

### §1 — Device-flow session surface (relay-only) — DONE

`POST/GET/DELETE /v1/auth/sessions*`, approve/verify, delete-all — over the M176 session store. The daemon validates shape (public key, ciphertext, verification code) and relays; no curve math server-side. 5-minute expiry; atomic state transitions; approve requires the dashboard principal, verify requires the session secret path per `docs/AUTH_DEVICE_LOGIN.md`.

- **Dimension 1.1** — DONE — full happy-path handshake against the store: create → approve → verify → credential minted → Test `test_device_flow_happy_path`
- **Dimension 1.2** — DONE — malformed public key / ciphertext → the documented ERR codes; expired session → the documented expiry behaviour → Test `test_device_flow_rejects_malformed`
- **Dimension 1.3** — DONE — approve/verify state machine is atomic under races (double-approve, verify-before-approve) → Test `test_device_flow_state_races_on_approve` + `test_device_flow_state_races_on_verify`

### §2 — Tenant plane

`/v1/tenants/me/*` (billing, charges, workspaces, provider, models CRUD), `/v1/models`, `/v1/api-keys[/{id}]`, `/v1/cli-credentials[/{id}]`, and workspace CRUD itself (`/v1/workspaces`, `/v1/workspaces/{workspace_id}`) — thin handlers over `afd_state`; api-key and cli-credential mint show the raw secret exactly once and store only hashes.

- **Dimension 2.1** — each tenant route: response-shape parity vs the Zig daemon on seeded data → Test `test_tenant_routes_shape_parity`
- **Dimension 2.2** — api-key half DONE (mint/list/revoke/delete over `afd_fleet::apikey`); the `afc_` command-line credential half is pending → Test `test_key_lifecycle_reveal_once`
- **Dimension 2.3** — keyset cursor + ordering vocabulary DONE (`afd_api::paging`, 10 unit tests); the seeded-row ordering proof lands with the api-key list handler → Test `test_list_keyset_pagination`
- **Dimension 2.4** — DONE — every route + method in this spec's Interfaces inventory exists in the Route enum; extras and gaps both fail → Test `test_route_inventory_matches_interfaces`

### §3 — Workspace fleets and install

`/v1/workspaces/{workspace_id}/fleets[/{fleet_id}]` CRUD + config PATCH (takes effect on next lease — no cache, no signal), and the install guarantee: event stream + consumer group created before the 201, bounded retries, exhaustion rolls back the Postgres row. Workspace ownership (`authorizeWorkspace`) composes with scopes on every route.

- **Dimension 3.1** — install creates stream+group before 201; injected Redis failure at each retry stage → rollback, no orphan row → Test `test_install_stream_guarantee_rollback`
- **Dimension 3.2** — config PATCH visible on next lease resolve, not before → Test `test_config_patch_next_lease`
- **Dimension 3.3** — ownership: a principal with valid scopes but the wrong workspace → 403/404 parity with Zig behaviour → Test `test_workspace_ownership_gate`
- **Dimension 3.4** — the committed FRONTMATTER corpus (TRIGGER.md → `config_json`, seeded from the `src/agentsfleetd/fleet_runtime/` frontmatter fixtures) parses to the same accept/reject verdicts and field values as `parseTriggerMarkdownWithJson`; malformed frontmatter (unclosed, wrong types, unknown keys) → the same error classes → Test `test_fleet_frontmatter_corpus_parity`

**§3 inherits M177 §5's install half (Indy, M177 stream).** M177 ported STORED config resolution only, because that is the half the runner plane calls. The install-time half — `config_markdown.zig` (338) + `yaml_frontmatter.zig` (272) — has four non-test callers and three are this milestone's: `fleets/create.zig:123`, `fleets/patch_txn.zig:114`, and `connectors/slack/channel_fleet.zig` (the fourth, `fleet_library/importer.zig:165`, is M179's and consumes the same entry point). It lands in `afd_fleet_runtime` beside the stored parser M177 built. **Implementation default:** a maintained serde-compatible YAML crate — `serde_norway` as of authoring (serde_yaml is archived); the agent re-verifies crate health at EXECUTE and records the pick in Discovery, because the fork-pinned `zig-yaml` rationale (upstream build breakage) dissolves only if the replacement is actually maintained.

### §4 — Vault and secrets routes

`/v1/workspaces/{id}/secrets[/{name}]` over afd_crypto envelopes: non-empty-JSON-object plaintext rule, caller-owned `key_name`, and the non-secret metadata projection (`meta_kind`, `meta_provider`, `meta_base_url`, `meta_has_key`) derived from the exact encrypted bytes and written in the same statement — list reads never decrypt.

- **Dimension 4.1** — store/read/list/delete round-trip; list returns projections without any decrypt call (assert via instrumented crypto layer) → Test `test_vault_list_no_decrypt`
- **Dimension 4.2** — projection/ciphertext cannot drift: same-statement write asserted; a Zig-written row lists identically from Rust → Test `test_vault_projection_parity`
- **Dimension 4.3** — non-object / empty plaintext rejected with the documented code → Test `test_vault_rejects_non_object`

### §5 — Events, SSE streams, messages, memories, grants

Workspace + fleet event lists (bounded, `since`-windowed), SSE streams (`/events/stream`) over the M176 hub: per-connection sequence counter resetting at 0, `Last-Event-ID` ignored, reconnect backfill via the events list with the 2-second overlap merge; `/messages` steer append (`XADD` → canonical event id); memories CRUD over the M177 store; integration-grants list/revoke.

- **Dimension 5.1** — SSE: ordered delivery, seq resets per connection, `Last-Event-ID` ignored → Test `test_sse_sequencing_semantics`
- **Dimension 5.2** — stream admission: the SSE cap sheds with the stream-class 429; ops routes unaffected → Test `test_sse_admission_cap`
- **Dimension 5.3** — steer message → stream entry id becomes the canonical event id; duplicate send stays idempotent per the documented dedup → Test `test_steer_append_event_id`
- **Dimension 5.4** — reconnect gap: client fetches `since` last-delivery−2s and merges by event id without duplicates → Test `test_sse_reconnect_backfill`

### §6 — Approvals and the gate sweeper

`/approvals[/{gate_id}]` + `:approve`/`:deny`, the approval-gate sweeper as a supervised task, and the terminal-row rule: gate-blocked rows are never reopened; a resolved gate lands a NEW event row via `actor=continuation:<original>`.

- **Dimension 6.1** — approve/deny transitions with scope + ownership gates; double-decision → conflict semantics parity → Test `test_approval_decision_races`
- **Dimension 6.2** — resolved gate emits a continuation event row; the blocked row stays terminal → Test `test_approval_continuation_row`
- **Dimension 6.3** — sweeper expiry behaviour matches the Zig sweeper on a seeded corpus → Test `test_approval_sweeper_expiry`

### §7 — Onboarding, preferences, fleet-library reads, analytics

`/onboarding`, `/preferences[/{pref_key}]`, workspace fleet-library reads; PostHog product-analytics port for the events these surfaces already emit (add none, rename none). **Implementation default:** PostHog over plain HTTP client calls in `afd_observability` — the Zig `posthog-zig` dependency retires with the port — because the event payload surface is small and a full SDK adds an unaudited dependency for no new capability.

- **Dimension 7.1** — preference/onboarding round-trips with shape parity → Test `test_prefs_onboarding_parity`
- **Dimension 7.2** — analytics: the existing event set fires with identical names/properties; nothing new emitted → Test `test_analytics_event_parity`

## Parallelization & execution map

(Internal batch labels here sequence THIS milestone's work only; the frontmatter **Batch:** line is the family-level ordering — two different axes, deliberately.)

| Batch | Scope | Runtime · model · reasoning tier | Why |
|---|---|---|---|
| B1 | §1 sessions + §4 vault | Claude Code · Opus 5 · xhigh | security boundaries (auth flow, secret storage) stay on the strongest tier |
| B1 | §3 fleets/install | Claude Code · Opus 5 · high | rollback choreography with a precise doc oracle |
| B1 | §2 tenant plane | Codex · GPT 5.6 sol · med | shape-parity porting with the route-inventory test + integration subset as oracle — cheap tier, tight oracle |
| B2 | §5 events/SSE/messages | Claude Code · Opus 5 · high | streaming semantics need care but are well-documented |
| B2 | §6 approvals | Claude Code · Opus 5 · high | race semantics + sweeper |
| B2 | §7 prefs/onboarding/analytics | Amp · GPT 5.6 sol · med | mechanical parity port; OpenCode · kim3 acceptable alternate |

Handler groups are disjoint file sets — parallel agents share one branch without collisions. Indy decides how many agents actually spin.

## Interfaces

```
Route groups ported (paths per src/agentsfleetd/http/route_template.zig):
  /v1/auth/sessions*                        §1
  /v1/tenants/me/* · /v1/models · /v1/api-keys* · /v1/cli-credentials* ·
  /v1/workspaces · /v1/workspaces/{workspace_id} (workspace CRUD)        §2
  /v1/workspaces/{id}/fleets*               §3
  /v1/workspaces/{id}/secrets*              §4
  /v1/workspaces/{id}/{events,events/stream} · fleet events/stream ·
  /messages · /memories* · /integration-grants*                          §5
  /v1/workspaces/{id}/approvals*            §6
  /v1/workspaces/{id}/{onboarding,preferences*,fleet-libraries}          §7
Response envelopes, error codes, pagination cursors: byte-parity with the
Zig daemon; public/openapi/ is the documented shape and stays UNCHANGED.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Install stream failure | Redis down during fleet create | bounded retries then rollback; caller gets the documented 5xx; no orphaned fleet row |
| SSE overload | viewer burst past the stream cap | stream-class 429 + Retry-After; existing connections unaffected |
| Hub gap | Redis pub/sub drop mid-stream | client reconnect + `since`-window backfill merges without duplicates |
| Session expiry mid-handshake | slow human | documented expiry code; re-login is clean |
| Double approval decision | two operators race | one wins; second gets conflict semantics parity |
| Vault plaintext invalid | non-object/empty payload | documented 4xx; nothing stored |
| Clerk metadata fetch failure | identity provider hiccup | worker retries with backoff; requests keep serving on cached scope data per Zig behaviour |
| Wrong-workspace access | scoped-but-foreign principal | ownership gate parity (403/404 as documented per route) |

## Invariants

1. Ownership and capability compose independently on every workspace route (`authorizeWorkspace` + `requireScope`) — enforced by the shared route layer, not per-handler code; `test_workspace_ownership_gate`.
2. Secrets are revealed exactly once at mint; storage is hash-only — `test_key_lifecycle_reveal_once`.
3. Vault list reads perform zero decrypt calls — instrumented crypto layer assertion in `test_vault_list_no_decrypt`.
4. SSE sequence ids are per-connection and non-durable; backfill is list-based — `test_sse_sequencing_semantics`.
5. Gate-blocked event rows are terminal; continuations are new rows — `test_approval_continuation_row`.
6. The daemon performs no elliptic-curve operations for device flow — enforced by afd_api's dependency set (no curve crate) + `test_core_dependency_freeze` extension.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| existing PostHog product events (unchanged set) | product | same user actions as today | same properties as today | no raw email/token/secret material | `test_analytics_event_parity` |
| SSE connection gauge + shed counter (existing families) | ops | stream connect/shed | route class, outcome | no payloads | `test_sse_admission_cap` |

No events added, renamed, or removed; no funnel change — analytics/funnel playbook untouched (parity port).

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_device_flow_happy_path` | create→approve→verify→afc_ credential minted; hash-only storage |
| 1.2 | integration (negative) | `test_device_flow_rejects_malformed` | bad key/ciphertext/expired → documented ERR codes |
| 1.3 | integration (race) | `test_device_flow_state_races` | double-approve / early-verify → exactly one legal outcome |
| 2.1 | e2e | `test_tenant_routes_shape_parity` | seeded data → field-level response parity vs Zig daemon |
| 2.2 | integration (negative) | `test_key_lifecycle_reveal_once` | second read shows metadata only; revoked key 401s immediately |
| 2.3 | integration | `test_list_keyset_pagination` | crafted rows → cursor order + boundary semantics parity |
| 2.4 | unit | `test_route_inventory_matches_interfaces` | Interfaces inventory ⊆ Route enum with methods; extras/gaps named |
| 3.1 | integration (negative) | `test_install_stream_guarantee_rollback` | injected failure per retry stage → rollback, zero orphans |
| 3.2 | integration | `test_config_patch_next_lease` | PATCH → old lease unaffected; next lease sees new values |
| 3.3 | integration (negative) | `test_workspace_ownership_gate` | foreign workspace + valid scopes → documented refusal |
| 3.4 | unit | `test_fleet_frontmatter_corpus_parity` | frontmatter corpus verdicts + field values equal `parseTriggerMarkdownWithJson`; malformed classes match |
| 4.1 | integration | `test_vault_list_no_decrypt` | list of N secrets → 0 decrypt invocations recorded |
| 4.2 | integration | `test_vault_projection_parity` | Zig-written row lists identically via Rust |
| 4.3 | unit (negative) | `test_vault_rejects_non_object` | `"x"`, `[]`, `{}` → documented 4xx each |
| 5.1 | integration | `test_sse_sequencing_semantics` | reconnect → seq restarts at 0; Last-Event-ID header ignored |
| 5.2 | integration (negative) | `test_sse_admission_cap` | cap+1 streams → one stream-class 429 |
| 5.3 | integration (replay) | `test_steer_append_event_id` | send → entry id = event id; duplicate → idempotent |
| 5.4 | e2e | `test_sse_reconnect_backfill` | kill + reconnect → merged, gapless, duplicate-free delivery |
| 6.1 | integration (race) | `test_approval_decision_races` | concurrent approve+deny → one decision, conflict for the other |
| 6.2 | integration | `test_approval_continuation_row` | resolve → new row with continuation actor; original terminal |
| 6.3 | integration | `test_approval_sweeper_expiry` | seeded expired gates → same outcomes as the Zig sweeper |
| 7.1 | integration | `test_prefs_onboarding_parity` | round-trip shape parity |
| 7.2 | integration | `test_analytics_event_parity` | recorded event stream equals the Zig daemon's on a scripted session |
| 2.1 (FM) | integration (negative) | `test_clerk_fetch_worker_retry` | identity-provider failure → worker backs off and retries; requests keep serving on cached scope data |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Route inventory parity for this milestone's groups (§2) | `cd rustd && cargo test test_route_inventory_matches_interfaces` | exit 0 | P0 | |
| R2 | Integration subset green on the Rust daemon | `make test-integration` (tenant/workspace lane) | exit 0 | P0 | |
| R3 | Security boundaries hold (§1, §4) | `cd rustd && cargo test device_flow` + `cargo test vault` | exit 0 | P0 | |
| R4 | Streaming semantics (§5) | `cd rustd && cargo test sse` | exit 0 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.lint`, `verify.unit`, `verify.version`) — the set `orly gate` runs; S5–S6 are the template's repository hygiene gates (secret scan, oversize sweep), deliberately outside the declared set; R-rows name oracles this spec's own Files Changed create, so every command is copy-paste by merge time.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Admin (`/v1/admin/*`) and operator (`/v1/fleets/*`) planes — M179, running concurrently.
- All signature-verified ingress (fleet webhooks, Svix, QStash, connectors, Slack, Clerk identity events) and schedules CRUD/sync — M180.
- Any `public/openapi/` change — the spec is the parity oracle; a mismatch is a Rust bug, not a docs edit.
- Credentials: no additions beyond the M176/M177 enumerations (Clerk issuer/audience already listed there).

---

## Product Clarity (authoring record)

1. **Successful user moment** — a teammate uses the dashboard and the CLI against a staging `agentsfleetd-rs` for a full session — login, fleet install, steer, watch the stream, approve a gate — and nothing feels or behaves differently.
2. **Preserved user behaviour** — every documented route, code, cursor, and stream semantic; `public/openapi.json` unchanged.
3. **Optimal-way check** — group-by-group parallel port under two oracles (route-inventory test + integration subset) is the direct path for a ~165-handler tail; anything cleverer adds drift risk.
4. **Rebuild-vs-iterate** — pure port; redesigns (route consolidation, envelope changes) are explicitly post-cutover. "Pure port" bounds the redesign, not the parity rule: a superseded or compatibility path that meets M181's single-implementation evidence bar (no in-tree emitter plus Indy's sign-off, recorded in Discovery) is left unported and registered as a declared divergence, not reproduced.
5. **What we build** — one repositories crate, handler groups over existing services, SSE plumbing, two supervised workers.
6. **What we do NOT build** — new endpoints, response-shape "improvements", OpenAPI edits, admin surface.
7. **Fit with existing features** — compounds with M177 (fleet service reuse); must not destabilize the live Zig integration lanes.
8. **Surface order** — both surfaces (CLI + UI) served by the same routes; parity, not preference, decides — divergence from CLI-first is justified: no new surface is being designed.
9. **Dashboard restraint** — no UI change ships; the dashboard keeps talking to whichever daemon serves the base URL.
10. **Confused-user next step** — unchanged error codes + `docs.agentsfleet.net` articles keep working; a parity bug reports as "Zig says X, Rust says Y" with the failing route named by the coverage gate.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven slices split by trust boundary first (sessions, vault isolated on the strongest tier), then by route group — because the parallelism Indy wants is only safe where file sets are disjoint and oracles are mechanical.
- **Alternatives considered:** splitting this milestone by crate rather than route group (rejected: route groups map 1:1 to the oracle's failure output, crates do not); porting the OpenAPI spec generation into Rust code annotations (rejected: the Redocly pipeline + coverage script already gate parity; churn without capability).
- **Patch-vs-refactor verdict:** this is a **refactor** (largest surface, same behaviour); verbatim SQL + two mechanical oracles keep 200 handlers honest.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.

### Declared divergences

- **The RLS session-context write is not ported.** `common_authz_sql.zig` keeps
  a second copy of the ownership verdict carrying
  `set_config('app.current_tenant_id', …)` in its select list. Nothing reads it:
  `schema/` declares no `ROW LEVEL SECURITY` policy and no
  `current_setting('app.current_tenant_id')` appears anywhere — the setting is
  written at three sites and read at zero. (The grep is discriminating:
  `current_setting('fleet.allow_gate_purge', …)` IS read, by seven triggers.)
  Left unported per Product Clarity 4's superseded-path clause; the ownership
  CHECK itself lands in full.

  **Settled Aug 27, 2026 — not ported.** Two things decided it. First, the
  readers were re-examined and every one is a Zig TEST: `common_authz_test.zig`
  creates a policy inside the test body (lines 339–343) to prove the mechanism
  and drops it again, so the setting has no production reader to serve — it is
  scaffolding for a feature `schema/` never adopted. Second, Indy's standing
  direction that this port reason in Rust rather than transcribe Zig: a session
  GUC is a Zig-shaped answer to connection lifetime, and `sqlx` has a different
  shape. Connections return to the pool between requests, so `set_config(…,
  false)` would leak one tenant's identifier onto whichever request drew that
  connection next — porting it faithfully would be a footgun guarding nothing.
  The Rust answer to the same intent already landed: ownership is a route fact
  the type system carries (commit `8ffa3dbd6`), enforced before a handler runs
  rather than advertised to a policy engine that was never switched on.

  If a real RLS policy is ever declared in `schema/`, the setting comes back
  transaction-scoped — `set_config(…, true)` inside an explicit `sqlx`
  transaction, never at session level — and that is a small, local change.

- **§1 `token_name` is held to printable ASCII.** `UZ-AUTH-017`'s registry entry
  documents "1 to 64 characters from space through tilde"; the Zig store bounds
  the LENGTH only, so a label carrying a newline is accepted there and refused
  here. The documented shape is the parity oracle this port grades against, so
  the stricter half wins — `afd_fleet::session::input::token_name_of` carries
  the reasoning and `a_token_name_outside_printable_ascii_is_refused` pins it.
- **§1 abort reasons are a closed set.** The Zig store takes the reason as a
  caller-supplied slice and the audit sink re-derives its own spelling, so the
  stored reason and the audited one agree by convention. `afd_redis::AbortReason`
  makes them one value, and a reason nobody declared cannot be written.
