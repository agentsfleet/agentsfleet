# Handoff — M177_001 runner control plane · §5 done, §2's money half next

Ephemeral. Delete at CHORE(close). Replaces the prior handoff.

## Scope / Status

Porting `/v1/runners` to `agentsfleetd-rs`. Spec
`docs/v2/active/M177_001_P0_API_RUNNER_CONTROL_PLANE_PARITY.md`, IN_PROGRESS.

- ✅ CHORE(open), crypto/dependency foundation, **§1 complete**
- ✅ **§2 assignment half** — claim, fence, reclaim, selection pass, lease row,
  hot-path write 1. Eight integration tests green.
- ✅ **§5 claim-time half** — `afd_fleet_runtime`: stored `config_json` →
  typed `FleetConfig`. This is what unblocked the rest of §2.
- ⏳ **§2 money half — NEXT, and no longer blocked.**
- ⏳ **§5's install half** — `parseTriggerMarkdownWithJson` (YAML frontmatter →
  `config_json`) is NOT ported. See "What §5 deliberately left".
- ⏳ §3 report, §4 activity/memory/bundles/mint, §6 sweepers, §7 harness

### Decisions this session (Indy's, not re-litigable)

1. **garde over `validator`** for schema bounds, against the download numbers.
   `validator` has ~17× the downloads and is maintained, but its rules are
   `length, range, email, url, regex, contains, must_match, required, ip,
   cards, non_control_character` — `length` bounds a COLLECTION and there is no
   per-element rule. This schema is mostly `Vec<String>` needing per-ENTRY
   bounds, so `validator` means a hand-written loop per list. garde's
   `inner(...)` does it declaratively. Trade recorded in the commit body.
2. **`jira-core` / `lineark-sdk` deferred.** Both are real and maintained
   (2.8.4 / 3.1.0 — better than the `0.0.1` crates a naive search finds), but
   both pull `ring 0.17.14`, which `e5aef4ccf` removed. Cost is one
   `CryptoProvider::install_default()` at boot plus two crypto stacks in the
   binary. M177 needs neither: its whole provider surface is three webhook
   signature schemes.
3. **reqwest stays at 0.13.** ring-vs-aws-lc-rs is a *rustls feature* choice,
   not a reqwest version choice; reverting buys nothing. Default feature sets
   are byte-identical between 0.12.24 and 0.13.4 — there is no perf or
   concurrency story in that major, and none was claimed.
4. **orly 0.7.0 pack committed into this PR**, breaking the prior handoff's
   "never commit the pack" rule. Indy's call, on the expectation that 0.7.1
   lands the logging-gate fix shortly.
5. **The four §5 error-class divergences are implemented, not absorbed** — see
   `afd_fleet_runtime/src/error.rs`, which is where the register lives.

## Working Tree

Worktree `/Users/kishore/Projects/agentsfleet-m177-runner-plane`.
**Tree is CLEAN.** Nothing staged, nothing modified, nothing untracked.

```
## feat/m177-runner-control-plane-parity
42dcbee78 fleet config resolves — and every Rust emit says what it is   ← this session
feec3292e the runner plane answers — guard, heartbeat, and the lease claim
6469d2b3c docs(m182): add spec — execution substrate abstraction
e5aef4ccf one crypto provider — aws-lc-rs replaces ring, reqwest 0.13
5f7beed8b afd_fleet — the runner row, enrolment, and the verdict
87ee0c886 chore(m177): open the runner control-plane parity stream
```

**6 commits, none pushed.** The prior handoff's warning about `git add -A`
staging the orly pack no longer applies — the pack is committed now (decision 4).

## Branch / PR (GitHub)

- Branch: `feat/m177-runner-control-plane-parity`
- PR: **none**
- CI: **has never run this branch.** Everything below is local.

## Running Processes

No tmux sessions. **The compose datastores are DOWN** — `docker compose ps` is
empty. The integration lane needs them:

```bash
cd /Users/kishore/Projects/agentsfleet-m177-runner-plane
make up                          # start postgres + redis + qstash
make test-integration-rustd      # drops schemas + flushes redis first
```

Ports are assigned per compose run, so re-read them rather than reusing the
prior handoff's numbers:

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
| CONFORM | `make harness-verify` | ✅ ALL GATES GREEN, 45 staged files |
| lint | `cargo clippy --workspace --all-features --all-targets` | ✅ clean, full deny set |
| fmt | `cargo fmt --all -- --check` | ✅ |
| unit (crate) | `cargo test -p afd_fleet_runtime` | ✅ 37 passed |
| unit (repo) | `make test-unit-all` | ⚠️ **passed BEFORE the final `cli.rs` edit; not re-run since** |
| integration | `make test-integration-rustd` | ❌ **never run against either new commit** |
| version | `make check-version` | not run this session |
| whole-repo sweep | `make harness-verify-all` | ❌ **red on 6 known false positives** (below) |

## What §5 built

```
afd_fleet_runtime/  15 files, 2813 lines, longest 329 (FLL cap is 350)
  src/error.rs          one type + Result alias; the divergence register
  src/name.rs           FleetName / CredentialName / Version newtypes
  src/provider.rs       WebhookProvider + ProviderRegistry traits, 3 schemes
  src/config/mod.rs     FleetConfig + the parse entry points
  src/config/raw/       the serde+garde schema, split 6 ways
  src/config/{trigger,gates,policy,repository}.rs   the domain half
```

Three stages, and only the third is written by hand: **serde** does shape,
**garde** does bounds, this crate does meaning. Design notes worth not
re-deriving:

- **Every schema field is an `Option`.** Not style — it is the mechanism that
  keeps "you forgot a key" and "you got the shape wrong" two different answers.
  serde is never asked for a required field, so it cannot raise "missing
  field", which leaves a deserialize failure only ever a SHAPE failure.
  Requiredness is decided one layer up. Do not "tidy" these into non-Options.
- **`FleetConfig::stored_shared()` returns `Arc`.** The Zig re-parses the whole
  config on every mint request (`credentials_mint_scope.zig:66`) just to read
  `repository_binding`. Parse once, share.
- **Three bounds sit outside garde on purpose**: the newtypes (garde validates
  in place and cannot PRODUCE a proof-carrying type), the trigger set's arity
  (garde refuses `dive` + `inner` together, so it sits beside its uniqueness
  rule), and the numeric caps (`Dollars` also refuses non-finite, which a range
  annotation cannot express).
- **No `UZ-` code is declared here** (RULE ERR). The wire code is the existing
  `UZ-AGT-008`; the mapping belongs at the HTTP boundary.

### What §5 deliberately left

The **install-time** path — `parseTriggerMarkdownWithJson`, i.e.
`config_markdown.zig` (338) + `yaml_frontmatter.zig` (272). It is called only
from create/patch/importer/Slack handlers, which are TENANT routes M177 does
not own. `fleet_session.zig:129` proves the claim path needs only
`parseStoredFleetConfig` + `extractFleetInstructions`. Spec §5's Dimension 5.1
corpus test spans both halves, so **5.1 cannot be closed until the install half
lands** — likely M178/M179. Flag this to Indy rather than quietly narrowing the
dimension.

The YAML crate decision (`serde_norway`) is therefore still open and belongs
with that half.

## Next Steps

1. **`make up`** — the datastores are down.
2. **Re-run `make test-unit-all` and `make test-integration-rustd`** against
   `42dcbee78`. Neither has run against this commit; the integration lane has
   never run against `feec3292e` either.
3. **§2 money half**, in the worker's order: `record_received` (built) →
   publish `event_received` → balance gate → budget gate → receive debit
   (first-delivery only) → approval gate → run-estimate debit → secrets_map →
   `fleet_sessions` upsert → `issue` (built). Unported Zig, ~1,700 lines:
   `state/tenant_provider.zig` (324), `fleet/budget.zig` (283),
   `fleet/approval_gate.zig` (289), `fleet_runtime/metering.zig` (273),
   `fleet/fleet_session.zig` (172), `fleet/secrets_resolve.zig` (160),
   `fleet/service_execution_policy.zig` (104), `state/tenant_billing.zig` (97).
   Both money gates fail OPEN on a datastore fault — a metering outage must not
   halt every fleet on the platform.
4. Wire the lease verb's router arm — `afd_api::router::runner_handler`.
5. §3 report — serial after §2, same agent (spec's B1 batch).

## Risks / Gotchas

- **CI has never run this branch.** Six commits, unpushed, no PR.
- **`make harness-verify-all` is RED on 6 known false positives** and will stay
  red until orly 0.7.1. Four are `build.rs` lines where stdout is cargo's IPC
  channel; two are `banner.rs` / `fatal.rs`, where writing to a stream IS the
  design. `PROMPT_ORLY_LOGGING_GATE_RUST.md` carries the fix for the next agent
  on the orly engine. **Do not "fix" these in this repository** — converting
  `build.rs` breaks the build silently, and `audits/logging.sh` is orly-managed,
  so a local edit is erased by `orly update` and flagged by `orly doctor`.
- **A green `harness-verify` row can mean "scanned nothing".** With an empty
  index every audit reports `OK: no source files in scope (--staged)` — it is a
  pre-commit lens over `git diff --cached`, never the repository. This nearly
  hid 65 real logging violations. Read the row's file COUNT, not just its tick.
- **`KEEP_TEST_STATE=1` breaks `test_migrate_applies_and_reports_success`** — it
  skips the schema reset a migration test needs. Use it only for a single
  `-p afd_fleet` suite, never the full lane.
- **`test_sigterm_stops_a_serving_daemon` is a pre-existing flake** — 20s
  `/readyz` timeout. It passed on the last full lane run.
- **Redis is NOT per-test.** The readiness index is one hash at a fixed key and
  streams are keyed by fleet, so lease tests derive fleet ids from
  `process::id()` + a counter. A constant id inherits the previous run's stream
  entries. `Fixtures::create()` is Postgres-only; use `create_with_queue()` only
  where the queue is read — connecting one per test stormed TLS handshakes and
  timed out four §1 tests.
- **`agentsfleetd migrate` no longer prints to stdout.** Its summary is now the
  `migrate_completed` event. The command's contract is its EXIT STATUS, which
  the lane asserts and which did not change — but anything scraping that line
  outside the repository would break.
- **The FLL cap bites constantly.** `raw.rs` hit 515 and was split six ways.
  Check `wc -l` before adding to `config/gates.rs` (329) or `config/mod.rs` (321).
- **SQL parity has no mechanical gate.** REVIEW's side-by-side read is the only
  enforcement — which is why every statement is collected in `src/sql/`.
- **`aws-lc-sys` proven on macOS/arm64 only.** musl cross-compile unknown
  (M181's to prove).
- **M181 §5 is still unbuilt**: the Rust daemon exports no telemetry.
  `agentsfleetd` does not depend on `afd_observability`.

## Open questions for Indy

1. **Spec §5's Dimension 5.1 spans both halves of §5** and only the claim-time
   half is built. Split the dimension, or leave §5 open until the install half
   lands in M178/M179?
2. `MAX_READY_CANDIDATES_PER_POLL` (64) as an operator knob? Analysis says no —
   `HRANDFIELD` samples uniformly so it cannot starve, and making it
   configurable diverges from `constants.zig`.
3. Can a tenant *require* an isolation class, or only be told which one it got?
   Recommendation: told, for M177 — a `require` is a scheduling constraint that
   must fail closed, which is a new refusal path the Zig has no counterpart for.
