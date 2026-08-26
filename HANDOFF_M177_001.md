# Handoff — M177_001 runner control plane · §2 minus the orchestrator

Ephemeral. Delete at CHORE(close). Replaces the prior handoff.

## Scope / Status

Porting `/v1/runners` to `agentsfleetd-rs`. Spec
`docs/v2/active/M177_001_P0_API_RUNNER_CONTROL_PLANE_PARITY.md`, IN_PROGRESS.

- ✅ CHORE(open), crypto/dependency foundation, **§1 complete**
- ✅ **§5 complete** — the stored-config parser AND, as of this session, the
  reader that feeds it
- ✅ **§2 every part** — claim/fence/reclaim, selection pass, lease row, money
  gates, provider resolution, `secrets_map`, approval gate (read + write +
  pass), lease network rules, context budget, endpoint threading,
  `ExecutionPolicy` assembly, the claim read, the terminal refusal write, the
  repair-branch naming
- 🔴 **§2's ORCHESTRATOR does not exist.** Every piece it calls does. This is
  the whole of what is left in §2 — see Next Steps.
- 🚩 **§2 issue-time run debit — NOT BUILT, by Indy's decision.** See Decisions.
- ⏳ §3 report, §4 activity/memory/bundles/mint, §6 sweepers, §7 harness

**No blockers.**

## Working Tree

Worktree `/Users/kishore/Projects/agentsfleet-m177-runner-plane`.

**Clean.** `git status -sb` shows no modified paths.

**35 commits ahead of `origin/main`, NONE PUSHED.** 223 files,
+25,668/−361.

## Branch / PR

- Branch: `feat/m177-runner-control-plane-parity` (GitHub forge)
- **PR: none. CI has never run this branch.** Every result below is local.

## Running Processes

**No tmux server.** The daemon runs detached.

```bash
kill 68828                                     # the live daemon
curl -s localhost:3000/readyz                  # {"ready":true,"database":true,"queue":true}
tail -f .tmp/rustd-serve.log
source .tmp/rustd-env.sh                       # before serve or migrate
AGENTSFLEET_LOG_LEVEL=debug ./rustd/target/debug/agentsfleetd serve
```

The port is **3000**. Nothing in `.tmp/rustd-env.sh` sets one; an older handoff
said 28990, which was a flag that session passed by hand.

Datastores up (docker compose, this worktree's namespace):
postgres **28979** · redis **28980** · qstash **28981**. Brought up with
`make _ensure-test-infra`; ports derive from the compose project name so linked
worktrees cannot collide.

**⚠ `.tmp/rustd-env.sh` is NOT a deployment template.** It points
`DATABASE_URL_MIGRATOR` at the same URL as `DATABASE_URL_API`, collapsing a
privilege split Postgres enforces (`schema/110`: `db_migrator` holds DDL,
`api_runtime` DML only, and the migrator needs a SESSION-pooled endpoint
because advisory locks are session-scoped).

## Routes actually served

`mounted=5 tabled=81`, and the daemon now says so in a log line at boot.

`GET /healthz`, `GET /readyz`, `GET /v1/runners/me`, `POST …/heartbeats`,
`POST /v1/runners`. Every other `RunnerRoute` is tabled with template, guard
and scope but `handler_for` answers `None`, so the binary returns 404 rather
than claiming an unfinished endpoint. `POST /v1/runners/me/leases` → 404 is
that, by design.

## Tests / Checks

| Gate | Command | Result |
|---|---|---|
| CONFORM | `make harness-verify` | ✅ ALL GATES GREEN (**staged scope — stage first**) |
| LOGGING | `bash audits/logging.sh` | ✅ CLEAN — `rust-direct=0` |
| lint + fmt | `cargo clippy --workspace --all-features --all-targets` | ✅ clean |
| workspace | `cargo test --workspace --all-features` | ✅ **820 passed, 0 failed, 81 ignored** — measured at this handoff, on `79bbcf646` |
| afd_fleet | `--lib` | ✅ 178 (109 at stream start) |
| afd_fleet_runtime | `--lib` | ✅ 49 |
| afd_crypto | all targets | ✅ green, known-answer tests included |
| **claim read** | `--test integration_lease_installed -- --ignored` | ✅ **4 passed, live Postgres** |
| **refusal write** | `--test integration_lease_block -- --ignored` | ✅ **3 passed, live Postgres** |
| boot | `migrate` then `serve` on compose | ✅ ready, probes green, logs flowing |
| **unit (repo)** | `make test-unit-all` | ⏳ **deferred by Indy — once, at the end** |
| **integration** | `make test-integration-rustd` | ⏳ **deferred by Indy — same** |

`cargo test --workspace` is the RUST half only. It is NOT `make test-unit-all`,
which also carries every TypeScript coverage gate, so it is not a
repository-wide "tests pass" claim.

To run ONE integration suite without the whole deferred lane:

```bash
TEST_DATABASE_URL="postgres://agentsfleet:agentsfleet@localhost:28979/agentsfleetdb?sslmode=disable" \
TEST_REDIS_URL="rediss://:agentsfleet@localhost:28980" \
TEST_REDIS_CA_CERT="$PWD/.tmp/redis-ca.crt" \
cargo test -p afd_fleet --all-features --test <suite> -- --ignored
```

## Next Steps

1. **The lease ORCHESTRATOR** (`afd_fleet`) — the last of §2. Compose, in the
   worker's order:
   `select` → `installed` → `record_received` → `payer` → `Providers::resolve`
   → `money_gates` → `Gates::check` → `Vault::declared` →
   `policy::build::assemble` → `Leases::issue`.
   Map the outcomes: `Admission::Refuse` calls `Leases::block` then answers
   no-work; `Retry`/`Await` answer no-work with `retry_after_ms` and write
   nothing; `Assembled::Ungranted` parks. **Two prerequisites are still
   missing, both plain SELECTs:**
   - **the `core.integration_grants` reader** feeding `policy::grants::Grants`.
     Without it every mintable credential parks. Table is
     `(id, fleet_id, service, status, …)`, unique on `(fleet_id, service)`.
     `Grants::none()` is the fail-closed placeholder, NOT the answer.
   - **`approvedWriteGateId`** — which approved write gate a lease's repair
     branch names. `policy::repair::branch_for` turns that id into the branch;
     `egress::build` refuses a write binding without one, so write-bound fleets
     cannot lease until this lands.
   Known duplicate read to decide on: `money_gates` resolves the payer
   internally, so the orchestrator calling `Accounts::payer` first (needed for
   provider resolution) costs one extra indexed single-row lookup per lease.
   Left as-is so ONE place decides what an unowned workspace means; worth
   revisiting by passing `tenant_id` into `Request`.
2. **The handler + router arm** (`afd_api`). Thin — the module doc is explicit
   that the decision is `afd_fleet`'s and the answer is this crate's. Follow
   `handler/runner/heartbeat.rs`. Needs a `Services::leases()` beside
   `runners()`, and the `RunnerRoute::Lease` arm in `router::handler_for`.
3. **`ServingPlane` grows its remaining seams** — `Leases`, `Gates`, `Vault`,
   `Providers`, `Accounts`. **This is where the KEK lands.** `preflight`
   already reads and validates `ENCRYPTION_MASTER_KEY` into `BootConfig::kek()`
   and refuses boot without it; the gap is only that it never reaches
   `plane.rs`, so nothing constructs the `Vault::new(database, Arc<Kek>)` the
   credential path needs.
4. **§3 report** — serial after §2.

**The wire types are DONE.** `afd_wire::lease::{LeaseRequest, LeasePayload,
LeaseResponse, SecretDelivery, BundleManifest}` exist and are pinned
byte-for-byte against `samples/fixtures/wire-v2/` by
`afd_wire/tests/roundtrip.rs`. Do not write new ones.

## M183_001 rides in this PR — do NOT treat it as stray

Another agent is authoring a spec **M183_001**, and Indy has said it belongs in
THIS branch's Pull Request. As of this handoff it has not landed in this
worktree — `find . -iname '*M183*'` returns only a build artifact whose name
coincides.

When it appears:

- It is **expected**, not dirty. Do not revert it, do not move it to its own
  branch, and do not omit it from the PR body.
- It is a SPEC document, so it is `docs/`-only and touches no Rust. If a
  commit under that name changes `rustd/`, that is worth raising before
  building on it.
- `make harness-verify`'s SPEC TEMPLATE gate grades spec files. Stage it and
  run the gate before committing, the same as any other change.
- The PR body must describe both streams. M177_001 is the runner control-plane
  port; M183_001 is the other agent's — read its Overview rather than
  paraphrasing it from its title.

## Risks / Gotchas

- **CI has never run this branch.** 35 commits, unpushed, no PR. Biggest risk
  here by a distance.
- **`make harness-verify` is STAGED-SCOPE.** With nothing staged every row
  reads "no source files in scope" and the summary still says ALL GATES GREEN.
  Read the file COUNT on the UFS row. `git add` first, always.
- **`make harness-verify` does NOT check file length.** Rubric S6 (350 lines)
  is the real check and it bites: `policy/build.rs` reached 436 this session
  and had to be split into `build.rs` + `grants.rs` + a shared
  `policy/fixture.rs`. **`claim.rs` sits at 349 with no headroom — split it
  before adding anything.**
- The UFS gate flags numeric literals inside `#[cfg(test)]`. Name fixture
  numbers, or express them relative to the constant they are about.
- The MS-ID gate flags milestone markers in source, including inside doc
  comments. Name the design, not the milestone.
- **The credit metric is a SEAM.** `debit_receive` returns `Deducted(Nanos)`
  rather than metering inline. If §6/M181 never attaches the instrument, the
  metric silently never fires and no test in either milestone catches it.
- `secrets_map` holds live tenant credentials un-wiped — bound for the wire,
  and a `serde_json::Value` tree has nowhere to put a destructor. What IS
  defended is the realistic leak: `Declared` has a hand-written `Debug`.
- **`KEEP_TEST_STATE=1` breaks `test_migrate_applies_and_reports_success`.**
  Single-suite use only.
- **Redis is NOT per-test.** Lease tests derive fleet ids from `process::id()`
  plus a counter; a constant id inherits the previous run's stream entries.
- `aws-lc-sys` proven on macOS/arm64 only. musl cross-compile is M181's.
- **`UZ-RUN-015` must answer 402.** The stock runner classifies a renew refusal
  by BOTH status and code — `control_plane_client_test.zig:42` pins that the
  same code on any other terminal status is NOT a budget breach.

## Decisions taken (do not re-open)

1. **SSRF endpoint refusal is PERMANENT**, diverging from the Zig's
   `resolveTenant`, which re-polls forever with no terminal row (Indy).
2. **Write 5 (`UPSERT core.fleet_sessions SET execution_id`) — NOT PORTED**,
   and neither is `CLEAR_STALE_EXECUTION`. `execution_id` has zero production
   writers of a value, zero production readers, and one production statement
   that only ever sets NULL behind an `IS NOT NULL` guard — a permanent no-op.
   It duplicates `fleet.runner_leases` without the fence or TTL that make that
   row trustworthy. The claim read ported this session deliberately omits it.
   Non-blocking follow-up: correct `data_flow.md` to name `fleet.runner_leases`
   as the source for "executing right now".
3. **The issue-time run-estimate debit — NOT IMPLEMENTED (Indy).** Spec §2 says
   build it; `docs/architecture/billing_and_provider_keys.md` — the document
   that owns billing — contradicts it and names M80_010 as having replaced the
   one-shot estimate. The estimate also has nowhere to land: `balance_nanos` is
   read-only across `afd_fleet/src/sql/`, `stage` belongs to the renewal
   accumulator whose `… = … + EXCLUDED.…` would DOUBLE CHARGE, and a third
   charge type breaks the two-rows-per-event invariant. **Correct the spec and
   `data_flow.md` §C rather than building it.**
4. **Two dead Redis writes are NOT ported** — `fleet:gate:pending:` and
   `fleet:gate:notify:`, the latter with no reader anywhere in the repo.
5. **No hand-written log wire format (Indy).** Logs, metrics and traces leave
   through an OTLP collector bound for Elastic and Grafana, so §3 logfmt is the
   collector's business. The daemon installs the STOCK `tracing_subscriber::fmt`
   on stderr, ANSI only on a terminal, level from `AGENTSFLEET_LOG_LEVEL`. A
   §3 logfmt `FormatEvent` was written and discarded; re-derive rather than
   restore if it is ever wanted.

## Standing instruction from Indy — no Zig regressions

**Do not port the Zig's SHAPE where it was a workaround for a missing library.**
The diagnostic question for any ported leaf function: *what could the Zig not
reach for, and does this workspace already depend on something that does it?*

Four confirmed instances, all fixed:

- `egress` — `is_misconfiguration` string-matched `Error::to_string()` against
  three constants to decide end-vs-retry → typed `Misconfigured`.
- `policy/build` — the grant set as `&[Box<str>]` with a nested linear scan,
  where `&[]` spelled BOTH "granted nothing" and "never read the grants" →
  `Grants` with no zero-argument constructor.
- `afd_core::id::Uuid7::to_bytes` — a hand-rolled hex nibble decoder written 20
  lines below `Uuid7::encode`, whose own doc says not to → `uuid::parse_str`.
- `afd_crypto::secret::decode_hex_into` — a hand-rolled hex decoder on the
  master-key parse path → `hex::decode_to_slice`.

**The audit is targeted, not exhaustive.** Parsing and serialization surfaces
have NOT been swept. A prompt for scoping that spec was written this session
and is in the conversation log, not on disk — ask Indy for it.

**Method, when replacing:** pin the existing behaviour with tests FIRST,
against the hand-rolled code, and only then swap. The crypto swap proved why —
every pre-existing hex test used the wrong LENGTH, so the digit check had zero
coverage and uppercase acceptance was unasserted.

## Corrections made this session

- **The prior handoff overstated §5**, which read "claim-time half ✅". The
  PARSER existed; nothing in `rustd/` read `core.fleets.config_json`. Do not
  assume a ✅ means a caller exists.
- **The daemon installed no `tracing` subscriber**, so all 97 emits across
  eight crates were no-ops that did not even evaluate their fields. A full boot
  produced one line — the banner, which is `println!`. The logging audit passed
  throughout because it checks call SHAPE, not that anything listens.
- **The `ExecutionPolicy` assembler was reworked before landing** on the
  no-Zig-regression instruction; the borrow checker then showed the grant set
  shares no lifetime with the policy, so it left `Inputs` and became its own
  argument.
- **The park path had NO coverage** despite being the assembler's headline
  behaviour. It now has three cases, including a partial grant set.
- **Three test modules each rebuilt the same fleet-config fixture** →
  `policy/fixture.rs`.
- **`AGENTSFLEET_LOG` renamed `AGENTSFLEET_LOG_LEVEL`** so the name says which
  knob it is. It formally collides with §7's reserved `AGENTSFLEET_LOG_<SCOPE>`
  family for a scope named `level`; no such scope exists and that override is
  the Zig binding only.
