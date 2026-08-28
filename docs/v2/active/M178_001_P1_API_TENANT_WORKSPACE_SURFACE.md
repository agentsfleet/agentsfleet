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
| `rustd/crates/afd_state/**` | EDIT | extends the M176-created repository crate: billing/wallet reads, model library + tenant models, tenant provider, preferences, onboarding, signup bootstrap. **Amended at EXECUTE — the vault did NOT land here.** This crate is the credential directory the authentication path cannot start without; putting AES-GCM behind it would rebuild every login when a projection column moved, and `afd_crypto` is an edge its three directories never call |
| `rustd/crates/afd_fleet_lifecycle/**` | ADD | §3's store: install, read, edit, purge. A new member rather than a module in `afd_fleet` (25,500 lines — 3.5× its nearest sibling, the condition that forced `afd_tenant` out) or in `afd_tenant` (which would acquire a YAML parser and a Redis stream client its api-key and login modules never call) |
| `rustd/crates/afd_vault/**` | ADD | §4's store: the sealed write, the never-decrypting list, and the reference lock a delete is taken under. A new member for the same reason §3 got one, plus one of its own: `afd_fleet::vault` already reads `vault.secrets` and stays put, because it is the RUNNER plane's reader — it opens a credential a fleet declared, refuses to degrade a row it cannot read, and never lists. Two failure policies over one table; folding them together means one of the two losing |
| `rustd/crates/afd_events/**` | ADD | §5's store: the narrative log's reads, the live tail, and the steer append. `core.fleet_events` had no owner — its statements sat in `afd_fleet::sql::event` behind the whole runner plane, so `afd_approval` carried a byte-identical COPY of the insert rather than depend on leases, money, policy and four sweepers to reuse ten lines of SQL. This crate is that reason removed |
| `rustd/crates/afd_billing/**` | ADD | **Indy, mid-stream.** `money/` plus the `billing.*` statements, out of `afd_fleet`. Reads a different schema from everything around it, and only the admission gate calls it |
| `rustd/crates/afd_runner/**` | ADD | **Indy, mid-stream.** `runner/` + `sweep/` plus `fleet.runners`/`fleet.runner_events`, out of `afd_fleet`. The HOST's plane where `afd_fleet` is the RUN's; the two meet at one audit row |
| `rustd/crates/afd_state/**` (vocabulary) | EDIT | `LEASE_STATUS_*` and `LAST_SEEN_NEVER` join `ADMIN_STATE_*`, which was already here because two planes read it |
| `rustd/crates/afd_core/src/event.rs` | ADD | `status` and `failure_label` spellings. NOT `afd_wire`, which depends on nothing but serde because it is a byte-exact port of the frozen `/v1/runners` contract — and neither column is on it |
| `rustd/crates/afd_observability/**` | EDIT | PostHog product-event emission for the surfaces this milestone ports (§7) |
| `rustd/crates/afd_fleet/**` | EDIT | install flow (ensure-stream retries + rollback), approvals service + gate sweeper, steer message append |
| `rustd/crates/afd_auth/**` | EDIT | CLI-credential mint/revoke service glue; Clerk metadata fetch worker port |
| `rustd/crates/agentsfleetd/**` | EDIT | approval-gate sweeper + Clerk fetch worker join the supervisor |
| `rustd/crates/afd_core/**` | EDIT | the `UZ-AUTH-*` session codes and their problem entries — the registry subset this milestone's refusals need |
| `rustd/crates/afd_crypto/**` | EDIT | HMAC under a variable-length pepper key, which the device-flow code digest is computed with |
| `rustd/crates/afd_redis/**` | EDIT | the approve and owner-checked abort transitions beside M176's verify-and-consume |
| `rustd/crates/afd_wire/**` | EDIT | the request and response shapes this milestone's routes exchange: the device-flow bodies (§1), the tenant plane's envelopes (§2), and the secret surface's create/list/replace payloads (§4) |
| `rustd/crates/afd_db/**` | EDIT | one `test-util` constructor, so a suite stubbing a pool-holding service answers with the refusal a real pool with no Postgres behind it gives, rather than inventing afd_db's failures from another crate |
| `rustd/Cargo.toml` + `rustd/Cargo.lock` | EDIT | new member |
| `docs/v2/active/M178_001_P1_API_TENANT_WORKSPACE_SURFACE.md` | EDIT | this spec: status, baseline, Discovery log, and the amendments to this table |
| `rustd/crates/agentsfleetd/src/preflight.rs` | EDIT | `API_URL` joins the resolved knobs — optional with `https://api.agentsfleet.net`, exactly as `runtime_loader.zig` reads it. A minted command-line credential records the deployment that issued it, and that must come from configuration rather than a request's `Host` |
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

- **Dimension 2.1** — DONE — each tenant route: response-shape parity vs the Zig daemon on seeded data → Test `afd_api/tests/tenant_shape_parity.rs` (12 cases). The oracle is the Zig handlers' `res.json(value, .{})`, which emits the struct's field set in DECLARATION order, so the suite pins key set AND order — a reordering a set comparison would call identical is one a client reading ordered columns can feel. What only a whole-surface suite sees is the cross-shape agreement the four per-route suites cannot: every paged envelope continues through `next_cursor` and spells an exhausted page as an explicit `null`, absent optionals stay on the wire (std.json's default, and the secret list is the documented exception), and `key` appears on the mint response and on no listing.
- **Dimension 2.2** — DONE. Api-key half over `afd_fleet::apikey` (mint/list/revoke/delete); `afc_` command-line half over `afd_fleet::cli_credential` (mint/revoke). The `afc_` mint is one transaction — advisory lock, owner-scoped revoke, insert — and the guard is a TYPE: `FreshSession` admits a browser session alone so a credential cannot mint its successor, `HumanIdentity` admits a terminal too so `logout` needs no browser. → Test `test_key_lifecycle_reveal_once` + `tenant_cli_credential.rs` (5 cases)
- **Dimension 2.3** — keyset cursor + ordering vocabulary DONE (`afd_api::paging`, 10 unit tests); the seeded-row ordering proof lands with the api-key list handler → Test `test_list_keyset_pagination`
- **Dimension 2.4** — DONE — every route + method in this spec's Interfaces inventory exists in the Route enum; extras and gaps both fail → Test `test_route_inventory_matches_interfaces`

### §3 — Workspace fleets and install

`/v1/workspaces/{workspace_id}/fleets[/{fleet_id}]` CRUD + config PATCH (takes effect on next lease — no cache, no signal), and the install guarantee: event stream + consumer group created before the 201, bounded retries, exhaustion rolls back the Postgres row. Workspace ownership (`authorizeWorkspace`) composes with scopes on every route.

- **Dimension 3.1** — DONE — install creates stream+group before 201; injected Redis failure at each retry stage → rollback, no orphan row → Test `test_install_stream_guarantee_rollback`
- **Dimension 3.2** — DONE — config PATCH visible on next lease resolve, not before → Test `test_config_patch_next_lease`
- **Dimension 3.3** — DONE — ownership: a principal with valid scopes but the wrong workspace → 403/404 parity with Zig behaviour → Test `test_workspace_ownership_gate`
- **Dimension 3.4** — DONE — the committed FRONTMATTER corpus (TRIGGER.md → `config_json`, seeded from the `src/agentsfleetd/fleet_runtime/` frontmatter fixtures) parses to the same accept/reject verdicts and field values as `parseTriggerMarkdownWithJson`; malformed frontmatter (unclosed, wrong types, unknown keys) → the same error classes → Test `test_fleet_frontmatter_corpus_parity` (`afd_fleet_runtime/tests/frontmatter_corpus.rs`, verdicts) + `frontmatter_fields.rs` (field values), both reading the same `tests/fixtures/fleetbundle/` the Zig suite reads

**§3 inherits M177 §5's install half (Indy, M177 stream).** M177 ported STORED config resolution only, because that is the half the runner plane calls. The install-time half — `config_markdown.zig` (338) + `yaml_frontmatter.zig` (272) — has four non-test callers and three are this milestone's: `fleets/create.zig:123`, `fleets/patch_txn.zig:114`, and `connectors/slack/channel_fleet.zig` (the fourth, `fleet_library/importer.zig:165`, is M179's and consumes the same entry point). It lands in `afd_fleet_runtime` beside the stored parser M177 built. **Implementation default:** a maintained serde-compatible YAML crate — `serde_norway` as of authoring (serde_yaml is archived); the agent re-verifies crate health at EXECUTE and records the pick in Discovery, because the fork-pinned `zig-yaml` rationale (upstream build breakage) dissolves only if the replacement is actually maintained. **Settled at EXECUTE: the default did not survive its own re-check** — `serde_norway` last published Dec 2024 — and the replacement is `saphyr-parser`, a tokeniser that resolves nothing, because the parity surface is `writeScalar`'s coercion table and not YAML's typing. Full reasoning and the measurements in Discovery.

### §4 — Vault and secrets routes — DONE

`/v1/workspaces/{id}/secrets[/{name}]` over afd_crypto envelopes: non-empty-JSON-object plaintext rule, caller-owned `key_name`, and the non-secret metadata projection (`meta_kind`, `meta_provider`, `meta_base_url`, `meta_has_key`) derived from the exact encrypted bytes and written in the same statement — list reads never decrypt.

- **Dimension 4.1** — DONE — store/list/replace/delete round-trip; the list returns projections with zero decrypt calls → Test `afd_vault/tests/integration_list_no_decrypt.rs` (4 cases). **The instrumented-crypto-layer plan did not survive EXECUTE, and what replaced it is stronger.** A tally counter proves what happened on the run that was measured, and only if every decrypt site remembered the funnel. Here the guarantee is structural — `Directory` holds no `Kek`, and `Envelope::open` takes one, so the listing half cannot decrypt — and it is checked observably as well: a row whose ciphertext has been corrupted still lists with its full projection, where `secret_list.zig` degrades that same row to an opaque `custom_secret`. The two implementations give different answers on that row, and the difference is the assertion.
- **Dimension 4.2** — DONE — projection/ciphertext cannot drift → Test `afd_vault/tests/integration_projection_parity.rs` (6 cases). Same-statement is asserted end to end: the suite opens the stored envelope with its own key and compares the four `meta_*` columns against a projection of that exact plaintext, after a create and again after a replace. The cross-daemon half seeds the exact column values `metadata.zig::project` writes and asserts this reader agrees with them — a Zig subprocess would add a build dependency to this lane without adding a fact, since what is under test is the READER's agreement with a column set.
- **Dimension 4.3** — DONE — non-object and empty plaintext rejected with `UZ-VAULT-001`, over-long with `UZ-VAULT-002`, and nothing stored → Test `afd_vault/tests/integration_reference_lock.rs` (7 cases) + `afd_api/tests/workspace_secrets.rs` (12 cases) + `afd_vault`'s own unit suite (28). The shape gate is a CONSTRUCTOR rather than a call each verb makes: `secrets.zig` runs `validateSecretName` and `vault.validateObject` at the top of the create and repeats two of the three at the top of the replace, and a third verb that forgot either would compile. A `SecretBody` cannot be anything but a non-empty JSON object within its bound, so there is no re-check to remember and none to delete.

**§4's reference lock is an RAII transaction, not a flag.** `secret_reference_txn.zig` carries a `Txn` with an `open` boolean and an idempotent `abort`, and its own module comment warns that `errdefer` is the wrong tool because every handler holding one returns `void` — two call sites had a rollback that was decoration. `sqlx::Transaction` rolls back when it is DROPPED, so every early return rolls back by the language's rules: no flag, no idempotent abort, and no path that can forget.

### §5 — Events, SSE streams, messages, memories, grants

Workspace + fleet event lists (bounded, `since`-windowed), SSE streams (`/events/stream`) over the M176 hub: per-connection sequence counter resetting at 0, `Last-Event-ID` ignored, reconnect backfill via the events list with the 2-second overlap merge; `/messages` steer append (`XADD` → canonical event id); memories CRUD over the M177 store; integration-grants list/revoke.

- **Dimension 5.1** — SSE: ordered delivery, seq resets per connection, `Last-Event-ID` ignored → Test `test_sse_sequencing_semantics`
- **Dimension 5.2** — stream admission: the SSE cap sheds with the stream-class 429; ops routes unaffected → Test `test_sse_admission_cap`
- **Dimension 5.3** — steer message → stream entry id becomes the canonical event id; duplicate send stays idempotent per the documented dedup → Test `test_steer_append_event_id`
- **Dimension 5.4** — reconnect gap: client fetches `since` last-delivery−2s and merges by event id without duplicates → Test `test_sse_reconnect_backfill`

### §6 — Approvals and the gate sweeper

`/approvals[/{gate_id}]` + `:approve`/`:deny`, the approval-gate sweeper as a supervised task, and the terminal-row rule: gate-blocked rows are never reopened; a resolved gate lands a NEW event row via `actor=continuation:<original>`.

- **Dimension 6.1** — DONE — approve/deny transitions with scope + ownership gates; double-decision → conflict semantics parity → Test `afd_fleet/tests/integration_approval_inbox.rs` (10 cases, live Postgres) + `afd_api/tests/workspace_approvals.rs` (10 cases, in front of the store). The race is Postgres's: both callers run one UPDATE carrying `WHERE status = 'pending'`, exactly one updates a row, and the loser's empty `RETURNING` is what tells "you decided this" from "somebody already had" — so `Resolution` has three arms and an already-answered gate reports the FIRST operator's attribution rather than the caller's. **Path divergence, decided by Indy:** the Zig daemon spells the decision `…/approvals/{gate_id}:approve`, and a router binds one parameter per segment, so that form is indistinguishable from the detail read beside it. The two carry different capabilities and the scope gate is a per-path layer, so the decision moved into its own segment — `…/approvals/{gate_id}/{decision}` — rather than the scope model giving way.
- **Dimension 6.2** — DONE — resolved gate emits a continuation event row; the blocked row stays terminal → Test `afd_approval/tests/integration_inbox.rs`. The continuation is part of RESOLVING rather than something a caller does afterwards: an approval that landed without one is a run a person unblocked and nothing restarted. It carries `actor=continuation:<blocked event>` and `resumes_event_id`, so the history reads forward without joining back through the gate table, and it is idempotent on the gate's action — `append_once` on the stream, `ON CONFLICT (fleet_id, event_id)` on the row — so a retried resolve continues the run once. A denial continues nothing, and neither answer ever rewrites the row the gate blocked.

**§6 lives in its own crate, `afd_approval`.** `afd_fleet::gate` is the RUNNER's side — it parks a run and reads the durable answer back — and the operator's side asks a different question of the same table. Keeping them apart is what lets the API's approval surface compile without leases, money, policy, bundles and the sweepers behind it. What the two share is one column's vocabulary, and that moved down to `afd_wire::approval::status` where both read it: two copies would let a row one plane wrote become one the other could not read. The operator type is `Decision` and has three arms rather than five — `pending` is not a decision, so the type cannot express it and the statement needs no guard against it.
- **Dimension 6.3** — DONE — sweeper expiry behaviour matches the Zig sweeper on a seeded corpus → Test `afd_fleet/tests/integration_approval_inbox.rs`. `Inbox::expire` takes only `pending` rows past their deadline, so an answer that landed a millisecond early stands — the operator's decision outranks the clock — and a swept row records `system:approval_gate_sweeper` with a reason, because an audit has to tell a gate a human denied from one that ran out of time.

### §7 — Onboarding, preferences, fleet-library reads, analytics

`/onboarding`, `/preferences[/{pref_key}]`, workspace fleet-library reads; PostHog product-analytics port for the events these surfaces already emit (add none, rename none). **Implementation default:** PostHog over plain HTTP client calls in `afd_observability` — the Zig `posthog-zig` dependency retires with the port — because the event payload surface is small and a full SDK adds an unaudited dependency for no new capability. **Settled at EXECUTE: this default did not survive its own re-check either, and Indy chose the crate.** The stated reasoning was wrong on two counts. `posthog-zig` is *agentsfleet's own* library (`build.zig.zon:60` → `github.com/agentsfleet/posthog-zig#v0.2.0`), so "retiring an unaudited dependency" was never the trade; and `posthog-rs` is PostHog's OFFICIAL Rust client (`github.com/posthog/posthog-rs`, MIT), v0.25.3 published Aug 26 2026 — two days before this read — with 1.55M downloads and eight releases in eighteen days. The parity argument inverted too: `posthog-zig` ALREADY injects `$lib`/`$lib_version` (`client.zig:134`) and already batches on a background flush thread, so hand-rolled HTTP would have had to reproduce both. Marginal cost with `default-features = false, features = ["async-client"]` is five new direct crates; `$os`/`$os_version` newly appear and `$lib` changes value, accepted as SDK-identification metadata rather than product event data.

- **Dimension 7.1** — DONE — preference/onboarding round-trips with shape parity → Test `afd_api/tests/workspace_preferences.rs` (10 cases, everything in front of the store) + `afd_tenant/tests/integration_preferences.rs` (7 cases, against live Postgres). The bag is `BTreeMap<&str, &RawValue>` rather than a parsed `Value`, so a stored preference returns byte for byte — a re-serialized `Value` would normalise spacing and key order on a payload the server has no business normalising, and the round-trip test pins exactly that. The key registry is a closed enum whose variant spelling IS the wire key, so `UZ-PREFS-001` is decided before a connection is drawn; the 1 KiB bound answers `UZ-PREFS-002` from the same verb, and both codes are asserted rather than the status alone.
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

- **§3.4 YAML crate — the spec's default was re-verified and REPLACED.** This
  section instructed the agent to re-check `serde_norway`'s health at EXECUTE.
  Measured Aug 27, 2026 on crates.io: `serde_norway` 0.9.42 last published
  Dec 21, 2024; `serde_yaml_ng` 0.10.0 last published May 26, 2024;
  `serde_yml` now publishes itself as DEPRECATED with a compatibility shim.
  The fork is as unmaintained as the `serde_yaml` it forked, so the stated
  rationale does not survive its own test.
  **Pick: `saphyr-parser` 0.0.12** (published Aug 18, 2026), and maintenance is
  the smaller half of why. `yaml_frontmatter.zig`'s `writeScalar` is a bespoke
  coercion table stricter than YAML 1.2, so this port does not want a YAML
  crate's typing at all — it wants the authored bytes. Proven by probe: the
  `saphyr` high-level loader reads `01` as `Integer(1)` and `1e5` as
  `FloatingPoint(100000.0)`, where the Zig writes the strings `"01"` and
  `"1e5"`; no post-processing recovers bytes a resolver discarded.
  `saphyr-parser` is the tokeniser underneath it and resolves nothing, which is
  the same shape the pinned `zig-yaml` fork's `.scalar` has. One new crate in
  the lock (`arraydeque`); `thiserror` was already there. Cost of the trade,
  stated: a 0.0.x series that may break API, confined to
  `afd_fleet_runtime::frontmatter::json`.
- **§3.4 declared divergences (five).** Recorded in
  `afd_fleet_runtime::frontmatter::json` and `::skill`, each pinned by a test.
  (1) A quoted magic word still collapses — `name: "true"` renders as the JSON
  boolean, matching the Zig, which loses quote style before `writeScalar` sees
  it; `saphyr-parser` hands the style over and this port discards it
  deliberately, because parity is the rule and the corpus grades verdicts.
  (2) Block scalars fold correctly here and are mis-lexed by the fork.
  (3) An apostrophe in a plain scalar no longer silently truncates the document
  (the M157 data-loss incident). (4) A missing fence, unreadable YAML, a
  duplicated key and a non-scalar key get four error variants where the Zig has
  one `MissingRequiredField`; the wire code is `UZ-AGT-008` either way, and
  `frontmatter_corpus.rs`'s `zig_class` folds the finer set back onto the Zig's
  vocabulary so the corpus is still graded in the oracle's own terms.
  (5) A wrong-typed OPTIONAL `SKILL.md` key is refused rather than silently
  dropped, extending the divergence M177 already declared for `skill`.
  Divergences 2 and 3 are cases where the Zig is wrong and silent; there is no
  honest way to port a silent wrong answer.
- **Corpus bug found, not fixed here: `tests/fixtures/fleetbundle/skill/missing_name.md`
  does not test what it claims.** Its own comment says it exercises an absent
  `name:`, but its `description` value carries a second `": "` — "…the required
  name: field." — which is not a valid plain scalar, so both daemons refuse it
  while tokenising, before any key is looked for. Verdict parity holds and the
  corpus row is green; the fixture is simply not covering the case it names.
  Left untouched because it is the Zig suite's input too and editing it changes
  that suite's behaviour — outside this section's blast radius. The case it
  meant to cover is held by
  `frontmatter::skill::tests::a_missing_name_names_the_key`.
- **Files Changed amendment: `make/test-integration.mk` does not exist.** M175
  §6 deleted it; the lane is `make/test-integration-rustd.mk` and it already
  sweeps `cargo test --workspace --all-features -- --ignored`, so §3's
  integration tests need no make edit. The table row is stale rather than
  pending.
- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Gate-flag triage (mechanical, auto-applied):** UFS `numeric-suspect` fired
  on `1000` and `1_000` inside `is_numeric`'s fixture list. Both are the
  SPELLINGS under test rather than values a constant could stand for, so they
  carry the gate's own `// pin test: literal is the contract` annotation.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.

### Declared divergences

- **`since=` refuses an impossible calendar date; the Zig rolls it over.**
  `parseRfc3339Z` validates the day as `1..=31` for every month and then runs a
  days-from-civil conversion, so it ACCEPTS `2026-02-31T00:00:00Z` and silently
  lands in March. `afd_events` shape-checks the same twenty-character `…Z` form
  and then hands the parse to `jiff`, which refuses it. That is a narrowing,
  which `docs/REST_API_DESIGN_GUIDELINES.md` §9 classes as breaking — taken
  deliberately, because an impossible calendar date is not a client contract
  and rolling it over answers a question nobody asked. The alternative was
  hand-writing Howard Hinnant's algorithm beside a maintained calendar crate
  the workspace already carries for `afd_billing::window`.

- **Two cursor formats survive, and neither is re-spelled.** The events
  endpoints take `?cursor=` carrying base64url of `{created_at}:{event_id}`
  (`fleet_events_filter.zig`), while `afd_core::paging::Cursor` spells a
  timestamp boundary `{millis}:{id}` in the clear (`keyset_cursor.zig`). The
  REST guide §3 forbids a request-side `?cursor=` name for NEW endpoints and
  §9 forbids renaming a query parameter inside `/v1` — "same rule for
  path-param names and query-param names". These are not new endpoints: the
  grammar is already exposed in production and a dashboard holds a cursor
  across a deploy, so §9 wins and §3's own grandfathering of `approvals/list`
  and `api_keys/list` is the precedent. `afd_events` carries its own `Cursor`
  with a test that decodes a cursor the Zig daemon issued. Converging the two
  is a `/v2` change.


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

- **`GET /v1/cli-credentials` is not ported, because it is not served.**
  `cli_credentials.zig`'s module comment describes three endpoints and names a
  list — "how an operator sees which terminals hold one" — but
  `route_table_invoke.zig` admits `POST` on the collection and `DELETE` on the
  item, and nothing else. The `_index_` test beside it is about a database
  INDEX, not a listing. So the list is documented and unserved; porting it
  would be adding an endpoint in a milestone whose rule is parity.
  `afd_api::services::TerminalCredentials` records this where the missing verb
  would otherwise read as an omission.

- **§1 `token_name` is held to printable ASCII.** `UZ-AUTH-017`'s registry entry
  documents "1 to 64 characters from space through tilde"; the Zig store bounds
  the LENGTH only, so a label carrying a newline is accepted there and refused
  here. The documented shape is the parity oracle this port grades against, so
  the stricter half wins — `afd_fleet::session::input::token_name_of` carries
  the reasoning and `a_token_name_outside_printable_ascii_is_refused` pins it.
- **§4 does not project `model` on the secret list.** `SecretSummary` documents
  it as an optional, nullable field and `secret_list.zig` answers it by
  DECRYPTING every row and reading the body's `model`. `vault.secrets` has four
  `meta_*` columns and none of them is `model`, so the only way to answer it on
  a list is an envelope open per row — which spec Invariant 3 forbids and
  `test_vault_list_no_decrypt` grades. The invariant wins and the field is
  omitted.

  Nothing observes the loss. The field is optional in the OpenAPI schema and
  `model?: string` in the dashboard's `Secret` union, so a response without the
  key validates and type-checks; the grep is discriminating — the dashboard
  reads `kind`, `provider` and `base_url` off this list and reads `model` only
  off `tenant_model_entries` (`model_id`) and the platform defaults, and the
  command-line `secret list` reads `name` and `created_at` alone.

  **The alternative was considered and not taken.** A `meta_model TEXT` column
  would serve the field with zero decrypts and keep byte-parity, and `model` is
  as non-secret as `provider` — but it is a schema change in a milestone whose
  Applicable Gates row says `SCHEMA GUARD | no | no schema change`, and it would
  only be honest once the Zig writers filled it too (Dimension 4.2 asks that a
  Zig-written row list identically from Rust), which makes it a cross-runtime
  edit plus a backfill for a field no client reads. If a client ever needs it,
  that is the change to make and it is small.

- **§4 lists a row this build cannot LABEL without its descriptors.** A row
  written before the projection columns existed carries NULL metadata; a row a
  newer daemon wrote may carry a `meta_kind` this build has no variant for. Both
  list as `custom_secret` — and both shed `provider` and `base_url` with it,
  where `rowToMetadata` keeps them. Reporting an opaque credential that still
  carried a provider label would contradict the union the dashboard narrows on,
  where that kind has no such field. Neither case is healed by decrypting:
  a heal-on-read path would put an envelope open back on the list and make
  "reads never decrypt" true only after warm-up. `agentsfleetd backfill` fills
  the first; the second is a newer daemon's vocabulary and is logged with the
  stored spelling.

- **§4's still-referenced refusal takes the selection lock later than Zig does.**
  `secret_reference_txn.zig` takes all three locks and lets its caller decide
  afterwards, so a delete that is about to be refused still holds
  `core.tenant_model_selection` for the tenant. Here the reference count is
  decided after step 2 and before step 3. Skipping a LATER lock cannot introduce
  a deadlock — a cycle needs two transactions each holding what the other wants,
  and a transaction that never takes the selection row cannot be in one — while
  taking it would hold a tenant-wide lock for the length of a transaction that
  is about to roll back. The protocol ORDER, which is the deadlock-freedom
  argument, is unchanged.

- **§1 abort reasons are a closed set.** The Zig store takes the reason as a
  caller-supplied slice and the audit sink re-derives its own spelling, so the
  stored reason and the audited one agree by convention. `afd_redis::AbortReason`
  makes them one value, and a reason nobody declared cannot be written.
