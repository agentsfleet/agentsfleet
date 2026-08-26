# Handoff — M177_001 runner control plane · §2's policy half, a live daemon, and the claim read

Ephemeral. Delete at CHORE(close). Replaces the prior handoff.

## Scope / Status

Porting `/v1/runners` to `agentsfleetd-rs`. Spec
`docs/v2/active/M177_001_P0_API_RUNNER_CONTROL_PLANE_PARITY.md`, IN_PROGRESS.

- ✅ CHORE(open), crypto/dependency foundation, **§1 complete**
- ✅ **§2 assignment half** — claim, fence, reclaim, selection pass, lease row
- ✅ **§2 money gates**, **provider resolution**, **`secrets_map`**,
  **approval gate** (read + write + pass, `Admission::Await`)
- ✅ **§2 lease network rules**, **context budget**, **endpoint threading**
- ✅ **§2 `ExecutionPolicy` assembly** — reworked and COMMITTED this session
- ✅ **the tracing subscriber** — the daemon is observable for the first time
- ✅ **the claim read** — `SELECT_FLEET_WITH_SESSION`, this session
- 🚩 **§2 issue-time run debit — NOT BUILT, by Indy's decision.** See Decisions.
- ⏳ **§2 remainder** — the lease ORCHESTRATOR and HANDLER. See Next Steps.
- ⏳ §3 report, §4 activity/memory/bundles/mint, §6 sweepers, §7 harness

**No blockers.**

## Working Tree

Worktree `/Users/kishore/Projects/agentsfleet-m177-runner-plane`.

**Clean except this file. 31 commits ahead of `origin/main`, NONE PUSHED.**

## Branch / PR

- Branch: `feat/m177-runner-control-plane-parity` (GitHub forge)
- **PR: none. CI has never run this branch.** Every gate result below is local.

## Running Processes

`agentsfleetd-rs` is live: `0.0.0.0:3000`, `{"ready":true,"database":true,"queue":true}`.
Log `.tmp/rustd-serve.log`; env `.tmp/rustd-env.sh` (source before serve/migrate).
Note the port is **3000**, not 28990 — nothing in the env file sets one, so the
prior handoff's 28990 was a flag that session passed by hand.

Datastores up (compose): postgres 28979 · redis 28980 · qstash 28981.
No tmux server.

**⚠ `.tmp/rustd-env.sh` is NOT a deployment template.** It points
`DATABASE_URL_MIGRATOR` at the same URL as `DATABASE_URL_API`, collapsing a
privilege split Postgres enforces (`schema/110`).

## Routes actually served

`mounted=5 tabled=81` — and the daemon now SAYS so, at boot, in a log line.

`GET /healthz`, `GET /readyz`, `GET /v1/runners/me`, `POST …/heartbeats`,
`POST /v1/runners`. Everything else is tabled with template, guard and scope and
returns `None`, so the binary answers 404 rather than claiming an unfinished
endpoint. `POST /v1/runners/me/leases` → 404 is that, by design.

## Tests / Checks

| Gate | Command | Result |
|---|---|---|
| CONFORM | `make harness-verify` | ✅ ALL GATES GREEN (**staged scope — stage first**) |
| LOGGING | `bash audits/logging.sh` | ✅ CLEAN — `rust-direct=0` |
| lint + fmt | `cargo clippy --workspace --all-features --all-targets` | ✅ clean |
| workspace | `cargo test --workspace --all-features` | ✅ **805 passed, 0 failed, 74 ignored** |
| afd_fleet | `--lib` | ✅ 173 (was 109 at stream start) |
| afd_fleet_runtime | `--lib` | ✅ 49 (was 42) |
| **claim read** | `--test integration_lease_installed -- --ignored` | ✅ **4 passed, live Postgres** |
| boot | `migrate` then `serve` on compose | ✅ ready, probes green, logs flowing |
| **unit (repo)** | `make test-unit-all` | ⏳ **deferred by Indy — once, at the end** |
| **integration** | `make test-integration-rustd` | ⏳ **deferred by Indy — same** |

`cargo test --workspace` is the RUST half only. It is not `make test-unit-all`,
which also carries the TypeScript coverage gates, so it is not a
repository-wide "tests pass" claim.

To run one integration suite without the whole lane:
```
TEST_DATABASE_URL="postgres://agentsfleet:agentsfleet@localhost:28979/agentsfleetdb?sslmode=disable" \
TEST_REDIS_URL="rediss://:agentsfleet@localhost:28980" \
TEST_REDIS_CA_CERT="$PWD/.tmp/redis-ca.crt" \
cargo test -p afd_fleet --all-features --test <suite> -- --ignored
```

## Next Steps

1. **The lease ORCHESTRATOR** (`afd_fleet`). Composes, in the worker's order:
   `assign::select` → `installed` → `event::record_received` → `money_gates` →
   `Gates::check` → `policy::build::assemble` → `Leases::issue`. **Every one of
   those now exists.** It must map four non-`Admit` outcomes onto the wire:
   `Refuse` writes the terminal row, `Retry`/`Await` answer no-work with
   `retry_after_ms`, and `Assembled::Ungranted` parks.
2. **The handler + router arm** (`afd_api`). Thin, per the module doc — the
   decision is `afd_fleet`'s, the answer is this crate's. Needs a
   `Services::leases()` accessor beside `runners()`.
3. **`ServingPlane` grows the rest of its seams** — `Leases`, `Gates`, `Vault`,
   `Providers`, `Accounts`. **This is where the KEK lands**: `preflight` already
   reads and validates `ENCRYPTION_MASTER_KEY` into `BootConfig::kek()` and
   refuses boot without it; it simply never reaches `plane.rs`, so nothing
   constructs the `Vault::new(database, Arc<Kek>)` the credential path needs.
4. **§3 report** — serial after §2.

The wire types are DONE and need no work: `afd_wire::lease::{LeaseRequest,
LeasePayload, LeaseResponse, SecretDelivery, BundleManifest}` already exist and
are pinned byte-for-byte against `samples/fixtures/wire-v2/` by
`afd_wire/tests/roundtrip.rs`.

## Risks / Gotchas

- **CI has never run this branch.** 31 commits, unpushed, no PR.
- **`make harness-verify` is STAGED-SCOPE.** With nothing staged every row reads
  "no source files in scope" and the summary still says ALL GATES GREEN. Read
  the file COUNT on the UFS row. `git add` first, always.
- **`make harness-verify` does NOT check file length.** Rubric S6 (350) is the
  real check, and it bites: `policy/build.rs` hit 436 this session and had to be
  split. `claim.rs` sits at 349 with no headroom.
- The UFS gate flags numeric literals inside `#[cfg(test)]`; the MS-ID gate
  flags milestone markers in source, including in doc comments.
- **The credit metric is a SEAM.** `debit_receive` returns `Deducted(Nanos)`
  instead of metering inline. If §6/M181 never attaches the instrument, the
  metric silently never fires and no test in either milestone catches it.
- `secrets_map` holds live tenant credentials un-wiped — bound for the wire, and
  a `serde_json::Value` tree has nowhere to put a destructor. What IS defended
  is the realistic leak: `Declared` has a hand-written `Debug`.
- **`KEEP_TEST_STATE=1` breaks `test_migrate_applies_and_reports_success`.**
- **Redis is NOT per-test.** Lease tests derive fleet ids from `process::id()`
  plus a counter; a constant id inherits the previous run's stream entries.
- `aws-lc-sys` proven on macOS/arm64 only. musl cross-compile is M181's.
- **`UZ-RUN-015` must answer 402.** The stock runner classifies a renew refusal
  by BOTH status and code (`control_plane_client_test.zig:42`).

## Decisions taken (do not re-open)

1. **SSRF endpoint refusal is PERMANENT**, diverging from the Zig, per Indy.
2. **Write 5 (`UPSERT core.fleet_sessions SET execution_id`) — NOT PORTED**, and
   neither is `CLEAR_STALE_EXECUTION`. `execution_id` has zero production
   writers of a value, zero production readers, and one production statement
   that only ever sets NULL behind an `IS NOT NULL` guard — a permanent no-op.
   It duplicates `fleet.runner_leases` without the fence or TTL that make that
   row trustworthy. The claim read ported this session deliberately omits it.
   Follow-up (non-blocking): correct `data_flow.md` to name
   `fleet.runner_leases` as the source for "executing right now".
3. **The issue-time run-estimate debit — NOT IMPLEMENTED (Indy).** Spec §2 says
   build it; `docs/architecture/billing_and_provider_keys.md` — the document
   that owns billing — contradicts it and names M80_010 as having replaced the
   one-shot estimate. The estimate also has nowhere to land: `balance_nanos` is
   read-only across `afd_fleet/src/sql/`, `stage` is the renewal accumulator's
   (`… = … + EXCLUDED.…` would DOUBLE CHARGE), and a third charge type breaks
   the two-rows-per-event invariant. **Correct the spec and `data_flow.md` §C
   rather than building it.**
4. **Two dead Redis writes are NOT ported** — `fleet:gate:pending:` and
   `fleet:gate:notify:`, the latter with no reader anywhere in the repo.
5. **No hand-written log wire format.** Logs, metrics and traces leave through
   an OTLP collector bound for Elastic and Grafana (Indy, this session), so §3
   logfmt is the collector's business. The daemon installs the STOCK formatter.
   A §3 logfmt `FormatEvent` was written and discarded; if it is ever wanted it
   is parked outside the repo and should be re-derived rather than restored.

## Corrections made this session

- **The prior handoff overstated §5.** It read "§5 claim-time half — stored
  `config_json` → typed `FleetConfig` ✅". The PARSER existed; **nothing in
  `rustd/` read `core.fleets.config_json`**. `SELECT_FLEET_WITH_SESSION` was
  not ported until this session, and the orchestrator cannot be written without
  it. Do not assume a "✅" means a caller exists.
- **The daemon installed no `tracing` subscriber**, so all 97 emits across eight
  crates were no-ops that did not even evaluate their fields. A full boot
  produced one line — the banner, which is `println!`. Now 5 lines at the
  default level and 11 at `AGENTSFLEET_LOG_LEVEL=debug`. The logging audit had
  been passing the whole time because it checks call SHAPE, not that anything
  listens.
- **The assembler was reworked before landing, on Indy's "no Zig regression"
  instruction.** What read as ported: `approved: &[Box<str>]` with a nested
  linear scan, where `&[]` silently spelled BOTH "granted nothing" and "never
  read the grants" — now a `Grants` set with no zero-argument constructor;
  `first_ungranted` returning the caller's own result enum pre-wrapped — now it
  returns what it found; and a bare `Result` instead of the crate alias. The
  borrow checker then showed the grant set shares no lifetime with the policy,
  so it left `Inputs` entirely and became its own argument.
- **The park path had NO test coverage** before this session despite being the
  assembler's headline behaviour. It now has three, including a partial grant
  set.
- **Three test modules each rebuilt the same fleet-config fixture.** Consolidated
  into `policy/fixture.rs`.
- **`AGENTSFLEET_LOG` was renamed `AGENTSFLEET_LOG_LEVEL`** so the name says
  which knob it is. Note it formally collides with §7's reserved
  `AGENTSFLEET_LOG_<SCOPE>` family for a scope named `level`; no such scope
  exists and that override is the Zig binding only.
