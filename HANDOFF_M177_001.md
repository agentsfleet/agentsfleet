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
- ✅ **§2 approval-gate WRITE side** — park, card detail (two halves as two
  TYPES), pause + readiness clear, and the pass that composes read and write.
  `Admission` gained its `Await` arm.
- ⏳ **§2 remainder — NEXT.** The run-estimate debit, `ExecutionPolicy` + the
  lease network rules, the router arm. See Next Steps.
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

1. ~~**The approval gate's WRITE half.**~~ DONE. The read side was done
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
2. **The run-estimate debit — 📟🔦📈💥☠️ JUDGMENT FLAG TO INDY. NOT BUILT.**
   Spec §2 decides to implement the issue-time floor-token run estimate because
   "the spec's two-debit shape and `data_flow.md` §C agree with each other and
   not with that code". A second architecture document contradicts BOTH, and it
   is the one that owns billing:

   `docs/architecture/billing_and_provider_keys.md`
   - §3 debit table, row 2: the run debit is "metered **incrementally** across
     the run — a delta on every `/renew`, settled at report **(M80_010 replaced
     the one-shot lease-issue estimate)**".
   - "Lease issue runs the *entry gate* …, the receive deduct, and the approval
     gate — **but no run debit**."
   - Ledger rows (M80_010): "one `receive` row, and **one `stage` row that
     M80_010 accumulates** … one event → **exactly 2 ledger rows**".
   - Rejected alternatives: "**Refund-on-actual-tokens. Superseded by M80_010** …
     the credit drained equals actual runtime × rate + actual tokens — **there is
     nothing to reconcile or refund after the fact**."

   `data_flow.md` §C line 250 describes the PRE-M80_010 shape ("one `stage` at
   the run debit, then UPDATEd with token counts after the report"), so it is
   stale on billing rather than authoritative. Its M177 divergence note (line
   852) was added BY this milestone's decision, so it is the same decision
   written twice, not independent corroboration.

   **Why this is not a paperwork question.** Implementing it needs three pieces
   of money machinery that do not exist:
   1. An **issue-time wallet drain**. `balance_nanos` is READ-only in all of
      `rustd/crates/afd_fleet/src/sql/` — the drain lives in the renewal CTE.
   2. A **charge type**. `stage` is owned by the renewal accumulator, whose
      `ON CONFLICT … DO UPDATE SET … = … + EXCLUDED.…` would ADD every slice on
      top of the estimate → the tenant pays estimate **plus** actual. That is a
      double charge. A third type instead breaks "exactly 2 ledger rows" and
      the revenue-by-charge-type query.
   3. A **settle-time reconcile** — precisely the machinery M80_010 deleted as
      unnecessary.

   It also weakens M177's own claim: the spec concedes Dimension 2.1's parity
   oracle can no longer compare against a live Zig daemon here, and
   row-equivalence at cutover is what this milestone exists to prove.

   **Recommendation: do NOT implement it.** Keep the single accumulating `stage`
   row, follow `billing_and_provider_keys.md`, and correct `data_flow.md` §C
   plus the spec. The estimate machinery already in `money/rates.rs` is
   correctly placed as a GATE input — that module's own note says it: "An
   estimate is not a charge." Nothing further is needed for §2's coverage
   refusal, which already works.

   Everything else in §2 proceeded; this is the only item parked.
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
- **Write 5 (`UPSERT core.fleet_sessions SET execution_id`) — DECIDED, and the
  earlier framing here was wrong.** This was recorded as "documentation drift",
  which a repo-wide trace disproves. `core.fleet_sessions.execution_id` is dead
  end to end in the Zig: **zero** production writers set it to a value, the one
  production statement touching it is `CLEAR_STALE_EXECUTION` (sets it to
  `NULL`), and **zero** production readers select it — `SELECT_FLEET_WITH_SESSION`,
  the claim read, takes `s.context_json` and nothing else. The only writers of a
  real value are two integration-test fixtures that hand-craft a busy row so
  they can watch the clear fire. The `IS NOT NULL` guard therefore makes that
  clear a permanent no-op, and `FleetSession.execution_id` /
  `execution_started_at` are always `null`/`0` inside a `@sizeOf == 424`
  assertion.

  So `data_flow.md` is not over-describing: it describes a designed feature
  that was never wired. The Zig has half an implementation — the clear without
  the set. "Follow the Zig" was not a reason; it was a Zig gap mistaken for a
  specification.

  **Decision (Indy, this session): do not port Write 5, and do not port
  `CLEAR_STALE_EXECUTION` either.** The reason is that `execution_id` would be a
  weaker duplicate of a truth we already own: `fleet.runner_leases` answers "is
  this fleet executing, since when, by whom, until when" WITH a fencing token, a
  `leased_until` TTL, a `UNIQUE runner_affinity` and a reclaim sweeper.
  `execution_id` answers the same question with no fence and no expiry — which
  is precisely why it needs crash recovery. It can only go stale. The Rust is
  currently clean of the whole mechanism (verified: no reference anywhere in
  `rustd/`); keep it that way.

  Two follow-ups, neither blocking M177: correct `data_flow.md` to name
  `fleet.runner_leases` as the source for "executing right now" (NOT to say "the
  daemon does not make this write", which would enshrine the gap); and raise the
  column itself as its own work item — recommendation is to DROP it plus the
  clear, the two struct fields and the two fixtures. If a cheap denormalised
  busy-flag is ever wanted for a fleet-list view (a legitimate read-amplification
  argument), it must be set INSIDE the lease-issue statement so it is atomic with
  the lease row and repairable from it — never as a separate write at a separate
  moment, which is the exact shape that drifts.

  One open constraint: `runner_leases` is in the `fleet` schema and
  `fleet_sessions` in `core`, so a grant boundary keeping the tenant status
  surface out of `fleet` would change the plumbing of the doc fix (not the
  conclusion). Worth checking when that surface is built.
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

1. `MAX_READY_CANDIDATES_PER_POLL` (64) as an operator knob? Analysis says no —
   `HRANDFIELD` samples uniformly so it cannot starve.
2. Can a tenant *require* an isolation class, or only be told which one it got?
   Recommendation: told, for M177.
