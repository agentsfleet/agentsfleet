<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M218_001: Shared crates and locked dependencies replace duplicated and hand-rolled Rust

**Prototype:** v2.0.0
**Milestone:** M218
**Workstream:** 001
**Date:** Oct 08, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — Rust workspace hygiene; routes, wire bytes and error codes do not change
**Categories:** API (the Rust daemon and runner crates), DOCS (the error standard and the crate-ownership record)
**Batch:** B1 — starts once M213 merges; §1 precedes every other Section
**Branch:** feat/m218-shared-crates
**Baseline revision:** 6e4a02127b4738f205c6ba1c8d42bce42413ed2e
**Test Baseline:** unit=4614 integration=5438 — Rust unit 4614 passed, 0 failed, 935 ignored (201 binaries); Rust integration 5438 passed, 0 failed (256 binaries), from Continuous Integration (CI) on PR #739 head `a5ef046ee`, whose tree is the comparison revision's; the TypeScript lanes are in the evidence report
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M218_001-6e4a02127.md`
**Depends on:** M213_001 (Rust runner cutover, Zig retired; one Pull Request (PR) with M213_002). It adds `Coded::told()` and `Coded::logged()` (`afd_core/src/error_code.rs:70,75` on its branch), the runner code this audit sweeps, and the egress slot bitmap §5 restructures. M217_001 (tests call code) is independent; both edit `rustd/crates/afd_dragonfly/src/test_util.rs`, so whichever merges second re-locates its rows.
**Provenance:** LLM-drafted (large language model Claude Opus 5.5, Oct 08, 2026) from the M213 refactor survey and grep evidence on `main` at `9b6736e47` and `feat/m213-rust-runner-cutover` at `f9ab1450f`
**Canonical architecture:** `docs/architecture/rust-ownership.md` §Review rubric, §Ownership decisions

## Overview

**Goal (testable):** Every verified row of the duplication ledger across the 54 `rustd/crates/` crates lands on its named owner, the family greps in rubric R2 and R3 return nothing, and `make lint-all`, `make test-unit-all` and `make test-integration-rustd` pass.
**Problem:** A fix to a shared mechanism lands in one copy and misses the rest. A direct HTTP client (timeout, no proxy) is built four times, three in `afd_identity` and once in `agentsfleetd`, and the `agentsfleetd` copy (`credentials.rs:106`) falls back to a proxy-honouring default client when the build fails. Three files hand-render hex while `afd_auth/src/directory.rs:78` records why `hex::encode` replaced exactly that loop. `afd_bench` re-implements `afd_core::clock::now()`. The error standard says every crate is accounted for; 10 of the 54 are missing from its table. The M213 survey read only the 649 files M213 touched, and the September audit in `rust-ownership.md` covered 36 crates; the workspace has 54.
**Solution summary:** §1 audits all 54 crates in six groups, each with a finder agent and an independent adversarial verifier, plus a cross-group family pass; only verified findings enter `docs/v2/reviews/m218-rust-duplication-ledger.md`, scored on `rust-ownership.md`'s rubric. §2–§5 land the seeded families, highest yield first; §6 lands the remaining rows; §7 makes the families Clippy can see fail `make lint-rustd`, once Indy approves. `rust-ownership.md` records each new owner and `RUST_ERROR_STANDARD.md` covers every crate.

## PR Intent & comprehension handshake

- **PR title (eventual):** refactor(rustd): move duplicated and hand-rolled code onto shared crates
- **Intent (one sentence):** A fix to a shared mechanism lands once, in the crate that owns it, and every caller gets it.
- **Handshake** — performed at PLAN, Oct 10, 2026. Restated: each mechanism the workspace copies (a log reason, a URL guard, a hex render, a charset bound, a clock, an environment read, a retry loop, a direct HTTP client, the slot bitmap) gets one owner, and a fix there reaches every caller. `ASSUMPTIONS I'M MAKING:` (1) a row that changes what any input does is refuted with that input, except the two changes Interfaces names and the defect fixes Discovery lists; (2) no tier (c) crate; (3) §7 lands last and waits on Indy's approval in review. Matches the Intent.

## Implementing agent — read these first

1. `docs/architecture/rust-ownership.md` — the rubric every finding is scored on (Consequence, Replacement fit, Ownership burden) and the September 2026 ownership decisions this audit does not reopen.
2. `docs/RUST_ERROR_STANDARD.md` — `error_shell!`/`error_lifts!`, the property test for which crates take the hull, and the conformance table §2 completes.
3. The M217_001 ledger, `docs/v2/reviews/m217-test-audit-findings-2026-10-08.md` on `feat/m217-tests-call-code` (not yet on `main`) — the ledger shape this milestone mirrors: provenance, reconciliation, one row per site, verifier rejections kept.
4. `rustd/crates/afd_validate/src/rules.rs` — `charset`, `nul_free`, `ascii_digits`, `finite`: what garde lacks, and what a hand-written bound becomes.
5. `rustd/crates/afr_providers/src/retry.rs` — `backon` with `.when`, `.adjust` and `.notify`: the shape a hand-written retry loop becomes.
6. `.orly/dispatch/write_rust.md` — Rust error discipline (ERR-RS), functional design (FN-RS), constants (UFS), and the reference-guideline read every Rust review cites.

## Files Changed (blast radius)

The ledger names each site by row; this table names the roles.

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M218_001_P2_API_DOCS_SHARED_CRATES_REPLACE_HANDROLLED_RUST.md` → `active/` → `done/` | MOVE / EDIT | Lifecycle, Dimension status, Discovery |
| `docs/v2/reviews/m218-rust-duplication-ledger.md` | CREATE | §1: coverage table, verified rows, refuted rows with reasons, dependency proposals |
| `rustd/crates/*/src/**`, `rustd/crates/*/tests/**` named by ledger rows | EDIT | Each row's replacement and the tests that prove it |
| `rustd/crates/afd_core/src/error_code.rs`, `rustd/crates/agentsfleetd/src/error.rs`, `rustd/crates/agentsfleetd/src/credentials.rs` | EDIT | §2 hull and log fields; §4 shared client |
| `rustd/crates/afd_identity/src/jwks/http.rs`, `metadata.rs`, `provider.rs` | EDIT | §4 one direct client |
| `rustd/crates/afd_connector/src/endpoint.rs`, `afd_connector/src/state/nonce.rs`, `afd_vault/src/projection.rs`, `afd_db/src/test_util.rs`, `afd_core/src/etag.rs`, `afd_crypto/src/mac.rs` | EDIT | §3 `url` and `hex` |
| `rustd/crates/afd_fleet_runtime/src/name.rs`, `afd_tenant/src/workspace/name.rs`, `afd_cron/src/verifier.rs`, `afd_library/src/validate.rs` | EDIT | §3 bounds on garde and `afd_validate` |
| `rustd/crates/afd_bench/src/lane/lease/drive.rs`, `afd_bench/src/fixture.rs`, `afd_bench/src/cli.rs`, `afd_webhook/src/freshness.rs` | EDIT | §3 clock and environment |
| `rustd/crates/afd_db/src/migrate/lock.rs`, `afd_dragonfly/src/hub/repair.rs`, `afd_dragonfly/src/test_util.rs`, `afd_tenant/src/workspace/directory.rs` | EDIT | §4 retries |
| `rustd/crates/afr_sandbox/src/egress/slot.rs`, `egress/kernel.rs`, `egress/testing.rs`, `egress/slot_tests.rs`, `egress/scope_tests.rs`, `egress/tests.rs` | EDIT | §5 |
| `rustd/Cargo.toml`, `rustd/Cargo.lock`, `rustd/crates/*/Cargo.toml` | EDIT | Dependencies a row moves onto; a new-to-graph crate only after Indy approves |
| `rustd/clippy.toml` | CREATE | §7, after Indy approves |
| `docs/RUST_ERROR_STANDARD.md`, `docs/architecture/rust-ownership.md` | EDIT | Every crate in the table; each new owner and each refuted replacement recorded |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — Use standard parsers, never hand-roll (PSR): the URL and hex rows. Error returns never silently substitute defaults (ESO): `credentials.rs:106`. Distinguish error classes (ECL): a retry keeps its classifier. Narrow types at parse boundaries (NTP). No Dead Code (NDC) and no helper without a consumer (HLP): each replaced copy goes in the commit that replaces it. No Legacy Retained (NLR). Orphan sweep (ORP) and its close-out gate (CHR). Literals become named constants (UFS). Fixtures use production constants (TFX).
- `.orly/dispatch/write_rust.md` — ERR-RS, FN-RS and UFS fire on every `*.rs` edit; reviews cite Pragmatic Rust guideline identifiers, at least `M-SIMPLE-ABSTRACTIONS`, `M-MOCKABLE-SYSCALLS`, `M-AVOID-STATICS`, `M-STRONG-TYPES-GUARD`, `M-LINT-OVERRIDE-EXPECT`.
- `docs/RUST_ERROR_STANDARD.md` — no hand-written error type; a second type needs a caller that discriminates on it.
- `.orly/docs/LOGGING_STANDARD.md` §8A — hoisted locals and literal `tracing::` emits; event names keep their bytes (EVENT-COMPAT).
- `docs/architecture/rust-ownership.md` §Review rubric — keep domain policy whose replacement has no demonstrated benefit.
- `.orly/dispatch/verify.md`, `.orly/dispatch/lifecycle.md` — package runners never satisfy a lane claim.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | Yes | Replacements shrink files; a file a shared helper pushes past 350 lines splits by concern |
| UFS / LOGGING / RUST ERR | Yes | `Logged` destructures into locals the `tracing::` call reads; no logging macro, because `.orly/audits/logging.sh` reads literal emits |
| MILESTONE-ID | Yes | No milestone identifiers in code, test names or Clippy reasons |
| Harness and lint edits (`.orly/AGENTS.md` §Hard Safety) | Yes, §7 | `rustd/clippy.toml` waits for Indy at PLAN; `make/*.mk`, `.orly/` and `.github/` stay untouched |
| SCHEMA / user interface (UI) / DESIGN TOKEN / ZIG | No | No schema, interface or Zig files |
| SPEC TEMPLATE | Yes | `bash .orly/audits/spec-template.sh --staged` on every spec edit |

## Prior-Art / Reference Implementations

- **Decision record:** `docs/architecture/rust-ownership.md` — the September 2026 audit's rubric and owners; this milestone appends rows to it.
- **Audit-then-fix:** M217_001 and its ledger — finder plus second reviewer, rows located by `file:line`, rejected findings kept.
- **Per family:** `afd_auth/src/directory.rs:78` (`hex::encode` over a `write!` loop); `afd_fleet_runtime/src/config/trigger.rs:73` (`custom(charset(..))` on the type); `afd_connector/src/callback.rs:106` (`Url::parse` in the crate that also splits on `"://"`); `afr_providers/src/retry.rs:109` (`backon`).
- **Reference checkouts:** `~/Projects/oss/rs/arrow-rs` for workspace structure and `~/Projects/oss/rs/builder/components/builder-core/` for cause-preserving parsing; `~/Projects/oss/rs/exonum` is not checked out, so search `~/Projects/oss/rs/` and ask before cloning.

## Sections (implementation slices)

### §1 — Audit: every crate swept, every finding verified — DONE

Preparation Section; its output is the ledger. **Groups:** G1 shared (`afd_core`, `afd_wire`, `afd_validate`, `afd_http`, `afd_db`, `afd_crypto`, `afd_observability`, `afd_otlp`, `afd_dragonfly`); G2 HTTP planes and identity (`afd_api`, `afd_api_ingress`, `afd_api_operator`, `afd_api_runner`, `afd_api_tenant`, `afd_api_wire`, `afd_auth`, `afd_identity`, `afd_sse`, `afd_state`, `afd_webhook`); G3 tenant and credentials (`afd_admin`, `afd_tenant`, `afd_credential`, `afd_vault`, `afd_connector`, `afd_library`, `afd_mail`, `afd_billing`); G4 event pipeline (`afd_admission`, `afd_approval`, `afd_cron`, `afd_events`, `afd_gate`, `afd_ingress`, `afd_memory`, `afd_outbound`, `afd_runner`); G5 fleet and daemon (`afd_fleet`, `afd_fleet_lifecycle`, `afd_fleet_ops`, `afd_fleet_runtime`, `afd_bench`, `agentsfleetd`); G6 runner (`afr_*`, `agentsfleet_runner`).

Per group, a finder agent reads every `src/` file and proposes rows. A separate verifier agent sees the rows and the source, never the finder's reasoning, and tries to refute each: the replacement differs on some input, error or edge; the new edge breaks a dependency rule (`afd_wire` carries no daemon crate, `afd_core` keeps minimal dependencies); or a prior decision covers it. Only confirmed rows count. A cross-group pass then checks the seeded families below and every private function name defined in three or more crates. **Owner order:** (a) a shared workspace crate (`afd_core`, then `afd_wire`, `afd_validate`, `afd_http`, `afd_db`, `afd_crypto`, `afd_observability`, `afd_otlp`), the lowest one every caller already depends on; (b) a crate already in `rustd/Cargo.lock`, a direct dependency before a transitive one; (c) a new crate.

**Bar for a new crate (tier c), all seven:** it replaces two or more hand-written copies, or one that parses untrusted input; licence MIT or Apache-2.0; a release within the last 12 months and no open RustSec advisory; builds on `rust-toolchain.toml` with `default-features = false` and only the features used; adds no second version of a locked crate (the `multiple_crate_versions` allowance covers `syn` only); removes more lines than it adds, counting its `[workspace.dependencies]` note; Indy approves it at PLAN, with every package it adds to `Cargo.lock` listed in its row. A crate locked only transitively meets the last three as well. **Prior decisions, not reopened without new evidence the verifier confirms:** `ipnet` for `afd_core/src/net.rs` (transitive only; `afd_core` stays minimal), `itertools`/`indexmap` for first-seen dedup (a one-line `HashSet` filter), `rtnetlink`/`netlink-proto` (unlocked and async), and every row of `rust-ownership.md` §Ownership decisions. **Seeded families, confirmed by grep at authoring:** LOG, ERR, URL, HEX, BOUND, CLOCK, ENV, RETRY, HTTP (§2–§4 cite sites) and SLOT (§5). **Required human decision:** Indy approves the ledger's Section assignments and each tier (c) crate at PLAN for §2.

- **Dimension 1.1** — DONE — The coverage table names all 54 crates, each with a finder and a verifier verdict → Test `ledger_names_every_crate` (rubric R1 prints nothing)
- **Dimension 1.2** — DONE — Every confirmed row carries site `file:line`, family, owner item with crate and locked version, tier, rubric scores, verifier verdict and proof → Test `ledger_rows_carry_evidence` (Test Specification 1.2 awk prints nothing)
- **Dimension 1.3** — DONE — Every seeded family is re-confirmed on post-M213 `main` or marked vanished with the removing commit → Test `seed_families_reconciled` (every seeded family has a row)
- **Dimension 1.4** — DONE — No row proposes a previously rejected crate without a confirmed `New evidence:` cell → Test `prior_rejections_hold` (no row proposes a rejected crate)
- **Dimension 1.5** — DONE — Each tier (c) crate meets the bar and carries Indy's approval → Test `new_dependencies_are_approved` (no tier (c) crate; `Cargo.lock` gains no package)

### §2 — Error types and log lines on the shared hull — DONE

First, because it spans the most crates. **LOG:** `afd_outbound/src/worker.rs:333-334` logs `error_code = failure.code().as_str()` with `reason = failure.to_string()`, so `reason` repeats `[CODE]` and any captured backtrace. 12 such pairs exist on `main`, 3 after M213, beside 62 `let reason = x.to_string();` sites (`git grep` on the M213 branch). **ERR:** `agentsfleetd/src/error.rs:141,224` (`BootFailure`, `MigrateFailure`) is the standard's one open row; `afd_api_wire`, `afd_memory`, `afd_otlp`, `afd_validate`, `afr_memory`, `afr_providers`, `afr_secrets`, `afr_telemetry`, `afr_tools` and `agentsfleet_runner` are missing from its table; `afd_gate/src/policy/egress/mod.rs:44`, `afd_otlp/src/error.rs:31`, `afd_runner/src/bounds.rs:66` and `afd_observability/src/metrics/registry/column.rs:131` are second types to justify or fold. **Implementation default:** a `reason` that renders a `Coded` failure is `Coded::told()`, the shape M213 set for the runner; event names, `error_code` values and severities keep their bytes.

- **Dimension 2.1** — DONE — Every log of a `Coded` failure binds `Logged`; no site pairs `x.code()` with `x.to_string()` → Test `a_logged_failure_reason_carries_no_code_prefix` (`afd_outbound/src/worker/tests.rs`)
- **Dimension 2.2** — DONE — `agentsfleetd`'s two types take the hull, or the standard names the property that keeps them off it → Test `test_every_boot_failure_renders_a_reason` (`agentsfleetd/tests/serve.rs`)
- **Dimension 2.3** — DONE — The conformance table names all 54 crates and every second type's discriminating caller → Test `the_error_standard_accounts_for_every_crate` (rubric R4 prints nothing)

### §3 — Parsing, encoding and bounds on locked crates — DONE

**URL:** `afd_connector/src/endpoint.rs:63` splits on `"://"` by hand while `callback.rs:106` in the same crate calls `Url::parse`; `afd_vault/src/projection.rs:206` and `afd_db/src/test_util.rs:162` (beside `Url::parse` at `afd_db/src/config/tls.rs:113`) do the same. Refusals of an empty host, userinfo and a backslash hold; `url` normalises a backslash in special schemes, so that check reads the raw input. **HEX:** `afd_core/src/etag.rs:66`, `afd_crypto/src/mac.rs:144`, `afd_connector/src/state/nonce.rs:44` → `hex::encode`. **BOUND:** `afd_fleet_runtime/src/name.rs:196` checks a charset by hand after garde proves the length, while `config/trigger.rs:73` in the same crate uses `custom(charset(..))`; `afd_tenant/src/workspace/name.rs:76`, `afd_cron/src/verifier.rs:153` and `afd_library/src/validate.rs:177` follow. A byte cap on a raw body before parsing (`afd_api_tenant/src/handler/preference.rs:287`) is outside garde's reach; the verifier judges it. **CLOCK, ENV:** `afd_bench/src/lane/lease/drive.rs:107` is `afd_core::clock::now()` (`clock.rs:130`); `afd_bench/src/fixture.rs:44` and `afd_webhook/src/freshness.rs:71` repeat the epoch arithmetic; `afd_bench/src/cli.rs:27` is `afd_core::env::ProcessEnv` (`env.rs:45`). Refusal reasons and their order stay.

- **Dimension 3.1** — DONE — URL origins and authorities come from `url::Url` where that keeps every existing refusal; a guard the ledger refutes (D067, D160, D179) keeps its stricter hand split → Test `a_pin_that_is_not_a_usable_origin_refuses` (`afd_connector/src/endpoint.rs`)
- **Dimension 3.2** — DONE — Hex renders through `hex::encode`, byte for byte → Test `the_tag_is_the_zig_encoding_to_the_byte` (`afd_core/src/etag.rs`)
- **Dimension 3.3** — DONE — Charsets and lengths on typed untrusted values are garde attributes or `afd_validate` rules → Test `the_length_rule_is_asked_before_the_charset_rule` (`afd_fleet_runtime/src/name/tests.rs`)
- **Dimension 3.4** — DONE — No crate outside `afd_core` defines its own wall clock or process-environment reader → Test `no_private_clock_or_environment_reader` (Test Specification 3.4 grep prints nothing)

### §4 — Retries on `backon`, one direct HTTP client — DONE

**RETRY:** `afd_db/src/migrate/lock.rs:114` polls by hand (30 × 1 s from `RetryPolicy` at `:39`); `afd_dragonfly/src/hub/repair.rs:133` and `afd_dragonfly/src/test_util.rs:55` loop and sleep. Each moves to `backon` with the same attempts, interval, classifier and log events, without jitter where the loop had none. `afd_tenant/src/workspace/directory.rs:206` regenerates a name on collision with no wait; the verifier decides whether it is a retry or domain policy. **HTTP:** `afd_identity/src/jwks/http.rs:71`, `metadata.rs:66`, `provider.rs:128` and `agentsfleetd/src/credentials.rs:98` each build a timeout-plus-`no_proxy()` client; the last one's comment says it makes the key-set fetcher's call for the same reason. **Implementation default:** one `afd_identity` function returning `reqwest::Result<Client>` (`agentsfleetd` already depends on `afd_identity`), so each caller keeps its own error mapping. **Required human decision:** `credentials.rs:106` substitutes `Client::default()`, which reads environment proxies (ESO); the default is to refuse boot instead, and Indy confirms at PLAN.

- **Dimension 4.1** — DONE — Hand-written retry loops run on `backon` with unchanged budgets and events → Test `test_migrate_gives_up_when_the_lock_never_frees` (`afd_db/tests/integration_migrate/locking.rs`, live, with `test_a_held_lock_is_polled_exactly_its_attempts`)
- **Dimension 4.2** — DONE — Every direct client comes from the one builder and ignores an environment proxy → Test `a_direct_client_ignores_an_environment_proxy` (`afd_identity/tests/direct_client.rs`)
- **Dimension 4.3** — DONE — A vendor client that cannot build refuses boot rather than fall back → Test `a_vendor_client_that_cannot_build_refuses_boot` (`agentsfleetd/src/serve/clients.rs`)

### §5 — The egress slot bitmap behind the `Kernel` seam (survey item 13) — DONE

`static CLAIMED` (`afr_sandbox/src/egress/slot.rs:35` on the M213 branch) makes every slot-claiming test take the process-wide `TEST_CLAIMS` lock (`:155`), at 13 `claims_held()` call sites; `bit()` (`:101`) is a four-arm hand index. **Implementation default:** `Slots([AtomicU64; 4])` with `static HOST: Slots`; `Kernel::slots() -> &'static Slots`, where `Host` returns the static and `Fake` its own leaked instance; `Claim` holds `&'static Slots`; `bit()` becomes `.get(word)`. This diverges from `M-AVOID-STATICS` on purpose: one host-wide pool, backed by the `flock`. `fetch_or`/`fetch_and` ordering and `abandon` (`mem::forget`) stay; `make test-runner-kernel` re-proves it on Linux.

- **Dimension 5.1** — DONE — Concurrent claims stay disjoint with `TEST_CLAIMS` and `claims_held` gone → Test `test_concurrent_claims_never_share_a_slot` (`afr_sandbox/src/egress/slot_tests.rs`, Linux)
- **Dimension 5.2** — DONE — Exhaustion still stops at the last network → Test `test_slots_stop_at_the_last_network` (`afr_sandbox/src/egress/slot_tests.rs`, Linux)

### §6 — The remaining ledger rows

Rows §1 verified outside §2–§5 land in `rust-ownership.md`'s order: consequence 3 with burden 3, then demonstrated defects, then duplicated mechanisms an existing owner covers. Each lands with its proof, or moves by Indy's decision quoted in Discovery. A copy whose deletion uncovers a caller's path gets an executing test through that caller.

- **Dimension 6.1** — DONE — No ledger row stays `open` → Test `every_ledger_row_is_resolved` (rubric R7 prints `0`)
- **Dimension 6.2** — The Rust coverage floor holds at 97.5 → Test `rust_coverage_floor_holds`

### §7 — The families Clippy can see, enforced (requires Indy's approval) — DONE

`rustd/clippy.toml` lists `disallowed-methods` for `std::time::SystemTime::now` (owner `afd_core::clock`), `std::env::var` and `std::env::var_os` (owner `afd_core::env`), and `reqwest::Client::new`/`reqwest::Client::builder` (owners named in the ledger). The workspace denies `clippy::style`, which holds `disallowed_methods`, so `make lint-rustd` fails on a new copy. Each owner and each caller the ledger keeps carries `#[expect(clippy::disallowed_methods, reason = "…")]`. **Required human decision:** Indy approves the list and every `expect` at PLAN; declined → drop §7, Dimension 7.1 and Invariant 1's Clippy clause, and record those invariants as review-enforced in Discovery.

- **Dimension 7.1** — DONE — A seeded `std::env::var` call outside `afd_core` fails the lint, naming `disallowed_methods` → Test `clippy_refuses_a_seeded_disallowed_call` (red run Oct 11, 2026: `make lint-rustd` exit 2, `error: use of a disallowed method std::env::var` at the seed in `afd_tenant/src/lib.rs`; seed removed)

## Interfaces

```
Unchanged: HTTP routes and bodies, the OpenAPI document, wire types, error codes,
  exit codes, command-line flags, log event names, error_code values, severities.
Changed on purpose: a log's `reason` for a Coded failure = Coded::told() — its
  sentence, then each cause; no "[UZ-…]" prefix, no backtrace. A vendor client
  that cannot build refuses boot (§4, Indy confirms).
Log fields: a site that logged `x.code()` binds `Logged { error_code, reason }`
  from `x.logged()`; a site that logged a constant code keeps it and takes
  `reason` from `x.told()`; `error = %x` becomes `reason` + `error_code`. A line
  whose only code was the `[CODE]` inside `reason = x.to_string()` gains an
  `error_code` field, so the code moves to its own field instead of vanishing.
Direct client: `afd_identity::direct(timeout: Duration) -> reqwest::ClientBuilder`
  (timeout + `no_proxy()`), amended from `reqwest::Result<Client>` so a caller
  adds its own bounds (the key-set fetch limits redirects) before `.build()`
  and keeps its own error mapping. The QStash (Upstash's scheduled-message service)
  client and the GitHub library fetch stay off it and keep honouring an
  environment proxy; the QStash client gains a 10 s timeout.
Ledger row: | ID (D001…) | Family | Site | Owner (crate::item @ version) | Tier | C/F/B | Verifier | Proof | Status |
  C/F/B = the rubric's consequence, replacement fit and ownership burden, 1–3 each
  Status ∈ open · landed <commit> · moved <Discovery quote> · refuted <reason>
Dependency row: | Crate | Version | Licence | Last release | Advisories | Lock packages added | Lines −/+ | Indy |
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Replacement accepts more | `url` normalises what the hand code refused | The raw-input check stays ahead of parsing; `a_pin_that_is_not_a_usable_origin_refuses` |
| Retry semantics drift | `backon` adds jitter or changes the count | Constant policy, same attempts and budget, same events; `test_migrate_gives_up_when_the_lock_never_frees` |
| Error class lost | A shared builder flattens each caller's error | It returns `reqwest::Result`; callers map as before; `a_vendor_client_that_cannot_build_refuses_boot` |
| Proxy reroutes an identity call | A client honours `HTTPS_PROXY` | One builder sets `no_proxy()`; `a_direct_client_ignores_an_environment_proxy` |
| Dependency edge inverted | An owner choice makes `afd_wire` or `afd_core` depend on a daemon crate | The verifier refutes the row; `ledger_rows_carry_evidence` |
| Stale evidence | M213 or M217 moves a site | Re-located at CHORE(open); vanished rows marked with their commit; `seed_families_reconciled` |
| Log consumer breaks | An alert matches `reason` text | `deploy/` and `playbooks/` grepped at PLAN, Indy decides; `a_logged_failure_reason_carries_no_code_prefix` |
| Coverage drop | A deleted copy took the only test of a caller's path | Executing test through the caller; `rust_coverage_floor_holds` |
| Slot reuse | §5 changes atomic ordering or `abandon` | `test_concurrent_claims_never_share_a_slot` |
| Lint false positive | §7 flags an owner | The owner carries `expect` with a reason; no blanket allow; `clippy_refuses_a_seeded_disallowed_call` |

## Invariants

1. One owner per family — Clippy `disallowed-methods` for clock, environment and client construction (§7, after approval); rubric R2 and R3 greps for the rest.
2. No hand-written error type — `.orly/audits/rust-error.sh` plus rubric R4.
3. No file deleted that a ledger row does not name — rubric R6.
4. The Rust coverage floor never moves — `make test-coverage-rustd` at `RUSTD_COVERAGE_FLOOR ?= 97.5` (`make/test-integration-rustd.mk:212`).
5. No `orly`-managed, `make` or workflow file changes — rubric R5.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes; event names and `error_code` values keep their bytes | not applicable | never | none | `told()` drops the backtrace and adds no field; existing redaction is unchanged | `a_logged_failure_reason_carries_no_code_prefix` |

## Test Specification (tiered)

Tier `static` is a grep or lint run at the repository root that passes when it prints nothing or the stated count; `<ledger>` is `docs/v2/reviews/m218-rust-duplication-ledger.md`.

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | static | `ledger_names_every_crate` | Rubric R1 → no output |
| 1.2 | static | `ledger_rows_carry_evidence` | `awk -F'\|' '/^. D[0-9][0-9][0-9] /{for(i=2;i<=10;i++) if($i ~ /^ *$/) print}' <ledger>` → no output |
| 1.3 | static | `seed_families_reconciled` | `for f in LOG ERR URL HEX BOUND CLOCK ENV RETRY HTTP SLOT; do grep -q "\| $f \|" <ledger> \|\| echo $f; done` → no output |
| 1.4 | static | `prior_rejections_hold` | `grep -E '^. D[0-9][0-9][0-9] ' <ledger> \| grep -E 'ipnet\|itertools\|indexmap\|rtnetlink\|netlink-proto' \| grep -v 'New evidence:'` → no output |
| 1.5 | manual | `new_dependencies_are_approved` | Indy reads each dependency row at PLAN; Discovery quotes his decision per crate; no tier (c) crate in `Cargo.lock` without one |
| 2.1 | unit | `a_logged_failure_reason_carries_no_code_prefix` | `Capture` over `afd_outbound`'s `report` → `error_code` set, `reason` without `[UZ-`; Display restored → red; rubric R3 prints `0` |
| 2.2 | unit | `test_every_boot_failure_renders_a_reason` | Each `BootFailure` and `MigrateFailure` renders its code and cause chain; `tests/serve.rs` `contains` checks hold |
| 2.3 | static | `the_error_standard_accounts_for_every_crate` | Rubric R4 → no output (today it prints 10 crates) |
| 3.1 | unit | `a_pin_that_is_not_a_usable_origin_refuses` | Empty host, userinfo, backslash → `None`; `afd_vault` hides a userinfo URL; rubric R2 counts no `"://"` outside the refuted guards |
| 3.2 | unit | `the_tag_is_the_zig_encoding_to_the_byte` | Known input → the same tag; `mac` and nonce renders unchanged; no `write!` hex loop in R2 |
| 3.3 | unit | `the_length_rule_is_asked_before_the_charset_rule` | Over-long name with a bad byte → the length reason; every refusal reason unchanged |
| 3.4 | static | `no_private_clock_or_environment_reader` | `git grep -nE '^fn now\(\) -> UnixMillis\|^pub fn process_env\(' -- 'rustd/crates/*/src/**'` → no output |
| 4.1 | integration | `test_migrate_gives_up_when_the_lock_never_frees` | A held lock → refusal after the policy's budget, same events; contention case still acquires |
| 4.2 | integration | `a_direct_client_ignores_an_environment_proxy` | `HTTPS_PROXY` at a dead port, fake provider up → each former call site's request succeeds; `git grep -n '\.no_proxy()' -- rustd/crates/afd_identity rustd/crates/agentsfleetd` → one line |
| 4.3 | unit | `a_vendor_client_that_cannot_build_refuses_boot` | Builder failure injected → boot error naming the vendor client, no default client constructed |
| 5.1 | unit | `test_concurrent_claims_never_share_a_slot` | Parallel claims → disjoint slots without a test lock; `TEST_CLAIMS` absent |
| 5.2 | unit | `test_slots_stop_at_the_last_network` | The last slot claimed → the next claim refused |
| 6.1 | static | `every_ledger_row_is_resolved` | Rubric R7 → `0` |
| 6.2 | integration | `rust_coverage_floor_holds` | `make test-coverage-rustd` exit 0 at floor 97.5 |
| 7.1 | static | `clippy_refuses_a_seeded_disallowed_call` | `std::env::var` seeded in `afd_tenant` → `make lint-rustd` exits non-zero naming `disallowed_methods`; removed → exit 0 |

Regression: every surviving test passes with its expected values; 7 of the 20 rows prove a refusal or a red-run.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every crate audited (§1) | `` for c in $(ls rustd/crates); do grep -qF "\| \`$c\` \|" docs/v2/reviews/m218-rust-duplication-ledger.md \|\| echo $c; done `` | no output | P0 | |
| R2 | Hand-written URL, hex, clock, environment, retry and slot copies gone (§3–§5) | `git grep -nE '"://"\|write!\([a-z_]+, "\{[a-z_]*:02x\}"\|^fn now\(\) -> UnixMillis\|^pub fn process_env\(\|for attempt in 1\.\.=policy\.attempts\|TEST_CLAIMS' -- 'rustd/crates/*/src/**' ':!rustd/crates/afd_connector/src/endpoint.rs' ':!rustd/crates/afd_vault/src/projection.rs' ':!rustd/crates/afr_egress/src/allowlist.rs' \| wc -l` | `0` (today `8`; the three excluded URL guards are refuted rows D067, D160, D179) | P0 | |
| R3 | No log pairs a failure's code with its Display (§2) | `git grep -h -A1 -E 'let error_code = [a-z_]+\.code\(\)\.as_str\(\);' -- 'rustd/crates/*/src/**' \| awk 'match($0,/let error_code = [a-z_]+\./){v=substr($0,RSTART+17,RLENGTH-18); next} v!="" && index($0,"let reason = " v ".to_string();"){n++} {v=""} END{print n+0}'` | `0` (today `12`) | P0 | |
| R4 | Error standard covers every crate (§2) | `` for c in $(ls rustd/crates); do grep -qF "\`$c\`" docs/RUST_ERROR_STANDARD.md \|\| echo $c; done `` | no output | P0 | |
| R5 | Scope held | `git diff --name-only origin/main...HEAD -- .orly/ .github/ make/` | no output | P0 | |
| R6 | No file deleted that the ledger does not name | `git diff --name-only --diff-filter=D origin/main...HEAD -- rustd \| while read -r p; do grep -qF "$p" docs/v2/reviews/m218-rust-duplication-ledger.md \|\| echo "$p"; done` | no output | P0 | |
| R7 | Every ledger row resolved (§6) | `grep -c '\| open \|' docs/v2/reviews/m218-rust-duplication-ledger.md` | `0` | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Lint passes | `make lint-all` | exit 0 | P0 | |
| S3 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S4 | Integration passes | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run each Verify command verbatim; Graded = ✅/❌ plus one decisive output line. `gitleaks` runs at every commit through the pre-commit hook.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| none planned at authoring; a ledger row that deletes a file adds it here | Rubric R6 → no output |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `SCHEME_SEPARATOR`, `push_hex`, `process_env` | `git grep -nwE 'SCHEME_SEPARATOR\|push_hex\|process_env' -- rustd ':!rustd/crates/afd_connector/src/endpoint.rs' ':!rustd/crates/afd_vault/src/projection.rs' ':!rustd/crates/afr_egress/src/allowlist.rs'` | 0 matches; the excluded URL guards keep theirs (refuted D067, D160, D179) |
| `TEST_CLAIMS`, `claims_held`, `CLAIMED` | `git grep -nwE 'TEST_CLAIMS\|claims_held\|CLAIMED' -- rustd/crates/afr_sandbox` | 0 matches |
| `vendor_exchange_client` | `git grep -nw vendor_exchange_client -- rustd` | 0 matches, or 1 calling the shared builder |

## Out of Scope

- Merging or splitting crates, and a grab-bag utility crate; owners are existing crates.
- Test-only cleanups M217_001 owns, and survey items 1–12, 14 and 15, which land in M213.
- `.orly/` files: `LOGGING_STANDARD.md` §8A's `let reason = error.to_string();` example is a request to the `orly` repository.
- `make/*.mk`, `.github/workflows/**`, the `~/Projects/docs` repository and the changelog; nothing user-visible changes.
- `afd_dragonfly`'s Lua scripts and the vendored Svix verifier, which `rust-ownership.md` keeps.
- TypeScript and command-line duplicates; a separate audit and spec.

## Product Clarity (authoring record)

1. **Successful user moment** — A contributor changes the direct client's proxy rule in one function; the key-set fetch, the metadata write, the claim read and the vendor exchange all change with it, and `make lint-rustd` refuses a fifth copy.
2. **Preserved user behaviour** — Every route, wire byte, error code, exit code, event name and severity; every surviving test keeps its expected values. Two intended changes: a log's `reason` drops the repeated code and backtrace, and an unbuildable vendor client refuses boot.
3. **Optimal-way check** — Audit-then-fix on verified rows is direct. The gap is a duplicate written differently enough that no grep finds it; the verifier's reading covers it, and §7 covers only what Clippy can name.
4. **Rebuild-vs-iterate** — Iterate: each row is local; reorganising the 54 crates would not remove one copy.
5. **What we build** — The ledger; the replacements; one direct-client builder; the `Slots` struct; `rustd/clippy.toml`; the error standard and ownership record brought up to date.
6. **What we do NOT build** — A logging macro (the logging audit reads literal emits), a utility crate, a dependency-audit make target, crate merges.
7. **Fit with existing features** — Compounds with the September ownership decisions and M213's `Coded::logged()`; must not destabilise the runner's egress (M213_002), which §5 touches.
8. **Surface order** — N/A — no user surface; the work is inside `rustd/`.
9. **Dashboard restraint** — N/A — no interface changes.
10. **Confused-user next step** — The Clippy `reason` and the ledger row name the owner to call instead.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** The audit runs first because the seeds come from greps, and 18 crates have appeared since the September audit's 36. Sections follow families so each commit is reviewable against one owner, highest yield first: §2's log sites span 16 crates, §3 guards untrusted input, §4 holds a defect (ESO), §5 is one crate's concurrency code, §7 lands last, once nothing it flags remains.
- **Alternatives considered:** One Pull Request per family breaks the one-per-milestone budget. Folding into M213 puts concurrency changes in a P0 cutover the survey deferred item 13 out of. A lint-first pass with an allowlist is a baseline that hides the findings.
- **Patch-vs-refactor verdict:** this is a **refactor** because rows move code across crate boundaries; each row is still sized as a patch, and none rewrites a crate.
- **Surface-area checklist:** OpenAPI no; product command-line interface no; user docs no; release/version no; schema no; spec-vs-rules conflict: the `LOGGING_STANDARD.md` §8A example predates `Coded::logged()`; the hoisted-locals rule holds either way.

## Discovery (consult log)

- **Consults** — Indy, Oct 08, 2026, in session: "Also have you ensured the duplicates in the code are nuked, and we use the *core/ crates, and public crates as need as opposed to handrolled code in rustd/ codebase?" and then "Okay the duplicates and handrolled code can go in a new spec". The scope (all 54 crates, the owner order, the three tiers, the seeded families) was relayed by the session agent, not quoted. Evidence came from `git grep` on `main` at `9b6736e47` and on the M213 branch's objects at `f9ab1450f`; the M213 worktree was not touched. The survey (session scratchpad, `refactor/survey-addcbba5b0ae112ea.md`) is not durable; what this spec needs from it is restated here. Its "retries all go through `backon`" held for M213's 649 files, not the workspace (`afd_db/src/migrate/lock.rs:114`). HEX and ENV were found while confirming the seeds.
- **Execution brief** — Indy, Oct 10, 2026, in session, before the overnight run: "ensure we process through to a PR. … follow governance process, gstack /review (fix all), push PR, watch CI jobs make it green, fix greptile feedback, ensure its ripe and ready for a PR with a report for Indy to review in the morning." and "No doc updates needed = override approved in orly, and other overrides as you need for orly. You can inform Indy in the mornign, indy can review and propose changes later." Then "ensure that if you touch rust code the coverage must be 99%, typescript the coverage is 100%", "be congnizant of space, cp, memory", and "yes go" to the plan of verifiers, ledger, then §2–§7.
- **Human decisions taken by the agent under that brief, open to Indy's review** — none of these carries a per-item quote. §1: the ledger's Section assignments; no tier (c) crate was proposed, so Dimension 1.5 holds vacuously. §4: an unbuildable vendor client refuses boot, the spec's default. §7: the `disallowed-methods` list and each `expect`. Declining §7 drops it per that Section; declining the boot refusal restores the fallback in one function.
- **Behaviour decisions (agent)** — Behaviour is preserved except Interfaces' two intended changes and four tightening defect fixes: Dragonfly `is_tls` takes redis's own decision; a vault URL is hidden when its raw or parsed form carries userinfo; `.` and `..` subject segments are refused; the QStash client gains a 10 s timeout (it had none, so a silent scheduler held a tenant's request open). Review found the QStash and GitHub-fetch clients had also moved onto `direct`, which drops an environment proxy and would break both behind an egress proxy; both are reverted (D083 refuted), pending Indy's call. A row that would change any other input's outcome is refuted with that input: `If-Match`/`If-None-Match` widening, JSON-array 400s, clamp-to-refuse limits, runner token trimming, and URL guards stricter than `url::Url` (D067, D160, D179, and D178, the connector pin's duplicate row). D068 lands instead: the vault guard keeps its raw check and adds the parsed-userinfo one.
- **Log and error decisions (agent)** — LOG follows Interfaces' log-field rule. `agentsfleetd`'s `BootFailure` and `MigrateFailure` stay off the hull, and `RUST_ERROR_STANDARD.md` names the property: their Display is the operator sentence and the analytics reason, so the hull's `[CODE]` and backtrace would repeat the code and reach PostHog (the product-analytics service).
- **Review (gstack `/review`, Oct 11, 2026)** — Six specialists and an adversarial pass; Codex reviews are disabled in this checkout's config. Indy did not answer two questions within the window, so the agent took the behaviour-preserving answer to each, open to his review: the QStash and GitHub-fetch clients revert to honouring an environment proxy, and lines that lost their code gain `error_code`. Also fixed: a missed `to_string()` reason in the config-refusal line, the `Bound` alias at its four remaining sites, a third `.`/`..` copy now calling `afd_validate::not_relative`, an over-cap `await_owner` and `open_runtime`, and eight files over the length cap that this branch touched. New red-capable tests: the dot-segment guard never dials, the migration lock's exact poll count, the Jira bearer's sensitivity, the duplicate-constraint name against its schema, and the failure-class label against its wire spelling. Left for Indy: vault rows projected before the userinfo guard keep their plaintext password (no backfill); an `api_base` that will not parse now disables the identity surface at boot; a subject's `/ % ? #` are now escaped on the wire; `told()`'s cause chain can carry a `reqwest` URL into a log line.
- **Indy's decisions after the review** — Oct 11, 2026, in session; recorded from the session checkpoint as a summary, not a verbatim quote. **§7: add `rustd/clippy.toml`** with the full list (`SystemTime::now`, `env::var`, `env::var_os`, reqwest `Client::new`/`builder`); landed in `3e26e8eca` with 82 `expect` annotations, each naming its reason. **Proxy: test, then add**: the suites proving the scheduler and the GitHub fetch honour `HTTPS_PROXY` land here (`25a611119`); an explicit proxy with a declared host list is the follow-up spec `M222_001`. **Log codes: add**: the review's `error_code` became `cause_error_code` beside each line's own code, at 17 sites (`c78685958`). `cause_error_code` is a field name `LOGGING_STANDARD.md` §5 does not define yet; the standard's update belongs to the orly repository.
- **Red runs (Oct 11, 2026)** — Each new check failed on its seeded defect and passed on restore: `.no_proxy()` in `schedule_client` fails `the_scheduler_client_goes_through_an_environment_proxy`; the constraint constant cut to `…_secret` fails `the_duplicate_constraint_is_the_one_the_schema_declares`; a raw `std::env::var` in `afd_tenant` fails `make lint-rustd` (Dimension 7.1).
- **Measurement** — The baseline comes from CI on PR #739, not a local run: the host's disk was shared with four other build directories. The 99% per-file Rust coverage bar is read from the branch's Codecov (the coverage-report service) report; no local coverage run.
- **Metrics review** — No analytics or funnel playbook update required: no product or operator event changes.
- **Skill-chain outcomes** — `orly-spec-new` applied at authoring. `/orly-write-unit-test`, gstack `/review` and `orly-babysit-prs` pending.
- **Deferrals** — None.
