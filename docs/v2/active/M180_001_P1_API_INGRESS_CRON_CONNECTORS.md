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

# M180_001: Signed ingress, cron, and connectors — the event producers serve from Rust

**Prototype:** v2.0.0
**Milestone:** M180
**Workstream:** 001
**Date:** Aug 23, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — trigger-plane parity; the Zig daemon keeps serving production while this lands
**Categories:** API
**Batch:** B5 — after M178 (approvals + workspace surface it feeds)
**Branch:** feat/m180-ingress-cron-connectors
**Test Baseline:** deferred to CHORE(close) per Indy override (Aug 29 2026): no `make test-unit-all` / `make test-integration-rustd` runs mid-milestone — `cargo fmt` + `cargo clippy` per section only; the full declared `verify.*` set runs once at the boundary, where the Test Delta is graded against `origin/main`'s counts
**Depends on:** M178_001 (approvals, workspace event surface); M179_001 (shared seams — `afd_api`, `rustd/Cargo.toml`, `make/test-integration.mk` — settle before ingress starts); M177_001 (fleet services); M176_001 (substrate)
**Provenance:** LLM-drafted (Claude Fable 5, Aug 23, 2026)
**Canonical architecture:** `docs/architecture/data_flow.md` §B. TRIGGER (six producers, one ingress) + `docs/architecture/connectors.md`

---

## Overview

**Goal (testable):** every signature-verified ingress route (fleet webhooks + approval + GitHub, Svix, QStash schedule fire, connector callbacks, Slack events, Clerk identity events), the schedules surface (CRUD + `/sync`) with its Upstash QStash (external cron provider) sync service, and the connector outbound worker serve from `agentsfleetd-rs` with signature verdicts, rejection codes, replay suppression, and stream writes equal to the Zig daemon.
**Problem:** the trigger plane is the daemon's unauthenticated-edge: HMAC (hash-based message authentication code) verification, timestamp windows, and replay suppression are the only wall between the internet and `XADD fleet:{id}:events` — a port defect here is a security defect, and cron double-fires or lost webhooks corrupt the "operational outcomes do not fall into limbo" promise.
**Solution summary:** port the signature wall — six verification paths (per-fleet webhook HMAC, approval HMAC, Svix, Slack v0, QStash JWT) plus the non-verifying trusted-client-IP derivation — with constant-time compares, the ingress handler groups, the schedules store + QStash client + sync service, the connector callback relay/complete pair + Slack events, and the outbound answer worker as a supervised task — graded by signature fixture matrices and the integration subset.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): signed ingress, schedules + QStash, connectors
- **Intent (one sentence):** every external event producer — webhook, cron, connector — lands events through `agentsfleetd-rs` with the same signatures accepted, the same forgeries rejected, and the same stream entries written.
- **Handshake** (filled at PLAN, Aug 29 2026): the Rust daemon takes over every route where something outside the platform starts work — a provider's webhook, a QStash schedule fire, a connector's OAuth return, a Slack event — verifying each delivery's signature exactly as the Zig daemon would, writing the same stream entries, suppressing the same replays, and delivering fleet answers outward through the same queue; a provider can be pointed at either daemon and observe no difference except the unified rejection codes recorded below. `ASSUMPTIONS I'M MAKING:` 1. rejection codes unify on `UZ-WH-*` across this surface (Indy's call, Aug 28) — a recorded divergence from the Zig, not parity; 2. the M178 approvals service and M179 seams on `main` are the ones this builds on; 3. the connector provider set is the Zig registry's five (Slack, GitHub, Zoho, Jira, Linear), no additions; 4. the crate verdicts in Prior-Art below are settled and not re-litigated at REVIEW.

## Implementing agent — read these first

1. `docs/architecture/data_flow.md` §B. TRIGGER — the six producers on one ingress, entry-id-as-event-id, the three webhook rejection codes (UZ-WH-020 misconfig · UZ-WH-010 bad signature · UZ-WH-011 stale timestamp, 5-minute window), and QStash replay suppression.
2. `src/agentsfleetd/crypto/hmac_sig.zig` — the canonical HMAC construction (single source; scrubbed key pads) the Rust canon must match bit-for-bit.
3. `src/agentsfleetd/auth/middleware/` — `webhook_sig.zig`, `webhook_hmac.zig`, `svix_signature.zig`, `trusted_client_ip.zig` — plus the two verifiers outside that tree: `http/handlers/connectors/slack/slack_sig.zig` (in-handler, per-request vault secret) and `cron/QStashVerifier.zig` (HS256 JWT, dual-key rotation). Verification order and failure codes.
4. `src/agentsfleetd/cron/` — `Service.zig`, `Store.zig`, `QStashClient.zig`: the daemon owns no timer; QStash calls back in, signature-verified.
5. `src/agentsfleetd/queue/` outbound worker + `docs/architecture/connectors.md` — connector answer delivery semantics.

## Files Changed (blast radius)

Measured, not planned: `git diff --name-only origin/main...HEAD`, grouped by crate.
Two rows of the original table named crates that were never built — `afd_qstash`
and `afd_schedule` were folded into a single `afd_cron` during §3, because the
vendor boundary and our own rows turned out to share a store and splitting them
bought two crates and no seam. Four rows are new since the original: M185 split
`afd_api` into a substrate plus planes underneath this branch, so this milestone's
routes re-homed into `afd_api_ingress` (a NEW fifth plane, created here) and
`afd_api_tenant`, over an `afd_http` substrate.

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_connector/**` (24) | CREATE | §4 — OAuth state mint/verify/consume, the provider registry, callback grants into the vault, Slack event handling |
| `rustd/crates/afd_cron/**` (17) | CREATE | §3 — one crate for both halves: the inbound HS256 delivery verifier, the outbound schedule REST client, the store, sync leases, and cron/timezone validation with the parity guard |
| `rustd/crates/afd_api_ingress/**` (16) | CREATE | §2/§4 — the NEW fifth plane: every webhook route plus connector events. Everything a sender proves with a signature over the body rather than with a bearer |
| `rustd/crates/afd_api/**` (16) | EDIT | Composition root after M185: mounts the planes, owns the route inventory tests |
| `rustd/crates/afd_redis/**` (15) | EDIT | dedicated (non-multiplexed) connection seam for the blocking outbound consumer — the `hub.rs` precedent — and the `append_once` claim script |
| `rustd/crates/agentsfleetd/**` (13) | EDIT | outbound worker joins the supervisor; supervised inventory counts it |
| `rustd/crates/afd_ingress/**` (12) | CREATE | §2 — the append path itself: `deliver.rs`, the `OnceScope` claim, secret resolution |
| `rustd/crates/afd_outbound/**` (10) | CREATE | §5 — the delivery queue and its supervised worker. **The only crate that imports a connector poster**, keeping the report path provider-agnostic (the Zig's Invariant 9) |
| `rustd/crates/afd_http/**` (10) | EDIT | M185 substrate: route metadata for the new paths, `provider_of`, `FleetPath`/`parse_fleet_id`, `APPROVAL_IDENTITY` — the two symbols both planes needed, which is what forced them down here |
| `rustd/crates/afd_api_tenant/**` (10) | EDIT | bearer-proven connector surface (catalogue, connect, status, callback) + fleet schedules |
| `rustd/crates/afd_webhook/**` (9) | CREATE | §1 — the signature wall: scheme table, Slack v0, vendored Svix verifier under `src/vendor/`. Pure verdicts, no datastore and no framework, so every branch is provable without either |
| `rustd/crates/afd_core/**` (5) | EDIT | `UZ-WH-*` / `UZ-CONN-*` codes declared in the error-code registry (`error_code/request.rs` family) |
| `rustd/crates/afd_vault/**` (3) | EDIT | the grant seal's key-name path |
| `rustd/crates/afd_crypto/**` (3) | EDIT | `Mac256` → `HmacSha256Tag` rename (mechanical, its own commit before §1) |
| `rustd/crates/afd_wire/**` (2) | EDIT | connector response types, moved out of the handlers so the CLI and the app read one shape |
| `rustd/crates/{afd_fleet_runtime,afd_fleet_lifecycle}/**` (2 each) | EDIT | event append call sites following the ingress seam |
| `rustd/crates/{afd_tenant,afd_runner,afd_approval}/**` (1 each) | EDIT | the renamed tag type and the approval resolve path |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | new members + the cron-parser, `subtle`, `backon` and `jiff` dependencies |
| `public/openapi.json` | EDIT | the new ingress/schedule/connector paths — **and three custom verbs moving segment**, see the docs entry in Discovery |
| `cli/src/lib/api-paths.ts`, `cli/test/fleet-schedule.*.test.ts` | EDIT | `/sync` → `/sync` in the client and its tests |
| `ui/packages/app/**` (2) | EDIT | the same path change on the dashboard's schedule call |
| `tests/fixtures/webhooks/**` (5) | CREATE | GitHub delivery corpus for Dimension 2.3 — `github_ping`, `github_pull_request`, `github_run_failure`, `github_run_failure_app`, `github_run_success` |
| `make/test-integration-rustd.mk` | EDIT | ingress/cron/connector subset against the Rust binary |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — CTM (every signature compare constant-time — the load-bearing rule of this milestone), PRI (webhook and Slack payloads are untrusted input), TIM (the 5-minute timestamp window is an explicit named invariant), ECL (provider outage ≠ bad signature), NSQ, UFS, NDC, TST-NAM, MSID, ERR (UZ-WH-* referenced), FLS, FLL.
- `dispatch/write_http.md` → `docs/REST_API_DESIGN_GUIDELINES.md` — for the schedules CRUD surface.
- `dispatch/write_rust.md` — deterministic replay/race tests; REVIEW cites Microsoft guideline mnemonics.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes | one middleware per module; handler-per-provider |
| LOGGING | yes | rejected ingress logs code + provider, never payload bytes or signatures |
| MILESTONE-ID | yes | none in source/tests |
| UFS | yes | window widths, header names, provider ids as named constants |
| SCHEMA GUARD | no | no schema change |
| SPEC TEMPLATE GATE | yes — this file | required sections filled |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/` ingress/cron/connector trees (Zig daemon) — behaviour and code source of truth; `crypto/hmac_sig.zig` is the byte-level oracle for the signature canon.
- **Reference:** M176 afd_crypto — the HMAC canon lands there once; this milestone consumes it (no second implementation — RULE UFS/OWN). Renamed `HmacSha256Tag` here: the type is the authentication tag, not the algorithm.
- **Reference:** `docs/architecture/data_flow.md` §B. TRIGGER — the invariant table is the acceptance oracle for replay behaviour; rejection codes diverge deliberately (see Interfaces).

**Crate verdicts (settled Aug 28–29, 2026 — Indy + two external reviews + build probes; not re-litigated at REVIEW):**

| Surface | Verdict | Decisive evidence |
|---|---|---|
| Per-fleet HMAC | local, closed `SigningScheme` enum over `HmacSha256Tag` | no crate owns "a table of webhook schemes"; a config DSL is the failure mode |
| Svix | **vendor** upstream verifier (`afd_auth/src/vendor/svix_verify.rs`, MIT, pinned SHA, patch list) | `svix = { default-features = false }` does not compile (5 errors in its own `connector.rs`, probed); full crate drags the API client. Upstream has shipped a signature-bypass fix, so lineage over re-derivation. **The Zig daemon is the behavioural oracle, not svix 2.1.0** — upstream also accepts `webhook-*` fallback headers and bare secrets; where upstream and Zig disagree, Zig wins and the delta is recorded |
| Slack v0 | local over `HmacSha256Tag` | scheme is stable and published; the SDK's verifier feature is unreachable without ~22 mandatory crates incl. `ctrlc` |
| QStash verify | `jsonwebtoken` 10.4 (`aws_lc_rs`, in the lock) | explicit `Validation`: `validate_aud = false` (v10.4 default `true` rejects any future `aud` claim — source-verified), `validate_nbf = true`, `leeway = 0`, iss/sub pinned, `jti`+`body` as non-optional claim fields |
| QStash CRUD | local `reqwest` 0.13 adapter | both existing SDK crates unmaintained (39 and 21 recent downloads) and force jsonwebtoken 9 / reqwest 0.12-ring / sha2 0.10; typed outcome classification is the deliverable |
| Cron validation | `philiprehberger-cron-parser` 0.3.0 + parity guard | zero-dependency, std-only (probed). Differential test vs the Zig grammar: 16 agree, 5 disagree — guard rejects the five (names/macros, `*/61` step-over-span, reversed ranges; the last two are crate bugs, reported upstream) |
| Outbound retry | `backon` (in the lock, proven in `afd_fleet_lifecycle::install`) | jitter is a recorded improvement over the Zig's flat `200ms << attempt` |
| GitHub payload parsing | **replaced** — `octocrab::models::webhook_events` | `WebhookEvent::try_from_header_and_body` owns 67 typed payload variants plus `installation.id()` and `repository.full_name`. Deletes every JSON path-walk the Zig carries: `extractValue`/`extractString`/`extractOwnedScalar` and the `routing_key_path`/`repository_path` descriptor fields. Already a dependency for App JWT minting, so no new supply-chain surface |
| Per-fleet ingress lookup | **replaced** — `FleetConfig::stored` | `serve_webhook_lookup.zig` ran two statements walking `config_json` with `jsonb_array_elements` to read one trigger's fields. Rust has a typed reader on the claim path already, so `SELECT_FLEET_INGRESS` selects the column and stops; the "first webhook trigger" rule moved into `Binding::read`, where 4 tests reach it |
| App-ingress subscriber match | **replaced** — relational half in SQL, document half in `Binding` | `SELECT_APP_INGRESS_TARGETS` nested two `EXISTS` clauses over `jsonb_array_elements` to match repository and event. `SELECT_APP_SUBSCRIBERS` asks only workspace/status/grant, and `Binding::serves_repository` + `Binding::admits` answer the rest with tests. Cost recorded: candidate rows are no longer pre-filtered by repository, bounded by the same `MAX_FANOUT` ceiling the fan-out is |
| App-ingress dedup | **unified** — `Ingress::deliver` | The Zig hand-rolls `setNx` → `xadd` → `del`-on-failure per target, which has a release path that can be got wrong. One `append_once` Lua script replaces all three, and the claim is atomic with the append |
| App replay identity | **kept, with its reason** — SHA-256 of the body | `x-github-delivery` is not covered by the signature, so keying the claim on it would let a captured delivery be resent under a fresh id and wake every subscribed fleet again. `github.zig`'s `authenticatedReplayId`, and the name is the argument |
| App fan-out ceiling | **kept, with its reason** — `MAX_FANOUT = 100` | A spend bound, not a latency one: one signed request becoming a hundred fleet runs is a hundred model spends. Refused whole rather than truncated, because waking the first hundred of a hundred and one silently picks whose fleet runs |

The rustls invariant, stated precisely: exactly one `CryptoProvider` — `aws-lc-rs`; `rustls/ring` must not be enabled (feature-graph check, not a package grep — `ring` in the lock is resolvable, not compiled). `chrono` is already in the binary via `object_store`/`octocrab`/`posthog-rs`; domain civil-time stays `jiff`.

## Sections (implementation slices)

### §1 — The signature wall

Six verification paths, not the four the draft counted: `webhook_sig` (per-fleet provider HMAC), `webhook_hmac` (approval deliveries), `svix_signature` (vendored upstream verifier), the Slack v0 verdict (in-handler — its secret is a per-request vault read, not a boot secret), and the QStash JWT verifier (§3 consumes it; it lands here with the wall). All over the afd_crypto canon: constant-time compares, the three rejection codes (UZ-WH-020 / UZ-WH-010 / UZ-WH-011), the 5-minute timestamp window. `trusted_client_ip` ports in this section too but is not part of the wall — it verifies nothing; it is pure XFF/`Fly-Client-IP` derivation with an audit trail.

- **Dimension 1.1** — signature matrix per middleware: valid passes; wrong key, tampered body, missing header, malformed header each → the documented code → Tests `afd_webhook/tests/scheme_matrix.rs`: `a_correctly_signed_delivery_verifies_on_every_scheme`, `a_signature_under_the_wrong_key_is_refused_on_every_scheme`, `a_tampered_body_is_refused_on_every_scheme`, `an_absent_signature_header_is_refused_on_every_scheme`, `a_malformed_signature_header_is_refused_on_every_scheme` — DONE
- **Dimension 1.2** — timestamp window: 4m59s accepted, 5m01s → UZ-WH-011; skew in both directions → Tests `afd_webhook/tests/scheme_matrix.rs`: `only_the_timestamped_scheme_binds_a_window`, `a_timestamped_scheme_missing_its_timestamp_is_refused_as_stale`; `afd_webhook/tests/svix_vendor.rs`: `the_tolerance_window_is_five_minutes_in_both_directions`, `a_stale_delivery_is_refused_as_stale_not_as_a_bad_signature` — DONE
- **Dimension 1.3** — verification is constant-time. The structural assertion is on the TYPE, not on a timing measurement: `HmacSha256Tag` does not derive `PartialEq` (`afd_crypto/src/mac.rs:30`), so a short-circuiting `==` is unwritable and `verify`'s `subtle::ConstantTimeEq` is the only comparison route (RULE CTM, `docs/greptile-learnings/RULES.md:95`) → Test `afd_crypto/tests/mac.rs`: `the_tag_type_offers_no_short_circuiting_comparison` — DONE
- **Dimension 1.4** — unconfigured webhook secret → UZ-WH-020, never a verify attempt → Tests `afd_webhook/tests/scheme_matrix.rs`: `an_empty_secret_is_refused_as_unconfigured_before_any_comparison`, `a_refusal_carries_its_registry_code_and_a_stable_sentence`; `afd_webhook/tests/svix_vendor.rs`: `an_empty_secret_never_parses` — DONE

### §2 — Webhook ingress handlers

`/v1/webhooks/{fleet_id}` (+`/approval`, `/github`), `/v1/webhooks/svix/{fleet_id}`, `/v1/ingress/{provider}`, `/v1/auth/identity-events/clerk` — verified payloads become `XADD fleet:{id}:events` entries (entry id IS the event id), made at-most-once by the `append_once` Lua script's Redis claim key; the approval webhook resolves gates through the M178 approvals service.

**Correction (Aug 29).** This section previously described the idempotency boundary as `INSERT … ON CONFLICT DO NOTHING`. It is not, and the difference is load-bearing rather than cosmetic. Ingress writes **nothing** to Postgres: the durable row appears when the runner leases the event, and a daemon that inserted one at ingress would be racing its own runner to describe the same event. `INSERT_FLEET_EVENT` has exactly two callers — `afd_fleet::lease::event` and `afd_approval::inbox` — and neither is ingress. What makes a redelivery safe is `afd_redis::streams::OnceScope`'s claim key, checked and set in the same Lua script as the `XADD`, so the claim and the append cannot come apart. See `afd_ingress/src/deliver.rs`.

- **Dimension 2.1** — verified webhook → one stream entry and **no** Postgres row (the row is the runner's, at lease); replayed delivery → zero new entries and the FIRST entry's id returned, per the `append_once` claim → Tests `afd_api/tests/webhook_fleet_route.rs`: `a_signed_failed_run_wakes_the_fleet_and_answers_the_events_id`, `a_redelivery_repeats_the_first_claim_and_reports_that_it_did` (the claim half); `afd_events/tests/fleet_event_writers.rs`: `the_durable_event_row_has_exactly_two_writers_and_ingress_is_not_one` (the zero-rows half — a source scan, because no runtime assertion proves a negative about code that did not run) — DONE
- **Dimension 2.2** — approval webhook resolves the gate exactly as the M178 surface does (one continuation row) → Tests `afd_api/tests/webhook_approval_wall.rs` (10 tests — the wall: BOTH fail-closed paths, wrong key, tampered body, a proof bound to another instant, the stale replay, the window edge, and that no refusal reaches the store) and `afd_api/tests/integration_approval_webhook.rs` (5 — the resolution: an approval resolves the gate and lands one continuation, a redelivery lands no second one, a denial resolves and starts nothing, a gate of ANOTHER fleet is not resolvable through this fleet's path, and both doors leave the same row). The continuation is the assertion rather than the status code: a resolved gate with none is a run a person unblocked and nothing restarted, and the response says `resolved` either way. Mutation-checked twice — dropping the fleet filter resolves the other fleet's gate at 200, and suppressing the continuation reddens three of the five — DONE
- **Dimension 2.3** — GitHub-flavored payload parsing parity on a fixture corpus (deliveries the Zig daemon accepts/rejects) → Tests `afd_api_ingress/src/handler/webhook/github/tests.rs`: `a_failed_run_becomes_the_digest_a_fleet_reasons_over`, `a_green_run_is_dropped_rather_than_woken_on`, `a_run_on_the_repairers_own_branch_is_dropped_before_its_conclusion_is_read`, `an_in_progress_run_is_dropped_as_an_action_rather_than_a_conclusion`, `an_event_this_daemon_serves_no_rule_for_is_unsupported_not_malformed`, `a_body_that_is_not_the_event_its_header_claims_is_an_error`, `the_two_policies_differ_only_on_the_pull_request_action`, `both_policies_drop_a_pull_request_from_the_repairers_own_branch`, `an_opened_pull_request_carries_the_twelve_field_digest`, `a_delivery_shaped_as_github_sends_it_deserializes`, `an_actor_missing_one_url_field_fails_the_whole_delivery`, `the_run_digest_carries_nine_fields_and_no_more` — DONE
- **Dimension 2.4** — Clerk identity events mutate the same identity state as the Zig daemon on fixture events → PORTED. The route was unmounted (`AuthRoute::IdentityEventClerk => None`) and the whole provisioning path was absent: `INSERT INTO core.tenants` / `core.users` appeared nowhere in production Rust, only in test fixtures, so no new person could get an account. `afd_tenant::signup` opens the five rows in one transaction — tenant, user, owner membership, a named workspace, the starter wallet — idempotent on `oidc_subject` with the unique index as the arbiter under concurrency, and the wallet healed on replay without refilling a spent balance. `afd_api_ingress::handler::webhook::identity_route` verifies the Svix delivery and mounts through the Auth family's tenant-then-ingress fallthrough → Tests `afd_api/tests/identity_signup_route.rs` (11 — the wall, the event-type routing, and the two address refusals, all before any store) and `afd_tenant/tests/integration_signup.rs` (5 — every row read back, the replay, both halves of the wallet heal, and two concurrent deliveries opening exactly one account) — DONE
- **Dimension 2.5** — every route + method in this spec's Interfaces inventory exists in the Route enum; extras and gaps both fail → Tests `afd_api/tests/admin_operator_route_inventory.rs`: `test_route_inventory_matches_interfaces`; `afd_api/tests/route_inventory.rs` — DONE

### §3 — Schedules and QStash

Workspace+fleet `/schedules[/{schedule_id}[/sync]]` CRUD, the schedules store, the QStash client (create/update/delete upstream schedules), the sync service (`/sync` reconciles), and the fire path `/v1/ingress/qstash/schedules`: signature verified at ingress, replay suppressed atomically, the daemon owns no timer.

- **Dimension 3.1** — schedule CRUD + `/sync` reconciliation parity (store rows + upstream calls recorded against a QStash fake) → Tests `afd_cron/tests/integration_store.rs` (14 tests — create, the three bounds, fleet-scoped reads, the fire target) and `afd_api/tests/integration_fleet_schedules.rs` (the HTTP surface, including that a create answers 201 with a sync state saying upstream does not know). The RECONCILIATION against a real upstream is `afd_cron/tests/integration_sync.rs` (4 tests, against the compose `qstash` dev server): the push is accepted, the scheduler's own key is ADOPTED onto the row and a delete names it, an unreachable scheduler keeps its reason, and a syncer that lost its fence writes nothing back — DONE
- **Dimension 3.2** — schedule fire: verified callback → event append; duplicate fire (same delivery) suppressed atomically under concurrency → Tests `afd_cron/tests/integration_fire.rs` (6 tests — the retry claimed by the first attempt, two daemons racing one retry appending once, and the three ways a claim must NOT collide: another schedule, another tick, another fleet) — DONE
- **Dimension 3.3** — QStash outage during CRUD → typed retryable error; store and upstream never diverge silently (sync repairs) → Tests `afd_cron/tests/error_surface.rs`: `only_the_kinds_a_retry_could_fix_are_retryable`, `a_refusal_and_an_outage_are_never_the_same_answer`, `a_vendor_status_never_reaches_the_caller` (the typed-error half: a vendor that answered is not retryable, one never reached is, and neither leaks its status to the caller); `afd_cron/tests/integration_fence.rs`: `a_failed_push_keeps_its_reason_and_stays_retryable` (the reason survives on the row and the next claim takes it). The REPAIR is `afd_cron/tests/integration_sync.rs`: `a_scheduler_that_cannot_be_reached_keeps_its_reason_on_the_row` proves the row records why and leaks no transport target, and the adoption case proves store and upstream stop diverging on the key — DONE

### §4 — Connectors and Slack

`/v1/connectors/{provider}/callback` (GET relay / POST complete) finishing OAuth (Open Authorization) grants into the vault via M176 crypto; `/v1/connectors/slack/events` (URL-verification challenge + event deliveries); workspace `/connectors[/{provider}[/connect]]` surface parity.

- **Dimension 4.1** — callback relay/complete: grant lands in the vault under the provider key name; states/nonces validated → Tests `afd_api/tests/connector_callback_route.rs` (10 tests) cover the browser leg WHOLE — it touches no store — and the dashboard leg up to its first vault read. The grant landing under the provider key name is past that line, and is now `afd_api/tests/integration_connector_callback.rs` (3 tests, live Postgres + Redis + a loopback token endpoint): a completed connect seals the handle under `Provider::grant_key()` and under no second name, a replay is refused without redeeming the code again, and a reconnect REPLACES the sealed grant rather than refusing a taken name. The token exchange reaches a real socket through `Exchange::pointed_at` — the seam the crate already carried — so the daemon does its own reading of the vendor's JSON rather than a stub handing back a parsed grant — DONE
- **Dimension 4.2** — forged/expired callback state → rejected, no vault write → Tests `afd_api/tests/connector_callback_route.rs` for the refusals reached BEFORE the vault (unshipped provider, no state, no code, an undecodable query, a missing bearer, an insufficient scope). A state that is forged, expired or another person's is refused by `Connectors::verify`, proved directly in `afd_connector/tests/connect_verify.rs` (7 tests) over stores that are not there — including the one that matters most: an authenticated BYSTANDER presenting a genuine, unexpired state earns `ForeignSubject` and not acceptance. Mutation-checked by deleting the subject check, which every `state` test survives. Only the SPENT slot needed a live Redis, and it is `integration_connector_callback.rs`'s `a_replayed_callback_is_refused_without_redeeming_the_code_again`. The proof of single use is the token endpoint's REQUEST COUNT, not the vault: a second redemption seals a byte-identical grant, so the vault cannot tell a replay from the first connect. Mutation-checked by making `spend` answer `Some` unconditionally, which lets the replay through to a 302 — DONE
- **Dimension 4.3** — Slack URL-verification answered; signed events accepted; bad Slack signature rejected → Tests `afd_api_ingress/src/handler/events.rs`: `test_the_handshake_echoes_the_value_it_asked_for`, `test_no_valueless_handshake_is_answered_as_one`, `test_a_provider_with_no_handshake_echoes_nothing` (the handshake half, as a unit); `afd_api/tests/connector_events_route.rs` (4 tests) for the route's pre-vault refusals — a connector with no inbound surface, BOTH cap bands, and that an unsigned handshake is never echoed. Signed-event acceptance reads the connector's app secret from the vault, and is `afd_api/tests/integration_connector_events.rs` (5 tests, live Postgres): a signed handshake is echoed from the secret the vault holds, one signed with the wrong secret reflects nothing, a bag that OPENS and carries no signing secret refuses `UZ-WH-020` rather than reading as a bad signature, and both acknowledgement cases count `core.fleet_events` either side. Mutation-checked by discarding the wall's refusal, which reddens two tests with the challenge visibly reflected in the output — the attack the echo's position behind the wall exists to prevent. The FM row is amended; see the Test Specification — DONE

### §5 — Outbound answer worker

The connector outbound queue worker as a supervised task: delivers fleet answers to connector destinations with jittered retry/backoff, failure accounting, and clean shutdown (joins on stop; in-flight delivery completes or re-queues).

Two recorded departures from the Zig, both improvements over workarounds it documents in its own comments: (1) the worker owns a **dedicated non-multiplexed Redis connection** (new seam in afd_redis, `hub.rs` precedent) and blocks on `XREADGROUP BLOCK` instead of the 250 ms idle poll — the poll existed because Zig's pooled connections could not park on a stream; `tokio::select!` races the blocking read against the supervisor's `CancellationToken`. Dropping the read future does not cancel the command server-side, so the pending-first read on resume is load-bearing, and Dimension 5.2 proves it. (2) The retry backoff is `backon`'s jittered schedule, where the Zig retried at a flat `200ms << attempt` — Dimension 5.1's "jittered" is this improvement, not parity. Delivery stays serial: two answers into one Slack thread must not reorder.

- **Dimension 5.1** — queued answer delivered once; destination 5xx → retry with backoff then documented terminal handling → Tests `afd_outbound/tests/integration_worker.rs`: `test_outbound_delivery_retry`; `afd_outbound/tests/delivery.rs`: `test_a_delivered_answer_is_offered_exactly_once`, `test_a_retryable_destination_is_offered_the_budget_and_no_more`, `test_a_destination_that_recovers_is_not_offered_again`, `test_a_permanent_refusal_is_offered_once` — DONE
- **Dimension 5.2** — shutdown mid-delivery: task joins; no lost or double-delivered answer → Tests `afd_outbound/tests/integration_worker.rs`: `test_outbound_shutdown_no_loss`; `afd_outbound/tests/delivery.rs`: `test_a_shutdown_stops_the_retry_without_abandoning_the_attempt`, `test_a_shutdown_during_a_successful_delivery_still_reports_it` — DONE

## Parallelization & execution map

(Internal batch labels here sequence THIS milestone's work only; the frontmatter **Batch:** line is the family-level ordering — two different axes, deliberately.)

| Batch | Scope | Runtime · model · reasoning tier | Why |
|---|---|---|---|
| B1 (first, alone) | §1 middlewares | Claude Code · Opus 5 · xhigh | the security wall; everything else consumes it |
| B2 | §2 webhooks | Claude Code · Opus 5 · high | idempotency + fixture corpus, well-oracled |
| B2 | §3 schedules/QStash | Claude Code · Opus 5 · high | reconciliation logic with a fake-upstream oracle |
| B2 | §4 connectors/Slack | Codex · GPT 5.6 tera · high | vendor flows with crisp fixtures |
| B3 | §5 outbound worker | Claude Code · Opus 5 · high | supervised-task semantics from the M176 pattern |

Indy decides how many agents actually spin per batch.

## Interfaces

```
Ingress routes (per src/agentsfleetd/http/route_template.zig):
  POST /v1/webhooks/{fleet_id}[/approval|/github] · /v1/webhooks/svix/{fleet_id}
  POST /v1/ingress/{provider} · /v1/ingress/qstash/schedules
  GET|POST /v1/connectors/{provider}/callback · POST /v1/connectors/slack/events
  POST /v1/auth/identity-events/clerk
  /v1/workspaces/{id}/fleets/{fleet_id}/schedules[/{schedule_id}[/sync]]
Rejection codes: UZ-WH-020 (misconfig) · UZ-WH-010 (bad signature) ·
UZ-WH-011 (stale timestamp, 5-minute window) — UNIFIED across this surface
(Indy's call, Aug 28 2026). The Zig answers three families here — the approval
webhook UZ-APPROVAL-003, Slack events UZ-SLK-010/011 — and the Rust collapses
them onto UZ-WH-*. A deliberate divergence, not parity: the error docs in
~/Projects/docs change with it, and M181's rollback note must say a reverted
Zig daemon answers the old codes.
Stream write: XADD fleet:{id}:events — entry id IS the canonical event id.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Forged signature | attacker or key rotation skew | constant-time verify → UZ-WH-010; no stream write, no payload logging |
| Stale delivery | provider retry after window | UZ-WH-011; provider retries per its policy |
| Unconfigured secret | misconfigured fleet | UZ-WH-020 before any verify work |
| Webhook replay | provider at-least-once delivery | idempotent insert + entry-id dedup → zero duplicate rows |
| Cron double-fire | QStash retry race | atomic suppression; exactly one event row |
| QStash outage | vendor down | typed retryable on CRUD; `/sync` reconciles when back |
| Slack retry storm | slow handler | fast-ack semantics parity; events processed idempotently |
| Connector destination down | outbound 5xx | jittered backoff retries; terminal handling parity + failure accounting |
| Forged OAuth callback | CSRF (cross-site request forgery)-style state reuse | state/nonce validation → rejected, no vault write |

## Invariants

1. Every signature compare on this surface is constant-time via the afd_crypto canon — one implementation, no per-handler crypto — `afd_crypto/tests/mac.rs`: `the_tag_type_offers_no_short_circuiting_comparison` (the type admits no other compare) + OWN review under RULE CTM.
2. Nothing writes `fleet:{id}:events` on this surface without a passed signature verdict — middleware ordering enforced by the Route metadata; `afd_webhook/tests/scheme_matrix.rs`.
3. The daemon owns no timer — schedule firing arrives only via verified QStash callbacks; enforced by afd_cron exposing no scheduler task; `afd_cron/tests/integration_fence.rs` (11 tests).
4. Replay is idempotent end-to-end (webhook + cron): duplicate deliveries produce zero additional durable rows — `afd_api/tests/webhook_fleet_route.rs`: `a_redelivery_repeats_the_first_claim_and_reports_that_it_did`, plus `afd_events/tests/fleet_event_writers.rs` bounding the writer set for the zero-rows half.
5. Rejected ingress logs code + provider only — never payload bytes, signatures, or secrets — LOGGING gate + log-capture assertions in the signature tests.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| ingress accept/reject counters (existing families) | ops | each verdict | provider, code, fleet id | no payloads/signatures | `afd_webhook/tests/scheme_matrix.rs` |
| outbound delivery outcome counter (existing) | ops | delivery attempt terminal state | provider, outcome, retry count | no message content | `afd_outbound/tests/integration_worker.rs`: `test_outbound_delivery_retry` |

No product-analytics changes (machine-facing ingress; parity port).

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit (negative-heavy) | `afd_webhook/tests/scheme_matrix.rs` (5 tests) | every scheme × {valid, wrong key, tampered body, absent header, malformed header} → documented verdict each |
| 1.2 | unit (negative) | `scheme_matrix.rs` + `svix_vendor.rs` (4 tests) | ±window-edge fixtures → accept/UZ-WH-011 exactly at the documented boundary; only the timestamped scheme binds a window |
| 1.3 | unit (structural) | `afd_crypto/tests/mac.rs`: `the_tag_type_offers_no_short_circuiting_comparison` | `HmacSha256Tag` does not implement `PartialEq`, so `verify`'s `subtle::ConstantTimeEq` is the only comparison route |
| 1.4 | unit (negative) | `scheme_matrix.rs` + `svix_vendor.rs` (3 tests) | no secret configured → UZ-WH-020 with its registry sentence, verify never invoked |
| 2.1 | integration (replay) + unit (structural) | `webhook_fleet_route.rs` (2 tests) + `afd_events/tests/fleet_event_writers.rs` | same delivery twice → one stream entry, the first entry's id returned; and `INSERT_FLEET_EVENT`'s caller set is exactly `afd_fleet` + `afd_approval`, never ingress (see the §2 Correction) |
| 2.2 | unit (wall) + integration (resolution) | `webhook_approval_wall.rs` (10 tests) + `integration_approval_webhook.rs` (5 tests) | every refusal happens before the store is touched; then an accepted delivery → gate resolved + exactly one continuation row, a redelivery adds none, a denial adds none, another fleet's gate refuses `UZ-APPROVAL-002` untouched, and the dashboard's bearer route leaves the same row |
| 2.3 | unit | `afd_api_ingress/src/handler/webhook/github/tests.rs` (12 tests) | fixture corpus → accept/drop verdicts equal the Zig daemon's; both policies; digest field counts pinned |
| 2.4 | unit (wall) + integration (provisioning) | `identity_signup_route.rs` (11) + `integration_signup.rs` (5) | every refusal happens before the store is touched; then a verified `user.created` opens five rows or none, a replay answers as the first did, and two concurrent deliveries open exactly one account |
| 2.5 | unit | `admin_operator_route_inventory.rs`, `route_inventory.rs` | Interfaces inventory ⊆ Route enum with methods; extras/gaps named |
| 3.1 | integration | `integration_store.rs` (14) + `integration_fleet_schedules.rs` (2) | create, bounds, fleet-scoped reads and the HTTP surface; `integration_sync.rs` (4 tests) carries the drifted-store-vs-upstream half |
| 3.2 | integration (race/replay) | `integration_fire.rs` (6) + `integration_fence.rs` (11) | concurrent duplicate fires → exactly one entry and both callers told the same id; and the single-syncer fence a push runs under |
| 3.3 | integration (negative) | `error_surface.rs` (9) + `integration_fence.rs` | upstream down during CRUD → typed retryable error that leaks no vendor status, its reason kept on the row and reclaimable; `integration_sync.rs` (4 tests) carries the repair |
| 4.1 | unit (relay) + integration (grant) | `connector_callback_route.rs` (10 tests) + `integration_connector_callback.rs` (3 tests) | the browser leg relays the whole handoff to the dashboard and verifies nothing; then connect→complete → the handle sealed under `Provider::grant_key()` and under no second name, and a reconnect replaces it rather than refusing |
| 4.2 | unit | `connector_callback_route.rs` (10) + `afd_connector/tests/connect_verify.rs` (7) | every route refusal before the secret read; and forged, foreign-secret, cross-connector, expired and BYSTANDER states each refused as their own reason. `integration_connector_callback.rs` carries the spent slot, proven by the token endpoint being asked exactly once rather than by the vault, which cannot distinguish a second redemption from the first |
| 4.3 | unit + integration | `events.rs` (6 tests) + `connector_events_route.rs` (4 tests) + `integration_connector_events.rs` (5 tests) | the handshake decision; no inbound surface refused before the vault; the two cap bands answer differently; an unsigned handshake is not echoed; then valid event accepted, bad signature rejected |
| 4.3 (FM) | integration (negative) | `integration_connector_events.rs` (2 of its 5) | fast-ack: a VERIFIED delivery this build serves no producer for is acknowledged 200 with its reason rather than 4xx'd into the sender's retry loop, and `core.fleet_events` is unchanged either side; a retried delivery earns a byte-identical answer and still leaves the count alone. **Amended from "ack returns before processing completes; retried delivery processed idempotently".** That wording describes `slack/events.zig` step 6 — `setNx` on `(channel_fleet_id, event.ts)` then XADD onto `fleet:{id}:events` — and this milestone ports no step 6: Out of Scope excludes the reactive bot surface, so the route ends at the ack and `grep -E 'deliver\|append\|XADD\|INSERT\|spawn\|queue'` over `events.rs` returns no code. With nothing processed, "processed idempotently" is a test that cannot fail (RULE TCF). The row now grades the fast-ack DECISION, which the port does keep, and the row count assertions go red the day a producer lands without a claim |
| 5.1 | unit + integration | `afd_outbound/tests/delivery.rs` (4 tests) + `integration_worker.rs`: `test_outbound_delivery_retry` | destination 5xx×N → jittered backoff budget + terminal handling |
| 5.2 | unit + integration | `afd_outbound/tests/delivery.rs` (2 tests) + `integration_worker.rs`: `test_outbound_shutdown_no_loss` | SIGTERM mid-delivery → join; answer delivered once or re-queued |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Signature wall holds (§1) | `cd rustd && cargo test signature` | exit 0 | P0 | |
| R2 | Ingress + replay parity (§2, §3) | `make test-integration-rustd` (ingress/cron subset rides the lane) | exit 0 | P0 | |
| R3 | Connector flows (§4, §5) | `cd rustd && cargo test connector` + `cargo test outbound` | exit 0 | P0 | |
| R4 | Route inventory parity for the ingress groups (§2) | `cd rustd && cargo test test_route_inventory_matches_interfaces` | exit 0 | P0 | |
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

**Credential gate (in scope):** this milestone adds the QStash signing keys, the Svix secret, the Slack signing secret, and the connector OAuth client credentials to the family enumeration — same fetch location (`~/.config/agentsfleet/` via `provision-env-1password`); the boot preflight extends to name them.

## Out of Scope

- Cutover, deploy shape, soak — M181.
- New providers, new webhook schemes, or window-width changes — parity only.
- Slack *bot* behaviour beyond event ingress (the reactive bot surface is its own product track).

---

## Product Clarity (authoring record)

1. **Successful user moment** — a deploy fails on GitHub, the webhook lands on staging `agentsfleetd-rs`, the fleet wakes, a scheduled run fires exactly once from QStash, and the connector answer arrives in Slack — the trigger plane is provably interchangeable.
2. **Preserved user behaviour** — providers keep their configured secrets and endpoints; every accept/reject verdict and retry interaction is unchanged.
3. **Optimal-way check** — middleware-first ordering is the direct path: the wall lands and everything behind it is ordinary handler porting.
4. **Rebuild-vs-iterate** — pure port; the one structural change (outbound worker on the M176 supervisor) preserves observable behaviour.
5. **What we build** — four middlewares, ingress handler groups, cron crate, connectors crate, outbound worker.
6. **What we do NOT build** — new event producers, signature-scheme changes, a daemon-owned timer.
7. **Fit with existing features** — feeds M178's event/approval surfaces; must not destabilize the six-producer ingress invariant (`data_flow.md` §B).
8. **Surface order** — N/A — machine-facing ingress; the schedules CRUD follows the existing API shape.
9. **Dashboard restraint** — no UI change.
10. **Confused-user next step** — a rejected webhook reports its documented UZ-WH-* code in the response and the provider's delivery log; existing troubleshooting docs stay accurate.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five slices with the signature wall isolated first and alone — every later slice consumes its verdict, so its correctness multiplies.
- **Alternatives considered:** running this concurrent with M178/M179 in Batch 4 (rejected: the approval webhook and schedules surface depend on M178's services; a three-way concurrent batch would put the two daemons' shared behaviour in review simultaneously across three PRs); re-implementing HMAC per middleware (rejected: one canon in afd_crypto, RULE OWN/UFS).
- **Patch-vs-refactor verdict:** this is a **refactor** (same behaviour, new runtime) with fixture-matrix discipline on the security wall.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **§1 shape (Aug 29):** verification cores land in `afd_auth` as pure functions with typed verdicts (the Zig `svix_verify.zig`/`slack_sig.zig` shape); the tower/axum shells wire in at §2 with the routes, matching `afd_api`'s Gate pattern — `afd_auth` stays framework-free, as it is today.
- **Test cadence (Indy, Aug 29):** no unit/integration lanes mid-milestone; `cargo fmt` + `cargo clippy` at each section boundary; one commit per completed section; the declared `verify.*` set runs once at CHORE(close).
- **No 1-to-1 Zig transliteration (Indy, Aug 29):** the Zig tree is the BEHAVIOURAL oracle, not a structural template. Where Rust has a crate or an idiom that does the job, it wins over a port of the Zig shape. Hand-rolling a primitive the workspace already carries is a defect, not a parity choice.
- **Cron grammar (Indy, Aug 29) — crate, not hand-roll.** `philiprehberger-cron-parser` 0.3.0 parses (zero dependencies, std-only). The guard on top is three checks for what it accepts and this daemon will not register: aliases and names (`@daily`, `MON`), a step wider than its field's span (`*/61`), and a reversed range (`5-2`). The last two are upstream bugs; Indy will fork and PR them, and the guard is deleted when they land. An earlier hand-rolled grammar was reverted.
- **Crate audit (Aug 29)** — four hand-rolled primitives found and all four replaced: `subtle::ConstantTimeEq` for a fold-XOR compare (RULE CTM); `url::Url` for a `?`/`#` substring search; `sqlx::FromRow` reading BY NAME for a positional fifteen-column decode (the order in `sql.rs` was load-bearing and silently so); `jiff::tz::TimeZone::get` for a zone-name shape check. The jiff change enables `tzdb-zoneinfo` workspace-wide — measured: zero extra crates, zero binary growth, and `debian:bookworm-slim` ships `/usr/share/zoneinfo` (verified in the image), so no Dockerfile change.
- **§4 shape (Aug 29):** a new `afd_connector` crate owns the OAuth state machine (`state` mint/redeem single-use with a TTL, `exchange` code→token per provider, `registry` for the five, `grant` sealing into the vault). The callback is two verbs on ONE path because OAuth splits across them — the browser arrives on GET and the dashboard finishes on POST. Slack's signing secret is a per-request vault read, not a boot secret, so its verification cannot ride the App-ingress path.
- **§5 shape (Aug 29):** the outbound worker is the RETURN leg — a fleet's answer reaching the connector destination the question came from. `afd_connector::outbound` as a supervised task over a dedicated non-multiplexed Redis connection, `tokio::select!` racing `XREADGROUP BLOCK` against the supervisor's cancellation token. Two recorded departures from the Zig, both improvements over workarounds it documents: the blocking read replaces a 250 ms idle poll that existed because Zig's pooled connections could not park on a stream, and `backon`'s jittered schedule replaces a flat `200ms << attempt`. Delivery stays serial — two answers into one Slack thread must not reorder.
- **Configuration (Indy, Aug 29) — TOML + env, sectioned.** A `Layered` [`EnvSource`] puts a TOML file underneath the process environment, env winning. Every existing knob keeps working and nothing downstream of `preflight::resolve` learns a file exists. Section-to-knob mapping is `[section] key` → `SECTION_KEY`, uppercased. Parser is `basic-toml` and NOT `toml`: measured, `toml` 0.9 pulls five crates including two `winnow` versions, where `basic-toml` pulls only `serde`, which the workspace already resolves — zero new transitive crates.
- **Configuration, part two (Indy, Aug 29) — the database pool knobs travel with it, and they are per-ENVIRONMENT.** M185 reshaped how `agentsfleetd` sizes its Postgres pool, and both knobs must be representable in the TOML file alongside every other one. The sectioned mapping already decided carries them with no new rule: `[database] pool_size` → `DATABASE_POOL_SIZE`, `[database] min_pool_size` → `DATABASE_MIN_POOL_SIZE`, and the role suffixes fall out mechanically — `pool_size_api` → `DATABASE_POOL_SIZE_API`, matching `read_knob`'s existing "role-suffixed name first, bare name second" lookup.

  | knob | default | what it is |
  |---|---|---|
  | `DATABASE_POOL_SIZE` | 50 | the ceiling |
  | `DATABASE_MIN_POOL_SIZE` | ceiling / 4 (= 12) | connections held open before any request arrives |

  **`min_pool_size` is not noise and must not be dropped as a tuning detail.** The old ceiling was 4, from `256 / 64` where the 64 was an asserted "request-sharing factor" nobody derived — visible in this worktree today at `afd_db/src/config.rs:27-29`. A pool that small sits permanently below its own ceiling, which `Db::acquire()` reads as evidence the datastore is ABSENT, so an ordinary burst answered 503 while Postgres was healthy. The floor is what keeps a connection handshake (147 ms typical, 337 ms under load) off the request path, and it only holds because the pool is now built with `connect_with` rather than `connect_lazy_with` — the floor is established before `Db::connect()` returns. A TOML mapping that carried the ceiling and not the floor would re-open the same hole one config file later.

  **Nothing to sequence — the mapping is knob-agnostic.** `[section] key` → `SECTION_KEY` projects whatever the file carries; the layer enumerates no knobs and does not care which exist, so `DATABASE_MIN_POOL_SIZE` resolves the moment M185 lands with no edit here. (For a reader wondering why this branch still shows the old shape — the ceiling computed as `256 / 64`, `connect_lazy_with` at `afd_db/src/pool.rs:99,140` — that is this branch trailing M185, not a dependency the TOML work has.)

  **The core_api precedent, corrected against the files rather than from memory.** The brief said "50 in local/testapi/devapi, 75 in apiprod". Read directly (`~/Projects/oss/core_api-develop/tools/config/*.toml`, `[db]` sections only): `local`, `devapi` and `testapi` carry **no** `pool_size` under `[db]` at all and take the code default of 40; only `apiprod` overrides it, at 75. The `50`s are the `[cache]` pool, a different section. The precedent is therefore stronger than stated and points the same way: **the code default is the everyday answer and one environment overrides where the hardware justifies it** — which is exactly what a `Layered` source with the environment above the file already gives, with no per-environment file needed for the common case.

  **The half of core_api's pattern this repository does NOT take.** Its `apiprod.toml` carries the database password as a plaintext literal beside `pool_size`. Ours cannot: a stored secret is a vault `key_name` reference resolved through `crypto_store.load()`, and that is a Hard Safety rule with no override. The TOML file carries SIZING and addressing; it never carries a credential. Worth naming here because the reference file is the thing a reader will open, and the two rules sit four lines apart in it.

  **A deploy decision, flagged rather than taken.** 75 connections against a stock 100-connection Postgres leaves room for one replica and nothing else. Raising the ceiling to 75 in any environment therefore travels with a `max_connections` raise on the server, which is an infrastructure change and an operator's call — not something a config default sets silently. Recorded so whoever writes `apiprod` values reads it first.

- **oauth2 crate — REFUSED (Indy, Aug 29), and the reason is the scope delimiter alone.** Probed rather than reasoned about. `oauth2` 5.0.0 with its documented default features adds five packages including a SECOND `reqwest` (0.12.28 beside our 0.13.4) and, decisively, turns on `rustls/ring` — feature-graph counts are `ring 0 → 5` where `aws-lc-rs` stays 5, which is two `CryptoProvider`s and the invariant this milestone's Prior-Art table states. That break is AVOIDABLE: `default-features = false` adds four packages (`oauth2`, `rand` 0.8.8, `rand_chacha`, `thiserror` 1.0.69), pulls no second `reqwest`, and leaves `ring` at 0. So the TLS objection is not what decides it.

  What decides it is `code.rs:149` and `helpers.rs:95`: the scope delimiter is a hard-coded space in BOTH directions — `scopes.join(" ")` building the consent URL, `space_delimited.split(' ')` reading the response. That is RFC 6749 and it is right for exactly ONE of the connectors here. Counted against the Zig oracle: Slack `app_mentions:read,chat:write,channels:history`, Zoho `Desk.organization.READ,Desk.basic.READ` and Linear `read,comments:create` all delimit with a COMMA; only Jira uses the standard's space. **Three of the four OAuth connectors deviate.** (An earlier draft of this entry said two of five and put Zoho in the space column — wrong, and corrected here; the error understated the case.) The response side is the dangerous half: Slack answers `"scope":"app_mentions:read,chat:write"`, the crate yields ONE `Scope` carrying a comma, and it lands in `core.connector_installs.scopes` as a single bogus `text[]` element. The row looks populated and nothing fails. Adopting the crate would mean routing around the one type it exists to provide, in both directions, for three connectors out of four.

  **What the finding CHANGED in our own code, because we were making the same mistake with a different value.** `grant::parse::slack` split the response on a `','` literal — correct today only because Slack is the one provider carrying an install row. The delimiter is now `Oauth2Flow::scope_delimiter`, provider data beside the scope list, read by both the request and the answer. Two guards hold it: `every_connectors_scope_list_uses_the_delimiter_it_declares` fails when a list carries the other delimiter and none of its own, and `only_atlassian_among_the_connectors_follows_the_standard_delimiter` pins the count at three. Both were mutation-checked — declaring Jira comma fails them with `` `jira` declares `,` and its scope list carries none `` and `left: 4, right: 3`.

  **What the repository does NOT have, named as a gap rather than left implied.** There is no captured vendor response anywhere in this tree. The delimiters rest on the Zig's constants, on the author having written spaces for Jira and commas for the other three in one sitting (a mixed table is a decision; a guess is uniform), on `jira/spec.zig` carrying `audience=api.atlassian.com` (provider-specific knowledge, not invention), and on `slack/callback.zig:90` naming its field `scope_csv` and splitting on `','`. That is a shipped integration's behaviour, not evidence in the tree. Closing it properly means a captured-response corpus under `tests/fixtures/`, the shape Dimension 2.3 already takes for GitHub deliveries. Not taken now; recorded as the one unproven assumption in this crate.

  Priced, so the refusal is a trade and not a preference: 1215 runtime lines today; the crate replaces at most 214 (`authorize_url`/`exchange_form` 50→15, the exchange POST and its status classification 82→35, the signed state 163→56, the refresh-triple parse 45→20) and forces back ~108 (an `HttpRequest` adapter for our own `reqwest`, `ExtraTokenFields` per provider, an `ok:false` pre-check, the delimiter workaround, `RequestTokenError` mapping, a per-callback client rebuild for Zoho's data centre). **Net −106 lines, 8.7%**, against 783 lines — 64% of the crate — the OAuth standard has no opinion about at all: the provider registry, the vault app-bag, the grant seal and install upsert, Jira's `accessible-resources` call, Zoho's data-centre table, the error registry.

  Two things ALSO found, both corrections to the case first put:
  - `rustd/Cargo.toml:660` sets `multiple_crate_versions = "allow"`, so a duplicate `thiserror` is not a gate failure. That objection was wrong.
  - The signed state is **not** buying a security property `oauth2::CsrfToken` lacks. `types.rs:562` shows `CsrfToken::new_random()` is 16 random bytes, base64url — an opaque bearer nonce whose binding lives in the store instead of in the token. With workspace + subject + expiry held in the Redis VALUE rather than in the token, an opaque token reaches the same unforgeability, the same starter binding, and the same single-use. Ours signs because `state.zig` signs and the format is sound, not because an opaque one would be weaker. Recorded so a later reader does not over-trust the HMAC.

- **Cutover parity is NOT an argument on this milestone (Indy, Aug 29).** An earlier draft of the entry above defended the signed-state wire format on the grounds that a connect started on the Zig daemon must complete on the Rust one during M181. Struck: the product is not in production, so there are no in-flight connects to preserve and no operator to inconvenience. Wire-format decisions on this port stand on their own merits or not at all — and where a Zig format is kept, the reason to record is that the format is sound, never that a cutover depends on it.

- **The connect flow's Redis dependency, and what a loss actually costs (Indy asked, Aug 29).** Traced rather than assumed. `start` makes exactly ONE write and it is the Redis nonce; every Postgres write — the sealed grant, the `connector_installs` row — happens in `finish`, AFTER the nonce is spent. So: Redis *unreachable* is not a connect-specific concern, because `router/probes.rs:63` is `database && queue` and the instance leaves rotation before a callback can arrive. Redis *data* loss — eviction, a lagged replica, a restart without persistence — fails CLOSED: `DEL` answers 0, the callback answers `UZ-CONN-002`, and there is **no partial state to repair**, because nothing durable had been written. The person starts the connect again. The nonce is also not the only replay gate: completion requires a state this deployment signed, inside its window, presented by the person whose subject tag it carries, who is authorised on that workspace, plus an authorisation code the vendor has not already spent. If the claim should survive Redis entirely, the move is Postgres — the callback is already there for the workspace check — at the cost of a table, a migration behind the Schema Removal Guard, and an expiry sweeper. Not taken now; recorded as priced.

- **Dimension test naming — the spec's names lose, and the reason is measured (Indy, Aug 30).** This spec contracted seventeen `test_`-prefixed names. Fourteen never existed under those names, and `afd_api` — where most of them belong — writes tests as behaviour sentences without the prefix. `docs/architecture/testing.md` legislates FILENAMES (`integration_<subject>.rs` / `<subject>.rs` / `<crate>_suite.rs`) and says nothing about function names, so there was no written standard to appeal to. Measured instead, by the day each test file was introduced: 2026-08-24 through 08-26 are 100% prefixed (378 tests), and from 08-27 the practice inverts — 14%, 0%, 31%, 9% (411 tests). Per crate the split is the same story: the 100% crates (`afd_auth`, `afd_db`, `afd_identity`, `afd_redis`, `afd_state`) all predate the 27th, while `afd_api` sits at 20%, `afd_cron` and `afd_webhook` at 0%. It is also the idiomatic Rust position — `#[test]` performs discovery, so a `test_` prefix is a carryover from runners that discover by name and cargo already prints the module path. **Decision: amend each Dimension to name the test that actually proves it.** Where the real test is better factored than the spec's guess, the spec was the thing that was wrong. `afd_outbound` and `afd_crypto` keep their prefixes — they are pre-27th crates and internally consistent; this is not a rename campaign.

  > Indy (2026-08-30, AskUserQuestion): selected **"Amend the Dimensions"** — "Each Dimension names the test that actually proves it. The spec's names were guessed before the tests existed; afd_api's convention stays intact." — context: the seventeen contracted test names vs. the practiced convention.

- **TOML configuration — DEFERRED to a follow-up (Indy, Aug 30).** The `Layered` TOML-under-env design is fully specified in this log (the three Configuration entries above) and none of it is built: `basic-toml` appears in no `Cargo.toml` in this workspace, there is no Section carrying it, and no Dimension grades it. It has sat unanswered across three checkpoints. Nothing in §1–§5 depends on it — `preflight::resolve` reads the process environment today and every knob works. **Decision: it leaves M180 as a named follow-up rather than growing a §6 on a milestone already 6.67 points under its coverage floor.** The design entries above stay in this log deliberately: they are the follow-up's input, and re-deriving the `basic-toml`-over-`toml` measurement, the section-to-knob mapping, and the `min_pool_size` hazard would be the expensive half.

  > Indy (2026-08-30, AskUserQuestion): selected **"Move to a follow-up"** — "M180 closes without it; the Discovery entry becomes a named follow-up item. Nothing in §1–§5 depends on it." — context: TOML support, unanswered across three checkpoints.

- **Docs gate — a docs change AND an override, and the first framing of this was wrong (Indy, Aug 30).** The question was first put as a template generalisation: `/v1/connectors/slack/events` → `/v1/connectors/{provider}/events`, same concrete URL, therefore nothing a reader could observe. That is true of the connector-events route and **false of the rest of the diff**. Measured against `origin/main`, `public/openapi.json` changes three CONCRETE published paths:

  | was | is |
  |---|---|
  | `/v1/workspaces/{workspace_id}/approvals/{gate_id}:approve` | `…/{gate_id}/approve` |
  | `/v1/workspaces/{workspace_id}/approvals/{gate_id}:deny` | `…/{gate_id}/deny` |
  | `…/fleets/{fleet_id}/schedules/{schedule_id}:sync` | `…/{schedule_id}/sync` |

  All three are published today at `~/Projects/docs/docs.json:239,240,286`. The approvals pair is **M178's surface**, not this milestone's — the router constraint reached a route M180 does not own, which is why it escaped the first reading of the blast radius.

  **The constraint is real and already argued in code.** `matchit` refuses any literal after a parameter inside one segment (`tree.rs:783`, "Prefixes after route parameters are not supported"), so a custom verb cannot be part of a route pattern at all; see `afd_http/src/route/fleet.rs:96` and `afd_http/src/route/workspace.rs:118`. The rejected alternative — capture the whole leaf and strip the suffix in the handler — works, but takes the verb out of the route table, and that table is what `route_inventory.rs` grades.

  **Decision: update `docs.json`, and keep the override for the connector-events template alone**, where "no observable change" is actually true.

  **Sequencing, discovered by the docs repository's own gate rather than reasoned about.** The commit was written and REFUSED by `_lint-openapi-drift`, which checks every path `docs.json` references against `public/openapi.json` on agentsfleet **main**. The three new paths do not exist there until M180 merges, so the docs commit cannot precede the code merge — the gate enforces code-first, correctly. The branch `chore/m180-ingress-cron-connectors-changelog` is created and parked at `origin/main`; the edit is a **LAND-time step**, not a pre-PR one:

  ```
  cd ~/Projects/docs && git checkout chore/m180-ingress-cron-connectors-changelog
  sed -i '' -E 's|(approvals/\{gate_id\}):approve|\1/approve|; s|(approvals/\{gate_id\}):deny|\1/deny|; s|(schedules/\{schedule_id\}):sync|\1/sync|' docs.json
  ```

  (Unrelated, found while checking that repository's state and recorded so it is not lost: `chore/m172-read-path-changelog` there carries **one unpushed commit**, unmerged.)

  > Indy (2026-08-30, AskUserQuestion): selected **"Update docs.json, keep override for events"** — "Three lines on a `chore/m180-*-changelog` branch off main. The override then covers only the connector-events template, where its premise is actually true." — context: three concrete published URLs change shape, not just the events template.

- **A published endpoint the port dropped, and the gate that would have caught it (Indy, Aug 30).** `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules/{schedule_id}` is declared in `public/openapi.json`, published at `~/Projects/docs/docs.json:283`, and answered **405**. The Zig serves it (`src/agentsfleetd/http/handlers/schedules/api.zig:68` — `.GET => getSchedule`), and the Rust service seam already carried `FleetSchedules::one`; only the handler and its mount were missing, so §3's "CRUD" was missing its R. Now implemented at `afd_api_tenant/src/handler/schedule.rs::one` and mounted beside `patch` and `delete`.

  **Why nothing failed.** Every route test in this repository reads the route TABLE, and the table declared `Scopes::rw(SCHEDULE_READ, SCHEDULE_WRITE)` — a read rung the router never mounted. `route_inventory.rs` grades the path ROSTER, not the verbs. The gate that did grade served-versus-documented, `scripts/check_openapi_route_coverage.py`, was deleted in `ee83ba80e` with the rest of the `check-openapi` family; its `ROUTE_COVERAGE_TESTS` variable is still sitting unreferenced at `make/quality.mk:60`. M181_001 §1 records the resulting state in as many words: *"`public/openapi.json` is now a committed static artifact with nothing generating or grading it. Until the Rust daemon emits its own document, the served-vs-documented direction is unguarded on both daemons."*

  **A replacement was built here and then withdrawn, deliberately.** An in-repo `openapi_route_coverage.rs` walked every published operation against the live router. Two things came out of it and both are worth keeping even though the file is not:

  1. Its first draft was a **false green**. Probing unauthenticated, an undeclared `PUT` and an unmounted `GET` both answer `401`, because the auth layer runs before axum dispatches on method — measured, not assumed (an unknown path answers `404`, a guarded path answers `401` either way). The working version presented a credential and probed each path first with a verb the contract does NOT declare, treating that `405` as proof the path can distinguish methods at all. Any successor gate needs that control, or it will pass with the endpoint missing.
  2. With the control in place it found a **second** gap: `DELETE /v1/fleets/runners/{id}` is published and mounted nowhere — and the Zig never declared it either (`route_template.zig` carries only `fleet_runner_get` and `fleet_runner_patch`). The contract over-declares an operation neither daemon has ever served.

  **Decision: the file goes, and M181_002 owns the gate.** Its R1 is this check done properly — `agentsfleetd routes --json` against `agentsfleetd openapi`, graded by `scripts/check_route_coverage.py` — and its Dimension 1.3 makes the committed artifact the daemon's own output, at which point the spurious runner `DELETE` disappears without anyone editing JSON. Hand-fixing it now would be undone by that regeneration.

  **The window this leaves, stated rather than implied.** Between M180 merging and M181_002 landing, nothing grades the daemon against the published contract. `route_inventory.rs` covers paths and not verbs; the docs repository's `_lint-openapi-drift` compares `docs.json` to `openapi.json` and never reaches the daemon. **And regeneration is not neutral in that window:** it makes the document match the daemon, so a verb the daemon is missing is erased from the contract rather than reported. The schedules `GET` is fixed here precisely so that cannot happen to it — anything else found before M181_002 needs the same treatment, in the daemon and not in the JSON.

  > Indy (2026-08-30, AskUserQuestion): selected **"Drop it — M181_002 owns this"** — "Keep the GET fix, delete the guard file, and record both findings in the spec's Discovery as named input to M181_002 so regeneration doesn't launder the schedules GET." — context: an in-repo openapi-vs-router guard duplicating M181_002's R1.

- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
