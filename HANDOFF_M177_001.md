# Handoff — M177_001 runner control plane · §2 lease, mid-slice

Ephemeral. Delete at CHORE(close).

## Scope / Status

Porting `/v1/runners` to `agentsfleetd-rs`. Spec
`docs/v2/active/M177_001_P0_API_RUNNER_CONTROL_PLANE_PARITY.md`, IN_PROGRESS.

- ✅ CHORE(open), crypto/dependency foundation, **§1 complete** (see prior handoff
  in git history for what §1 built — unchanged this session)
- ✅ **§2 assignment half** — claim, fence, reclaim, selection pass, lease row,
  and hot-path write 1. Eight integration tests green against live datastores.
- ⏳ **§2 money half — BLOCKED ON §5.** The gates, the debits and
  `ExecutionPolicy` all need the fleet config, which §5 owns.
- ⏳ **§5 NEXT** — pulled forward ahead of §2's remainder (Indy, this session).
- ⏳ §3 report, §4 activity/memory/bundles/mint, §6 sweepers, §7 harness

### Decisions taken this session (all Indy's, none re-litigable)

1. **OIDC/Clerk required at boot** — matches Zig + M176's stated gate.
   `Identity::Absent` is gone; four knobs are required, `OIDC_JWKS_URL` stays
   optional-derived. Verified against the real binary, both directions.
2. **Two debit points, per the spec, NOT per the Zig code.** The Zig lease path
   debits once (`service_billing.zig:264` — *"No issue-time stage debit"*) while
   the spec and `data_flow.md` §C both say two. Decision: implement the
   DOCUMENTED behaviour in Rust, leave Zig alone, register the divergence.
   Recorded in spec §2 and `data_flow.md`. **The run estimate is an ESTIMATE
   reconciled by §3's settle — never a second charge.**
3. **OTLP export deferred to M181** as its new §5, with three dimensions and a
   vendor-neutral knob rename. See Risks.
4. **§5 before §2's remainder** — the spec's batch table said otherwise; it was
   wrong, because `FleetSession::claimFleet` resolves config.
5. **FLL split by concern**, not by moving tests to `_test.rs` siblings.

## Working Tree

Worktree `/Users/kishore/Projects/agentsfleet-m177-runner-plane`; main checkout
still on `main`.

```
## feat/m177-runner-control-plane-parity
6469d2b3c docs(m182): add spec — execution substrate abstraction   ← another agent
e5aef4ccf one crypto provider — aws-lc-rs replaces ring, reqwest 0.13, object_store
5f7beed8b afd_fleet — the runner row, enrolment, and the verdict
87ee0c886 chore(m177): open the runner control-plane parity stream
```

4 commits, **none pushed**. §1 + §2 remain **staged and uncommitted** — now 76
files, +7743/−262. `git diff --cached` is the whole slice, and
`make harness-verify` grades the staged lens, so leaving it staged keeps the
gates re-grading it.

**Deliberately NOT staged, never commit** — the orly 0.6.8→0.6.9 pack, carried
in so the rules here match the ones being followed:

```
 M .oracle/orly.json
 M dispatch/write_any.md
 M dispatch/write_python.md
 M dispatch/write_rust.md
 M dispatch/write_shell.md
```

⚠️ **`git add -A` stages these.** It happened twice this session and was undone
with `git restore --staged <paths>` both times. Check `git diff --cached
--name-only | grep -E '^\.oracle/|^dispatch/'` before any commit.

## Branch / PR (GitHub)

- Branch: `feat/m177-runner-control-plane-parity`
- PR: none
- CI: **has never run this branch.** Everything green below is local.

## Running Processes

No tmux sessions. Compose datastores up in the worktree — postgres, redis,
qstash. `make down` stops them.

```bash
cd /Users/kishore/Projects/agentsfleet-m177-runner-plane
make test-integration-rustd                     # drops schemas + flushes redis first
KEEP_TEST_STATE=1 make test-integration-rustd   # inner loop — SEE RISKS, it breaks migrate tests
```

To run one lease suite directly (the lane's own URLs, port varies per compose):

```bash
cd rustd
TEST_DATABASE_URL="postgres://agentsfleet:agentsfleet@localhost:$(docker compose port postgres 5432 | sed 's/.*://')/agentsfleetdb?sslmode=disable" \
TEST_REDIS_URL="rediss://:agentsfleet@localhost:$(docker compose port redis 6379 | sed 's/.*://')" \
TEST_REDIS_CA_CERT="$PWD/../.tmp/redis-ca.crt" \
cargo test -p afd_fleet --features test-util --test integration_lease_assign -- --ignored
```

## Tests / Checks

| Gate | Command | Result |
|---|---|---|
| CONFORM | `make harness-verify` | ✅ ALL GATES GREEN (67 staged files) |
| lint | `cargo clippy --workspace --all-features --all-targets` | ✅ clean, full deny set |
| fmt | `cargo fmt --all -- --check` | ✅ |
| unit | `make test-unit-all` | ✅ **All unit lanes passed** (101 Rust suites + every TS coverage gate) |
| integration | `make test-integration-rustd` | ⏳ **IN FLIGHT at handoff — verify first** |
| version | `make check-version` | not re-run this session |
| S5 secrets | `gitleaks detect` | ✅ no leaks (4,678 commits) |
| S6 oversize | FLL sweep over the diff | ✅ clean on every `.rs` |

§2's own suites, run directly and green: `integration_lease_affinity` (4),
`integration_lease_assign` (3), `integration_lease_issue` (1).

## What §2 built

```
src/sql/lease.rs     6 statements, ported verbatim + LeaseRow (23-param binder)
src/sql/event.rs     INSERT_FLEET_EVENT (hot-path write 1)
src/lease/store.rs   Leases — owns pool + queue + entropy, all private
src/lease/affinity.rs  claim / release / reset_meters + the Fence newtype
src/lease/reclaim.rs   expire a dead holder, take back its event
src/lease/assign.rs    readiness peek → candidates → claim → reclaim-or-fresh
src/lease/envelope.rs  Acquired + Kind; parses the producer's contract ONCE
src/lease/issue.rs     the lease row, its audit row, the lifetime tally
src/lease/event.rs     record_received — opens the narrative log
src/error/{mod,detail}.rs   split: what went wrong vs what the caller is told
```

Design notes worth not re-deriving:

- **`Leases` owns BOTH datastores.** A lease is the one verb that cannot be
  served from either alone. Splitting them would let a caller take a claim
  without being able to read the event it claimed for.
- **`pool()` and `queue()` are `pub(crate)`.** Nothing outside the crate can run
  a statement that is not in `sql/` — that property is what makes the
  side-by-side parity read meaningful (Invariant 5).
- **`select()` returns `Result`, where Zig swallows.** Zig's `assign.select`
  catches every error and answers null, so a Redis outage silently reads as "no
  work". Porting that literally violates FN-RS. The store propagates; the
  HANDLER will convert a fault to Zig's observable 200-no-work, visibly.
- **`Acquired.workspace_id` is a `Uuid7`, `event_created_at` a `UnixMillis`.**
  Parsed once at the boundary. Do not re-add downstream `Uuid7::parse` calls.

## Next Steps

1. **Confirm the integration lane finished green** — it was still running at
   handoff. `grep -E 'test result: FAILED|Error 101' /tmp/integ.log`.
2. **§5 — `afd_fleet_runtime` config parser.** Port
   `fleet_runtime/config_parser.zig` (299 lines) + `config.zig` (65) against the
   committed corpus. Dimensions 5.1/5.2. This unblocks everything below.
3. **§2 money half**, in the worker's order: `record_received` (built) → publish
   `event_received` → balance gate → **budget gate** → receive debit
   (first-delivery only) → approval gate → run-estimate debit → secrets_map →
   `fleet_sessions` upsert → `issue` (built). Unported dependencies, ~1,700 Zig
   lines: `state/tenant_provider.zig` (324), `fleet/budget.zig` (283),
   `fleet/approval_gate.zig` (289), `fleet_runtime/metering.zig` (273),
   `fleet/fleet_session.zig` (172), `fleet/secrets_resolve.zig` (160),
   `fleet/service_execution_policy.zig` (104), `state/tenant_billing.zig` (97).
4. Wire the lease verb's router arm — `afd_api::router::runner_handler`, the
   match is total so the unported arms are already listed.
5. §3 report — serial after §2, same agent (spec's B1 batch).

## Risks / Gotchas

- **CI has never run this branch.**
- **`KEEP_TEST_STATE=1` breaks `test_migrate_applies_and_reports_success`** — it
  skips the schema reset a migration test needs. Cost an hour of false
  diagnosis. Use it only for a single `-p afd_fleet` suite, never the full lane.
- **`test_sigterm_stops_a_serving_daemon` is a pre-existing flake** — 20s
  `/readyz` timeout, while `test_sigint_…` passes immediately before it with
  identical shape. Observed before any of this session's edits.
- **Redis is NOT per-test.** The readiness index is one hash at a fixed key and
  streams are keyed by fleet, so every lease test derives fleet ids from
  `process::id()` + a counter. A constant fleet id inherits the previous run's
  stream entries — this cost a debugging cycle. `Fixtures::create()` is now
  Postgres-only; use `create_with_queue()` only where the queue is read, because
  connecting one per test stormed TLS handshakes and timed out four §1 tests.
- **Do NOT re-run `git add -A` without re-checking the pack files** (above).
- **The FLL cap bites constantly** — five files crossed 350 this session. Check
  `wc -l` before adding to `assign.rs`, `error/mod.rs`, `fleet_fixtures.rs`.
- **`make harness-verify` runs NO length audit.** FLL is agent-enforced per edit
  plus rubric row S6; a green harness says nothing about file length.
- **`LOGGING_STANDARD.md` does not cover Rust** (§1 scope lists Zig/TS/JS/shell;
  `logging.sh` scans `src/` + `agentsfleet/src/`, not `rustd/`). A whole module
  family landed with zero emits and no gate fired. A paste-ready prompt to fix
  the orly pack was handed to Indy this session — not yet actioned.
- **M181 §5 is new and unbuilt**: the Rust daemon exports no telemetry at all.
  `agentsfleetd` does not depend on `afd_observability`, and `OTLP_EXPORT` is
  spawned only as a stub in `tests/daemon.rs`. M181's Dimension 4.3
  (`test_metric_continuity`) cannot pass until it is built.
- **SQL parity has no mechanical gate.** REVIEW's side-by-side read is the only
  enforcement — which is why every statement is collected in `src/sql/` and
  copied, never re-derived.
- **`aws-lc-sys` proven on macOS/arm64 only.** musl cross-compile unknown
  (M181's to prove).
- **S6 red for pre-existing reasons** — `Cargo.toml` 420 / `Cargo.lock` 2693,
  both already over at `origin/main`. Indy's call whether S6 excludes manifests.

## Open questions for Indy

1. **Commit the staged §1+§2 slice?** Still unasked, now 76 files.
2. `~/Projects/docs` branch for the OIDC boot-behaviour change — an operator who
   upgrades without the four knobs gets a daemon that refuses to start. Indy
   said he would decide; nothing written.
3. Should `MAX_READY_CANDIDATES_PER_POLL` (64) become an operator knob? Analysis
   done: `HRANDFIELD` samples uniformly so it does not starve, and no Rust crate
   changes the trade-off. Making it configurable diverges from `constants.zig`.
4. Cloudflare execution model — broker or second push plane (M182 now specced).
5. Can a tenant *require* an isolation class, or only be told which one it got?
