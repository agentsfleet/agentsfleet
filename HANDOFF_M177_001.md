# Handoff — M177_001 runner control plane · §2's key, credentials and gate

Ephemeral. Delete at CHORE(close). Replaces the prior handoff.

## Scope / Status

Porting `/v1/runners` to `agentsfleetd-rs`. Spec
`docs/v2/active/M177_001_P0_API_RUNNER_CONTROL_PLANE_PARITY.md`, IN_PROGRESS.

- ✅ CHORE(open), crypto/dependency foundation, **§1 complete**
- ✅ **§2 assignment half** — claim, fence, reclaim, selection pass, lease row
- ✅ **§5 claim-time half** — stored `config_json` → typed `FleetConfig`
- ✅ **§2 money gates** — payer → balance → fleet budget → receive debit
- ✅ **§2 provider resolution** — two strategies behind one trait object
- ✅ **§2 `secrets_map`** — the two-channel split, disjoint by construction
- ✅ **§2 approval-gate DECISION** — rule match, condition, route table
- ✅ **§2 approval-gate READ side** — recorded reference, mirror-then-durable
  decision, anomaly counter
- ⏳ **§2 remainder — NEXT.** The gate's WRITE side (park, card detail, pause),
  the run-estimate debit, `ExecutionPolicy`. See Next Steps.
- ⏳ §3 report, §4 activity/memory/bundles/mint, §6 sweepers, §7 harness

**No blockers.** Everything below is local and reversible.

## Working Tree

Worktree `/Users/kishore/Projects/agentsfleet-m177-runner-plane`.
**CLEAN** — `git status -sb` shows nothing staged, modified, or untracked.

```
## feat/m177-runner-control-plane-parity
3288bfb92 refactor(core): ask serde for a map, instead of reading the first byte
3fa990ec6 refactor(rustd): delete the hand-rolled parsers Zig had no library for
2c732397e feat(rustd): a parked event resolves on a later poll, not on a waiting thread
e4ae4a734 fix(rustd): an SSRF-refused endpoint ends the event, and only objects are objects
36a656cb7 refactor(rustd): a connector is a descriptor, and a fault is not a posture
e0055ffa5 feat(rustd): the approval gate decides, and the order it decides in is a type
23d546c1d feat(rustd): a fleet's credentials split by channel, not by convention
ffad7c998 feat(rustd): the provider resolves through two strategies, one interface
```

**20 commits ahead of `origin/main`, NONE PUSHED.**

## Branch / PR

- Branch: `feat/m177-runner-control-plane-parity` (GitHub forge)
- **PR: none. `gh pr list --head … --state all` → `[]`.**
- **CI: has never run this branch.** Every gate result below is local.

## Running Processes

**None.** `tmux list-sessions` → no server running. No dev server, no watcher.

**Datastores are DOWN** — `docker compose ps` lists no containers. Bring them up
before any integration work:

```bash
cd /Users/kishore/Projects/agentsfleet-m177-runner-plane && make up
```

Ports are assigned per run. Re-read them from compose; do not reuse old numbers.

## Tests / Checks

| Gate | Command | Result |
|---|---|---|
| CONFORM | `make harness-verify` | ✅ ALL GATES GREEN |
| LOGGING (whole repo) | `bash audits/logging.sh` | ✅ CLEAN — `rust-direct=0` |
| lint | `cargo clippy --workspace --all-features --all-targets` | ✅ clean |
| fmt | `cargo fmt --all -- --check` | ✅ |
| unit | `cargo test -p afd_fleet --lib` | ✅ **109 passed** |
| unit | `cargo test -p afd_fleet_runtime --lib` | ✅ **42 passed** |
| unit | `cargo test -p afd_core` | ✅ incl. registry + problem-table parity |
| unit | `cargo test -p afd_api --all-features` | ✅ |
| unit | `cargo test -p afd_identity --all-features` | ✅ |
| S6 file length | rubric sweep | ✅ clean (known exceptions only) |
| **unit (repo)** | `make test-unit-all` | ⏳ **deferred by Indy — run once at the end** |
| **integration** | `make test-integration-rustd` | ⏳ **deferred by Indy — same** |

> Indy's standing instruction: do NOT run `make test-unit-all` or
> `make test-integration-rustd` per-change. Run both ONCE, after all 7 sections
> are complete. Commit after each section.

Every unit test in this milestone is datastore-free.

**`cargo test -p afd_identity` without `--all-features` fails to compile** —
its suites import `afd_core::clock::FixedClock`, which is behind `test-util`.
Pre-existing, unrelated to this work, and green with `--all-features`.

## Next Steps

1. **The approval gate's WRITE half.** The read side is done
   (`approval_gate_async.zig`, `approval_gate_anomaly.zig`). Remaining:
   `approval_gate_park.zig` (122 — `parkEvent`, `logGateActivity`),
   `approval_gate_detail.zig` (306 — the card's two-source detail: the
   workspace-authored half statable as fact, the model-authored half attributed
   as a claim), and `pauseFleet` + `fleet_ready.forceClear`.
   - **`Admission` gains an `Await` arm when this lands.** It has three today.
   - **KIND-PARK rule:** a fleet whose repository binding declares WRITE access
     parks EVERY first-encounter event — before the rules walk AND before the
     no-gates return — and anomaly counters are skipped on that path.
   - **NOT this milestone's:** Slack rendering (349), sweeper (126), route
     handler (90), prose (130). §5's note puts rendering with M178.
2. **The run-estimate debit** — the DOCUMENTED divergence (spec §2). Needs a
   wallet-drain statement the Zig has no issue-time counterpart for.
   `money/charge.rs` already marks where a real transaction belongs.
3. **`ExecutionPolicy` + `fleet_sessions`** — `service_execution_policy.zig`
   (104), `lib/contract/execution_policy.zig` (320), plus the lease network
   rules from `git/repository_http_policy.zig` (129).
   `endpoint::validate` already answers the bare host the egress allowlist
   needs. See the Write 5 gotcha below.
4. **Wire the lease verb's router arm** — `afd_api::router::runner_handler`.
5. **§3 report** — serial after §2, same agent (spec's B1 batch).
6. **Boot still owes the daemon a KEK.** `Vault::new` takes an `Arc<Kek>` and
   nothing constructs one: `ENCRYPTION_MASTER_KEY` → `Kek::from_hex` belongs in
   `agentsfleetd::serve`, the handle in `plane.rs`, landing with step 4. The
   daemon must REFUSE TO START without it — that is what makes the key a field
   rather than a fallible global read.

## Risks / Gotchas

- **CI has never run this branch.** 20 commits, unpushed, no PR.
- **`make harness-verify` does NOT check file length.** `error/mod.rs` hit 460
  lines with every gate green. Rubric S6 is the real check:
  `git diff --name-only origin/main...HEAD | grep -v '\.md$' | xargs wc -l |
  awk '$1>350'`. Clean now except `audits/logging.sh` (orly-managed),
  `Cargo.lock` (generated) and `Cargo.toml` (manifest).
- **The UFS gate counts numeric literals inside `#[cfg(test)]`.** The rule page
  holds test blocks out of the STRING count; the numeric-suspect check does not.
  Name fixture numbers as you write them.
- **The MS-ID gate flags `§X.Y` in source.** Bare `M178` is fine; `§4.5` is not.
  Name the verb, not the section.
- **A green harness-verify row can mean "scanned nothing."** Read the file
  COUNT. `ERROR REGISTRY` scans Zig only; the Rust registry is covered by
  `cargo test -p afd_core`, which caught two real gaps this session.
- **`UZ-RUN-015` must answer 402.** The stock runner classifies a renew refusal
  by BOTH status and code — `control_plane_client_test.zig:42` pins that the
  same code on any other terminal status is NOT a budget breach.
- **Write 5 (`UPSERT core.fleet_sessions SET execution_id`) is documentation
  drift.** `data_flow.md` §C lists it among the six lease writes; the Zig lease
  path does NOT make it — `fleet_session.zig` only CLEARs a stale handle. Do not
  invent the write to match the diagram. **Open Indy decision, due at step 3.**
- **The credit metric is a SEAM.** `debit_receive` returns `Deducted(Nanos)`
  instead of calling a meter inline. If §6/M181 never attaches the instrument,
  the metric silently never fires and no test in either milestone catches it.
- **`secrets_map` holds live tenant credentials un-wiped.** They are bound for
  the wire and a `serde_json::Value` tree has nowhere to put a destructor — the
  Zig's arena does not zero them either. What IS defended is the realistic leak:
  `Declared` has a hand-written `Debug` rendering names and the mintable half
  but never a stored value.
- **`KEEP_TEST_STATE=1` breaks `test_migrate_applies_and_reports_success`.**
  Single-suite use only.
- **Redis is NOT per-test.** Lease tests derive fleet ids from `process::id()`
  plus a counter; a constant id inherits the previous run's stream entries.
- **`aws-lc-sys` proven on macOS/arm64 only.** musl cross-compile is M181's.

## Decisions taken this session (do not re-open)

1. **SSRF endpoint refusal is PERMANENT.** The Zig omits
   `SecretEndpointInvalid` from `resolveTenant`'s permanent list, so it falls
   through the `else` and re-polls forever with no terminal row. Flipped, per
   Indy. Registered as a divergence beside the issue-time debit; costs no SQL
   change, and the dual-run differ that would have graded it went with the Zig
   integration lanes.
2. **The object gate is swept across six boundaries**, via
   `afd_core::json::object_from_slice`: runner enrolment body, capability
   report, JWT header, JWKS document, OIDC discovery document, vault credential.

## Corrections made this session (design notes worth keeping)

- **The connector registry was a regression, not a port.**
  `credentials/integration.zig` states RULE CFG outright — a connector is a
  descriptor, not a branch — and injects its registry for testability. The first
  cut collapsed both into an enum with two matches. It is now
  `Connector` / `Connectors` / `Descriptor` / `Registry`, which is
  `afd_fleet_runtime::provider`'s shape exactly. Adding a connector is ONE entry
  in `DECLARED`; the mint exchange attaches later as a second trait.
- **`GateRef` was carrying Zig's `"action_id|deadline_ms"`** — a hand-rolled
  pipe format that exists upstream only because Zig has no serializer. One
  writer, one reader, both on the lease path, no contract. It is a serde type
  now, and `#[serde(try_from)]` runs the identifier validation on every read.
- **Two stored vocabularies were hand-written matches.** `Answer` and `Status`
  are `#[serde(rename)]` declarations read through
  `afd_core::spelling::from_spelling` — which was `runner::policy`'s
  crate-private `parse_wire` with one consumer and now has three.
- **The URL guard uses `url::Url`, not thirty hand-written lines.** The wire
  justification for hand-rolling was checked and found false: the runner
  compares allowlist entries with `std.ascii.eqlIgnoreCase` at all three of its
  matching sites, so normalisation is harmless. `Host` comes out TYPED, so
  `ssrf` classifies an `Ipv4Addr` directly. **Three verdicts changed**, each the
  parser agreeing with what a client would dial: `https:///just/a/path` dials
  `just`; `https://256.1.1.1/v1` is refused (the Zig passed it through as a
  NAME — the unsafe direction); a schemeless host reports `InvalidScheme`.
- **The object gate is a serde adapter, not a byte scan.** An earlier cut
  scanned for the first non-whitespace `{` and was defended with "serde parses
  twice" — true of the `Value`/`Map` round-trip, false of serde. `ObjectOnly`
  forwards `deserialize_struct` as `deserialize_map`: one pass, no intermediate,
  and serde's own `invalid type: sequence, expected struct …` in place of a
  message this repository would have had to word itself.
- **`admit/posture.rs` is `admit/fault.rs`.** `Posture` is the money path's —
  `billing.usage_ledger.posture`'s own spelling — and the module beside it meant
  `admit/mod.rs` imported `posture::PAYER` and `Posture` on adjacent lines for
  two unrelated questions.

## Open questions for Indy

1. **Write 5** — port the `fleet_sessions` busy-marking the diagram describes,
   or follow the Zig and leave it out? Recommend following the Zig and
   correcting `data_flow.md`, since row-equivalence is the graded claim.
2. `MAX_READY_CANDIDATES_PER_POLL` (64) as an operator knob? Analysis says no —
   `HRANDFIELD` samples uniformly so it cannot starve.
3. Can a tenant *require* an isolation class, or only be told which one it got?
   Recommendation: told, for M177.
