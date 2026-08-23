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

# M176_001: Rust daemon substrate — boots, migrates, authenticates, shuts down clean

**Prototype:** v2.0.0
**Milestone:** M176
**Workstream:** 001
**Date:** Aug 23, 2026
**Status:** PENDING
**Priority:** P0 — every route milestone (M177–M180) builds on these crates
**Categories:** API | INFRA | OBS
**Batch:** B2 — serial after M175; M177+ depend on it
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** M175_001 (workspace, lanes, afd_core, afd_wire)
**Provenance:** LLM-drafted (Claude Fable 5, Aug 23, 2026)
**Canonical architecture:** `docs/architecture/concurrency.md` (thread/lock/shutdown layer) + `docs/architecture/data_flow.md` §Connection topology

---

## Overview

**Goal (testable):** `agentsfleetd-rs serve` boots against real Postgres + Redis to a green `/healthz` + `/readyz`, `agentsfleetd-rs migrate` produces `audit.schema_migrations` row-parity with the Zig migrator on a fresh database, vault rows encrypted by the Zig daemon decrypt in Rust (and the reverse), and SIGTERM joins every background task before teardown in a deterministic test.
**Problem:** the port family needs a daemon skeleton with the four substrate layers — database, Redis, envelope crypto, authentication — proven interoperable with live production data shapes before any route logic exists; getting shutdown or crypto subtly wrong here poisons every later milestone.
**Solution summary:** seven new crates (`afd_crypto`, `afd_db`, `afd_redis`, `afd_auth`, `afd_api` shell, `afd_observability`, `afd_state` seeded with the auth-consumed lookups) plus the `agentsfleetd` binary crate with boot/shutdown choreography ported from `cmd/serve.zig`; parity proven by cross-implementation fixtures (crypto), fresh-database diffs (migrations), and deterministic lifecycle tests (shutdown).

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): daemon substrate — db, redis, crypto, auth, boot/shutdown
- **Intent (one sentence):** the Rust daemon can be booted, migrated, probed, and cleanly stopped against the production schema and stores, so route milestones only add handlers, never plumbing.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/concurrency.md` — the C1–C5 invariants, thread map, two-flag shutdown semantics, and 7-step teardown this milestone re-derives as a task-shutdown ordering; the Zig comments are the spec.
2. `src/agentsfleetd/cmd/serve.zig` + `cmd/serve_background.zig` — boot order and the background fleet being ported.
3. `src/agentsfleetd/db/pool.zig`, `src/agentsfleetd/db/pool_migrations.zig`, and `schema/embed.zig` (repo root) — pool roles, migration bookkeeping, and the "version IS the slot number" rule the Rust runner must preserve.
4. `docs/AUTH.md` §Auth model in one screen + §Backend validation — the five principal surfaces, `bearer_or_api_key` routing order, and JWKS (JSON Web Key Set) cache semantics; auth-flow work reads this first (repo rule).
5. `src/agentsfleetd/secrets/crypto_primitives.zig` + `state/vault.zig` — key-encryption-key/data-encryption-key (KEK/DEK) envelope layout and the metadata-projection-in-same-statement rule.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_crypto/**` | CREATE | envelope encryption (KEK/DEK, AES-256-GCM), HMAC canon, zeroizing secret types |
| `rustd/crates/afd_db/**` | CREATE | sqlx pools (three roles), migration runner with advisory lock + audit bookkeeping |
| `rustd/crates/afd_redis/**` | CREATE | streams ops, subscription hub, session store, `fleet:ready` hash |
| `rustd/crates/afd_auth/**` | CREATE | principal, scope catalogue + ladder, JWKS verify, bearer routing, requireScope |
| `rustd/crates/afd_api/**` | CREATE | axum shell: Route enum + route_meta, admission limiter, problem+json envelope |
| `rustd/crates/afd_observability/**` | CREATE | tracing → OpenTelemetry Protocol (OTLP) export, semconv attributes, counters |
| `rustd/crates/afd_state/**` | CREATE | repository crate, seeded with the auth-consumed lookups (api keys, CLI credentials); M178/M179 extend it |
| `rustd/crates/agentsfleetd/**` | CREATE | binary: subcommand parsing (serve/migrate), boot choreography, task supervisor |
| `rustd/Cargo.toml` | EDIT | new workspace members + workspace dependencies |
| `src/agentsfleetd/secrets/envelope_fixture_export.zig` | CREATE | Zig-side emitter + reader for cross-implementation crypto fixtures |
| `samples/fixtures/vault-envelope/**` | CREATE | committed envelope fixtures (Zig-written for Rust to decrypt; test-time reverse pass) |
| `make/test-integration.mk` | EDIT | Rust substrate integration suite joins `test-integration` against the same compose services |
| `.github/workflows/test-integration.yml` | EDIT | Rust integration steps beside the Zig ones (CI edit — explicit user approval per repo rule; this spec is the record) |
| `docs/architecture/concurrency.md` | EDIT | adds the Rust task-map section beside the Zig thread map |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — CTM (constant-time comparison for secrets), ECL (error classes: timeout ≠ fatal ≠ retryable — the two pool-acquire failures stay distinct), NSQ (named constants, schema-qualified SQL), OWN (one owner per resource — hub owns the subscribe connection), FLS (drain all result layers), UFS, NDC, TST-NAM, MSID, FLL.
- `dispatch/write_rust.md` — deterministic concurrency tests are mandatory here (shutdown, hub, advisory lock); preserved error variants; REVIEW cites Microsoft guideline mnemonics.
- `dispatch/write_zig.md` — fires for the fixture emitter.
- `docs/AUTH.md` — read before §4 work (auth-flow rule in AGENTS.orly.md).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes | module-per-concern crates; split query modules by domain |
| LOGGING | yes — runtime logging begins here | scoped events, error codes, severity, redaction via tracing fields; secrets never formatted |
| MILESTONE-ID | yes | no milestone tokens in `rustd/` or fixtures |
| UFS | yes | env knob names, Redis key patterns, pool sizes as named constants byte-identical to Zig |
| ZIG GATE | yes — fixture emitter | init/deinit + errdefer; imports production crypto modules |
| SCHEMA GUARD | no | `schema/*.sql` untouched — Rust embeds the same files |
| SPEC TEMPLATE GATE | yes — this file | required sections filled, zero tpl residue |

## Prior-Art / Reference Implementations

- **Reference:** `~/Projects/oss/exonum` — message-driven shutdown with explicit signal-ownership coordination and the builder-with-`development_node()` pattern; its separate-thread-per-HTTP-runtime hack is an actix workaround to NOT copy (axum futures are Send).
- **Reference:** `~/Projects/oss/core_api-develop` — the anti-pattern record for this milestone: zero signal handling, three runtimes in one process, infinite pool-construction retry, `NoTls` Postgres. Each is inverted here as a Failure Mode or Invariant.
- **Reference:** `src/agentsfleetd/` (Zig daemon) — behaviour source of truth; SQL and env knob names port verbatim.

## Sections (implementation slices)

### §1 — afd_crypto: envelope parity

KEK resolved once at boot from `ENCRYPTION_MASTER_KEY` (64 hex chars → 32 bytes), immutable after; per-row DEK wrapped under the KEK; AES-256-GCM (Advanced Encryption Standard, Galois/Counter Mode) with the documented 96-bit random-nonce policy; HMAC-SHA256 (hash-based message authentication code) canon with constant-time compare. Secrets ride zeroize-on-drop newtypes whose Debug/Display redact. **Implementation default:** RustCrypto crates (`aes-gcm`, `hmac`, `sha2`, `zeroize`) — pure-Rust, audited, no C linkage, matching the Zig daemon's no-C posture.

- **Dimension 1.1** — Rust decrypts Zig-written envelope fixtures → Test `test_envelope_decrypts_zig_fixtures`
- **Dimension 1.2** — Zig decrypts Rust-written envelopes (reverse pass runs in the Zig test build) → Test `test_envelope_zig_reads_rust`
- **Dimension 1.3** — tampered tag, nonce, or ciphertext fails closed with a typed error, no panic → Test `test_envelope_rejects_tampered`
- **Dimension 1.4** — secret newtypes zero on drop and redact in Debug/Display → Test `test_secret_types_redact`

### §2 — afd_db: pools and migrations

Three pool roles (default/api/migrator) with env parity (`DATABASE_URL[_API|_MIGRATOR]`, size and timeout knobs, TLS-required default, `?sslmode=disable` honored for local dev); the two distinct acquire failures (capacity vs datastore-unreachable) preserved as separate variants. Migration runner embeds the same `schema/` SQL files, applies under the same Postgres advisory lock, writes the same `audit.schema_migrations` / `audit.schema_migration_failures` bookkeeping, reaps orphans, honors `MIGRATE_ON_START`. **Implementation default:** sqlx with plain runtime queries (no compile-time query macros this milestone) — parity beats macro ergonomics during the port.

- **Dimension 2.1** — fresh-database migrate: applied-version set and bookkeeping rows identical to the Zig migrator → Test `test_migrate_parity_fresh_db`
- **Dimension 2.2** — two concurrent migrators: one applies, one waits and no-ops; no double-apply → Test `test_migrate_advisory_lock_contention`
- **Dimension 2.3** — a failing migration records a failure row and never marks success → Test `test_migrate_failure_bookkeeping`
- **Dimension 2.4** — pool acquire distinguishes capacity timeout from datastore-unavailable → Test `test_pool_error_classes`

### §3 — afd_redis: streams, hub, sessions

Stream ops parity (`XADD fleet:{id}:events` where the entry id IS the canonical event id; non-blocking `XREADGROUP`; `XACK`; `PUBLISH`), pool with short-lived commands only; the SubscriptionHub as one dedicated subscribe connection per process with refcounted channels fanning out via broadcast; the Redis-backed session store (`auth:session:{id}`, 5-minute time-to-live, atomic Lua state transition); the global `fleet:ready` hash. **Implementation default:** the `redis` crate (tokio + TLS features) — retires the ~3.0k-line hand-rolled Zig client files (the full `queue/` tree is 3.7k; its connector outbound worker is M180's port, not this crate swap).

- **Dimension 3.1** — stream append → consumer-group read → ack round-trip with entry-id-as-event-id → Test `test_stream_xadd_readgroup_ack`
- **Dimension 3.2** — hub: N subscribers share one connection; channel closes at zero refcount → Test `test_hub_refcount_single_connection`
- **Dimension 3.3** — hub reconnects and resubscribes after a dropped connection; late subscribers stream on → Test `test_hub_reconnect_resubscribes`
- **Dimension 3.4** — session state transition is atomic under parallel approve/verify races → Test `test_session_transition_atomic`

### §4 — afd_auth: principals, scopes, verification

`AuthPrincipal`, the scope catalogue mirrored from `src/agentsfleetd/auth/scopes.zig` (read < write < admin ladder expanded at parse), JWKS fetch (issuer-derived URL, 6-hour cache, refresh on key-id miss) + RS256 verify (`iss`/`aud`/`exp`), `bearer_or_api_key` routing in the documented order (`agt_t` → hash lookup; `afc_` → credential lookup + live scope resolve; else OIDC (OpenID Connect); prefixed branches ahead of the verifier check), `requireScope` any-of hierarchy-expanded 403 naming the missing scope (UZ-AUTH-022). All hash compares timing-safe.

- **Dimension 4.1** — prefix routing parity, including a no-verifier deployment still resolving `agt_t`/`afc_` → Test `test_bearer_prefix_routing`
- **Dimension 4.2** — bad signature / expired / wrong aud / wrong iss each 401; key-id miss triggers exactly one refresh → Test `test_jwks_verify_negative_paths`
- **Dimension 4.3** — `fleet:admin` passes a `fleet:read` gate; empty scope set fails closed → Test `test_scope_ladder_expansion`
- **Dimension 4.4** — missing scope → 403 UZ-AUTH-022 naming the scope → Test `test_require_scope_names_missing`

### §5 — afd_api shell: routes, admission, envelope

The axum + tower shell: a `Route` enum as the single metadata source with an exhaustive `route_meta()` (collapsing the Zig daemon's four parallel total tables — middleware chain, scopes, admission class, span template — a new variant fails compilation until matched); the axum router generated from the enum; admission-based rate limiting (atomic in-flight ceiling, 429 + `Retry-After` + `X-RateLimit-*` before body read; ops routes exempt; stream class capped separately); the 16 KiB request-header limit with its proxy-chain rationale (`src/agentsfleetd/http/server.zig:29-42`); the `application/problem+json` error envelope wired to the afd_core code registry. Only `/healthz` + `/readyz` are served this milestone.

- **Dimension 5.1** — `route_meta` is total: non-exhaustive match fails the build; a walk over all variants passes → Test `test_route_meta_total`
- **Dimension 5.2** — requests past the ceiling shed with 429 + headers before any handler runs → Test `test_admission_sheds_over_ceiling`
- **Dimension 5.3** — headers >4 KiB and ≤16 KiB accepted; >16 KiB → 431 → Test `test_header_limit_16k`
- **Dimension 5.4** — error responses match the problem+json shape the Zig daemon emits → Test `test_problem_json_envelope`

### §6 — afd_observability: push-only telemetry

tracing subscriber exporting logs/traces/metrics over OTLP (push-only egress — no scrape endpoint, matching the Zig daemon), semconv attributes with low-cardinality `http.route` templates, counter families. Export failure never blocks the request path (bounded buffer, drop counter).

- **Dimension 6.1** — spans carry the route template, never the raw path → Test `test_span_route_template`
- **Dimension 6.2** — OTLP endpoint down: requests unaffected; drop counter increments → Test `test_otlp_outage_nonblocking`

### §7 — Boot and shutdown choreography

Boot order ported from `cmd/serve.zig` (pools → Redis → migrations check → session store → hub → secrets → verifier → middleware → background tasks → listen); a task supervisor where every background task owns a cancellation token and an awaited join handle — stop → join → drop (invariant C2); the two-flag boot-window semantics preserved (SIGTERM during boot cannot kill the background stack while the server may still come up); `/readyz` probes dependencies, `/healthz` reports liveness. This section opens with a **complete task inventory** derived from the `concurrency.md` thread map: every long-lived thread AND every detached-worker class (Clerk metadata fetch, fleet install-step workers) maps to either a named supervised task here or an explicitly deferred inventory row naming its arriving milestone — detached workers become tracked tasks with bounded drains; no unsupervised spawn path exists. Deadlines are `tokio::time::timeout` at call sites, and shutdown cancellation is PROVEN to interrupt blocked I/O (one cancellation test per long-lived I/O owner) — together these are the explicit replacement for the Zig `call_deadline` scheduler and its socket-shutdown wake, stated as invariants, not assumed away.

- **Dimension 7.1** — SIGTERM: every task joins before pools close; deterministic via event handshakes, no sleeps → Test `test_shutdown_joins_all_tasks`
- **Dimension 7.2** — boot-window SIGTERM follows the two-flag semantics → Test `test_boot_window_sigterm`
- **Dimension 7.3** — `/readyz` red when Postgres or Redis unreachable; `/healthz` still 200 → Test `test_readyz_dependency_probe`
- **Dimension 7.4** — missing/malformed `ENCRYPTION_MASTER_KEY` refuses boot with a named error, non-zero exit → Test `test_boot_refuses_bad_kek`
- **Dimension 7.5** — the task inventory covers every `concurrency.md` thread-map row (supervised or explicitly deferred), and cancellation interrupts a blocked read on every I/O owner → Test `test_task_inventory_and_cancellation`

### §8 — Credential enumeration and integration harness

Credential-gate rule (AGENTS.orly.md §Bootstrap): this milestone's downstream credentials, enumerated with fetch locations — `DATABASE_URL`, `DATABASE_URL_API`, `DATABASE_URL_MIGRATOR`, `REDIS_URL`, `ENCRYPTION_MASTER_KEY`, `OIDC_ISSUER`, `OIDC_AUDIENCE` (+ optional `OIDC_JWKS_URL`), OTLP endpoint + token — all resolved from `~/.config/agentsfleet/` env links (`.githooks/post-checkout`; provisioned by `provision-env-1password` in dotfiles). Boot preflight fails loud listing every missing one. The Rust substrate integration suite joins `make test-integration` against the same compose services.

- **Dimension 8.1** — preflight lists all missing credentials in one output, not first-failure-only → Test `test_preflight_lists_missing`
- **Dimension 8.2** — Rust integration suite runs inside `make test-integration` and propagates failure → Test `test_integration_lane_rust`

## Parallelization & execution map

(Internal batch labels here sequence THIS milestone's work only; the frontmatter **Batch:** line is the family-level ordering — two different axes, deliberately.)

| Batch | Scope | Runtime · model · reasoning tier | Why |
|---|---|---|---|
| B1 | §1 crypto | Claude Code · Opus 5 · xhigh | cross-implementation crypto parity is unforgiving; wrong-but-plausible is the failure mode |
| B1 | §2 db | Claude Code · Opus 5 · high | mechanical port with a precise bookkeeping oracle |
| B1 | §3 redis | Claude Code · Opus 5 · high | crate-backed rewrite with clear behaviour tests |
| B2 | §4 auth | Claude Code · Opus 5 · xhigh | security boundary; AUTH.md ordering subtleties |
| B2 | §5 api shell | Claude Code · Opus 5 · high | enum + tower layers, well-scoped |
| B2 | §6 observability | Codex · GPT 5.6 tera · high | well-trodden tracing/OTLP wiring, cheap to verify |
| B3 | §7 boot/shutdown | Claude Code · Opus 5 · max | the choreography is the riskiest judgment work in the family |
| B3 | §8 harness | Claude Code · Opus 5 · high | lane wiring + preflight, mechanical |

B1 sections are independent crates; B2 consumes B1; B3 composes everything. Indy decides how many agents actually spin per batch.

## Interfaces

```
Binary        agentsfleetd-rs {serve, migrate}   (doctor/backfill: M181)
Env knobs     byte-identical names to cmd/serve.zig (UFS constants)
HTTP (this
milestone)    GET /healthz → 200 · GET /readyz → 200|503 (dependency-probed)
Crate seams   afd_crypto::Envelope {seal, open} over Zeroizing buffers
              afd_db::Pools {default, api, migrator} + Migrator::run
              afd_redis::{Streams, Hub, SessionStore}
              afd_auth::{AuthPrincipal, Verifier, layers}
              afd_api::{Route, route_meta} — exhaustive, single metadata source
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Pool capacity exhausted | burst past pool size | bounded wait then capacity-timeout variant → 500 with the capacity error code; distinct from datastore-down |
| Postgres unreachable | outage / bad URL | datastore-unavailable variant; `/readyz` 503; boot preflight fails loud; no infinite retry loop |
| Redis down at boot | outage | boot refuses with named dependency; during serve: hub reconnect loop with jittered backoff, viewers see a gap then resume |
| Advisory-lock contention | two migrators race | second waits; on timeout exits non-zero without partial bookkeeping |
| JWKS unreachable | identity provider outage | cached keys keep verifying ≤6h; fetch failure on cold cache → 401 path with retryable class, never a panic |
| OTLP endpoint down | collector outage | bounded buffer drops with counter; request path unaffected |
| SIGTERM in boot window | orchestrator restart | two-flag semantics: background stack survives until the server verdict; then clean stop |
| KEK absent/malformed | misconfig | boot refused, named error, non-zero exit — fail closed before any traffic |

## Invariants

1. Stop → join → drop for every background task — enforced by the supervisor owning every join handle + `test_shutdown_joins_all_tasks`.
2. Exactly one Redis subscribe connection per process — only the hub type can construct the pub/sub client (visibility) + `test_hub_refcount_single_connection`.
3. KEK is resolved once before traffic and immutable after — a set-once cell with no setter on the public surface.
4. All I/O deadlines are `tokio::time::timeout` at the call site — afd_db/afd_redis export only wrapper clients whose constructors take timeout config; raw clients stay private.
5. Secrets live in zeroize-on-drop newtypes that redact in Debug/Display — `test_secret_types_redact` + clippy deny on the raw-type escape hatch.
6. Scope strings, error codes, and env knob names are single-sourced (afd_core/afd_auth constants mirrored from the Zig canon) — duplication fails `test_error_registry_unique` and UFS review.
7. Every task spawn goes through the supervisor — `tokio::spawn` is a clippy `disallowed-methods` entry outside the supervisor module, so an unsupervised task fails `make lint-all`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `daemon.otlp_dropped_total` | ops | telemetry export buffer drops a batch | count, signal type | no payload content | `test_otlp_outage_nonblocking` |
| `daemon.boot_preflight_failed` | ops | boot refused on missing config/creds | missing-key names only | values never logged | `test_preflight_lists_missing` |
| product analytics | not applicable | — | no product events change in this milestone | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_envelope_decrypts_zig_fixtures` | every committed fixture opens to its known plaintext |
| 1.2 | integration | `test_envelope_zig_reads_rust` | Zig test build opens Rust-sealed envelopes (reverse pass) |
| 1.3 | unit (negative) | `test_envelope_rejects_tampered` | flipped byte in tag/nonce/ciphertext → typed error, no panic |
| 1.4 | unit | `test_secret_types_redact` | Debug/Display of secret newtypes contain no key material; drop zeroizes |
| 2.1 | integration | `test_migrate_parity_fresh_db` | applied-version set + bookkeeping rows equal Zig migrator output on a fresh database |
| 2.2 | integration (negative) | `test_migrate_advisory_lock_contention` | concurrent migrators → single apply, no duplicate rows |
| 2.3 | integration (negative) | `test_migrate_failure_bookkeeping` | injected failing SQL → failure row present, success absent |
| 2.4 | integration (negative) | `test_pool_error_classes` | exhausted pool vs stopped Postgres → two distinct variants |
| 3.1 | integration | `test_stream_xadd_readgroup_ack` | append/read/ack round-trip; entry id used as event id |
| 3.2 | integration | `test_hub_refcount_single_connection` | N subscribers, one connection observed; zero refcount closes channel |
| 3.3 | integration (negative) | `test_hub_reconnect_resubscribes` | killed connection → resubscribe; post-gap messages delivered |
| 3.4 | integration (negative) | `test_session_transition_atomic` | parallel approve/verify → exactly one wins; store state legal |
| 4.1 | unit | `test_bearer_prefix_routing` | `agt_t`/`afc_`/JWT/garbage each route to the documented validator; no-verifier deployment still resolves prefixes |
| 4.2 | unit (negative) | `test_jwks_verify_negative_paths` | bad sig / expired / wrong aud / wrong iss → 401 each; kid miss → one refresh |
| 4.3 | unit | `test_scope_ladder_expansion` | admin satisfies read; empty set fails closed |
| 4.4 | unit (negative) | `test_require_scope_names_missing` | missing scope → 403 UZ-AUTH-022 with the scope name in the body |
| 5.1 | unit | `test_route_meta_total` | every Route variant yields metadata; compile guard noted |
| 5.2 | integration (negative) | `test_admission_sheds_over_ceiling` | ceiling+1 concurrent requests → one 429 with Retry-After before handler execution |
| 5.3 | integration (negative) | `test_header_limit_16k` | 8 KiB header accepted; 17 KiB → 431 |
| 5.4 | unit | `test_problem_json_envelope` | error body fields match the Zig envelope shape byte-for-field |
| 6.1 | unit | `test_span_route_template` | span attribute is the template, not the raw path with ids |
| 6.2 | integration (negative) | `test_otlp_outage_nonblocking` | collector down → request latency unaffected; drop counter grows |
| 7.1 | integration | `test_shutdown_joins_all_tasks` | SIGTERM → join order asserted via handshake events; no timeout-based sleeps |
| 7.2 | integration (negative) | `test_boot_window_sigterm` | signal mid-boot → behaviour matches the two-flag rule |
| 7.3 | integration (negative) | `test_readyz_dependency_probe` | stopped Postgres → `/readyz` 503, `/healthz` 200 |
| 7.4 | integration (negative) | `test_boot_refuses_bad_kek` | absent/short key → non-zero exit naming the knob |
| 7.4 (FM) | integration (negative) | `test_boot_refuses_redis_down` | Redis unreachable at boot → refusal naming the dependency, non-zero exit |
| 7.5 | integration | `test_task_inventory_and_cancellation` | inventory rows cover the `concurrency.md` map; cancel token interrupts a blocked read per I/O owner |
| 8.1 | integration (negative) | `test_preflight_lists_missing` | three unset knobs → all three named in one failure |
| 8.2 | integration | `test_integration_lane_rust` | seeded failing Rust integration test → `make test-integration` exit non-zero |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Boot to ready on compose (§7) | `curl -fsS localhost:3000/readyz` after `agentsfleetd-rs serve` (PORT default 3000, `src/agentsfleetd/http/server.zig:63`) | HTTP 200 | P0 | |
| R2 | Migration parity (§2) | `cd rustd && cargo test test_migrate_parity_fresh_db` | exit 0 | P0 | |
| R3 | Crypto parity both directions (§1) | `cd rustd && cargo test -p afd_crypto` + Zig reverse-pass suite | exit 0 both | P0 | |
| R4 | Deterministic shutdown (§7) | `cd rustd && cargo test test_shutdown_joins_all_tasks` | exit 0 | P0 | |
| R5 | Substrate integration suite | `make test-integration` | exit 0 | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.lint`, `verify.unit`, `verify.version`) — the set `orly gate` runs; S5–S6 are the template's repository hygiene gates (secret scan, oversize sweep), deliberately outside the declared set; R-rows name oracles this spec's own Files Changed create, so every command is copy-paste by merge time.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Every business route: runner verbs (M177), tenant/workspace surface (M178), admin/operator (M179), signed ingress (M180); the credential broker, cron, fleet-config parsing (M177), connectors (M180).
- `doctor`/`backfill` subcommands and deploy shape (M181); PostHog product-analytics port (lands with the surfaces that emit events, M178).

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator on staging watches `agentsfleetd-rs migrate` no-op against an already-migrated production-shaped database, `serve` go ready, and Ctrl-C produce an orderly join log — the Rust daemon is real infrastructure, not a demo.
2. **Preserved user behaviour** — the Zig daemon is untouched and remains the only production binary; the shared `schema/` files and their version slots are byte-identical.
3. **Optimal-way check** — substrate-first is the direct route; porting a route first (demo appeal) would smuggle plumbing in untested.
4. **Rebuild-vs-iterate** — deliberate rebuild on new substrate (tokio/sqlx/axum); determinism is preserved by parity fixtures and the shared schema, so the rebuild does not trade run-to-run determinism away.
5. **What we build** — six crates + the binary with boot/shutdown, crypto/migration parity fixtures, integration-lane wiring.
6. **What we do NOT build** — any tenant- or runner-visible behaviour change; a second migration system (same files, same bookkeeping); a bespoke deadline scheduler (tokio timeouts suffice).
7. **Fit with existing features** — compounds with M175 lanes; must not destabilize the Zig daemon's migrations — both binaries must be able to run `migrate` against the same database interchangeably.
8. **Surface order** — N/A — no user surface (operator-only substrate).
9. **Dashboard restraint** — N/A — no UI.
10. **Confused-user next step** — boot preflight names every missing knob and its fetch location in one output; `/readyz` body names the failing dependency.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** eight slices in three internal batches — the three independent stores first (crypto/db/redis), then the layers that consume them (auth/shell/observability), then composition (boot/shutdown/harness) — because dependency direction, not theme, dictates safe parallelism.
- **Alternatives considered:** porting `call_deadline` + the sync primitives faithfully (rejected: tokio owns both concerns natively; porting them re-imports the hazard class concurrency.md exists to police); using diesel or a second migration tool (rejected: one embedded-SQL runner with the existing bookkeeping keeps both binaries interchangeable on one database).
- **Patch-vs-refactor verdict:** this is a **refactor** (substrate rebuilt on a new runtime) because the Zig concurrency layer cannot be translated line-by-line into async Rust without importing its workarounds; parity fixtures + the shared schema keep the refactor honest.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
