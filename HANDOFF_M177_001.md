# Handoff — M177_001 runner control plane · §2's key, credentials and gate

Ephemeral. Delete at CHORE(close). Replaces the prior handoff.

## Scope / Status

Porting `/v1/runners` to `agentsfleetd-rs`. Spec
`docs/v2/active/M177_001_P0_API_RUNNER_CONTROL_PLANE_PARITY.md`, IN_PROGRESS.

- ✅ CHORE(open), crypto/dependency foundation, **§1 complete**
- ✅ **§2 assignment half** — claim, fence, reclaim, selection pass, lease row
- ✅ **§5 claim-time half** — stored `config_json` → typed `FleetConfig`
- ✅ **§2 money gates** — payer → balance → fleet budget → receive debit
- ✅ **§2 provider resolution (this session)** — two strategies, one interface
- ✅ **§2 `secrets_map` (this session)** — the two-channel split
- ✅ **§2 approval-gate DECISION (this session)** — rule match, route table
- ⏳ **§2 remainder — NEXT.** The approval gate's I/O half, the run-estimate
  debit, `ExecutionPolicy`. See Next Steps.
- ⏳ §3 report, §4 activity/memory/bundles/mint, §6 sweepers, §7 harness

## Working Tree

Worktree `/Users/kishore/Projects/agentsfleet-m177-runner-plane`.
**CLEAN.** Nothing staged, modified, or untracked.

```
## feat/m177-runner-control-plane-parity
e0055ffa5 the approval gate decides, and the order it decides in is a type ← this session
23d546c1d a fleet's credentials split by channel, not by convention        ← this session
ffad7c998 the provider resolves through two strategies, one interface      ← this session
840e8cc51 docs(m177): handoff — §2's money gates land
f1b5be6ce refactor(rustd): split the admission pass at its seams
```

**11 commits ahead of `origin/main`, none pushed. No PR. CI has never run this
branch.** Everything below is local.

## What this session built

3,626 lines across 28 files. **96 `afd_fleet` unit tests** (was 39) and
**42 `afd_fleet_runtime`** (was 37) — all datastore-free.

```
afd_fleet/src/provider/       resolution — 1,268 lines
  mod.rs        244   Resolution trait, Box<dyn> dispatch, the object gate
  endpoint.rs   298   the provider⇔base_url pairing + host extraction
  managed.rs    314   the tenant's own credential
  resolved.rs   231   Resolved + SecretString (private key, redacting Debug)
  selection.rs  227   the two row reads + the tenant→workspace bridge
  ssrf.rs       206   the blocklist, over std::net
  platform.rs   199   the active platform default
  store.rs       67   Providers
afd_fleet/src/vault.rs 214   one store, one key, one decrypt routine
afd_fleet/src/secrets/        the two-channel split — 531 lines
  mod.rs        303   Declared { secrets_map, mintable } + the routing branch
  integration.rs 228  the ids, the spellings, mints_on_demand
afd_fleet/src/gate/           the pure decision — 434 lines
  mod.rs        269   Decision, match_rule, condition evaluation
  route.rs      165   RefState × Decision → Route
afd_fleet_runtime/src/config/condition.rs 157   the `field == 'value'` grammar
afd_fleet/src/error/classify.rs 196  code / detail / permanence, split out
afd_fleet/src/sql/{provider,vault}.rs   six statements, verbatim
```

### Design decisions worth not re-deriving

1. **The provider resolver is a `Box<dyn Resolution>` with two impls.** A
   strategy knows WHICH vault row carries its key and what that row's body
   means; the shared half — open the row — runs between the trait's two methods
   and is written once. Both methods are synchronous and pure, so the whole
   fork is provable with no Postgres, no vault and no key. An `async fn` in the
   trait would have needed boxed futures to stay object-safe AND would have
   given that up.
2. **`ip_literal.zig` does not come across.** 249 lines of hand-rolled IPv4/IPv6
   parsers and octet constants become `IpAddr::from_str` plus std's own
   predicates. Four ranges std does not name (`0/8`, `240/4`, `fc00::/7`,
   `fe80::/10`) are four masked comparisons, each named. A literal a parser
   fails to PARSE is one it reports as SAFE — the wrong place to re-derive
   anything.
3. **The base-URL HOST extraction stays hand-written, with `url` 2.5.8 in the
   lock.** `Url::host_str` normalises — lower-cases, punycodes, unbrackets IPv6
   — and that host travels to a stock Zig runner as its egress-allowlist entry,
   where `hostFromUrl` produces the unnormalised form. Thirty lines of byte
   copying buys wire parity; the CLASSIFICATION, where being subtly wrong is a
   security hole, is std's.
4. **`SecretString` is private on `Resolved`, with no `Deref` and no owned
   getter.** That is Invariant 3 as a compiler fact. It replaces the Zig's
   `committed` flag + `defer if (!committed) …deinit()` — a hand-rolled move,
   where the flag is what a move already means.
5. **The credentials split is a TYPE.** `Declared { secrets_map, mintable }` is
   built by one branch and constructible nowhere else, so a mintable handle
   cannot reach both channels. The Zig walks one list appending to two builders,
   and Invariant 1 holds because that walk is written correctly.
6. **`mints_on_demand` is one negation, not a list** — a new connector is
   on-demand BY DEFAULT. The other spelling's failure mode is shipping a stored
   refresh token to a child process.
7. **No `REGISTRY` slice in the integration port.** It exists in Zig because
   `Spec` carries a function pointer; with the mint strategies out of scope
   every entry collapses to a bool the id already knows.
8. **One rules traversal, not two.** `match_rule` walks; `Decision::of` is a
   pure function of its result. The Zig has two walks and a comment insisting
   they agree.
9. **Provider failures answer `UZ-INTERNAL-003`, not `UZ-PROVIDER-*`.** Those
   codes belong to the TENANT plane's handler (M178). `service_billing.zig`
   logs the internal code for the whole family; adding the finer ones here
   would be unreferenced codes that look like coverage.

## Two findings from this session

1. **`serde_json` fills a struct from a JSON ARRAY, positionally.**
   `["anthropic","sk-live"]` parsed as a credential with a provider and a key
   and passed every shape check after it. `loadJson` refuses a non-object at
   the top; `provider::credential` now does too, structurally and in one pass.
   Caught by a test, not by review.
2. **`SecretEndpointInvalid` retries forever in the Zig.** It is absent from
   `resolveTenant`'s permanent list and falls through its `else`, so a stored
   endpoint failing the SSRF guard is re-polled indefinitely. **Ported as-is**
   — the two daemons must write the same rows during the cutover — and pinned
   by `an_ssrf_refusal_is_ported_as_transient`, so flipping it cannot be
   silent. **Open question for Indy** (see below).

## Tests / Checks

| Gate | Command | Result |
|---|---|---|
| CONFORM | `make harness-verify` | ✅ ALL GATES GREEN |
| **LOGGING (whole repo)** | `bash audits/logging.sh` | ✅ **CLEAN — `rust-direct=0`** |
| lint | `cargo clippy --workspace --all-features --all-targets` | ✅ clean |
| fmt | `cargo fmt --all -- --check` | ✅ |
| unit (afd_fleet) | `cargo test -p afd_fleet --lib` | ✅ 96 passed |
| unit (afd_fleet_runtime) | `cargo test -p afd_fleet_runtime --lib` | ✅ 42 passed |
| unit (afd_core) | `cargo test -p afd_core` | ✅ incl. registry parity |
| S6 file length | rubric command | ✅ clean (see gotcha 2) |
| unit (repo) | `make test-unit-all` | ⏳ **deferred by Indy** — once at the end |
| integration | `make test-integration-rustd` | ⏳ **deferred by Indy** — same |

> Indy's standing instruction: do NOT run `make test-unit-all` or
> `make test-integration-rustd` per-change; run both **once**, after all 7
> sections are complete. Commit after each section.

## Next Steps

1. **The approval gate's I/O half** — everything the decision core is upstream
   of. `approval_gate_async.zig` (193: `lookupEventGateRef`, `evaluateRef`,
   `EventGateRef`), `approval_gate_park.zig` (122: `parkEvent`,
   `logGateActivity`), `approval_gate_anomaly.zig` (81) + the counter INCR,
   `approval_gate_detail.zig` (306: the card's two-source detail), and
   `pauseFleet` + `fleet_ready.forceClear`. **`Admission` still has no `Await`
   arm — add it when this lands.** Note the KIND-PARK rule: a fleet whose
   repository binding declares WRITE access parks EVERY first-encounter event,
   before the rules walk and before the no-gates return.
   **NOT this milestone's:** the Slack rendering (349), the sweeper (126), the
   route handler (90), the prose (130) — §5's note puts rendering with M178.
2. **The run-estimate debit** — the DOCUMENTED divergence (spec §2). Needs a
   wallet-drain statement the Zig has no issue-time counterpart for.
   `money/charge.rs` already notes where a real transaction belongs.
3. **`ExecutionPolicy` + `fleet_sessions`** — `service_execution_policy.zig`
   (104) and `lib/contract/execution_policy.zig` (320), plus the lease network
   rules from `git/repository_http_policy.zig` (129). `endpoint::validate`
   already answers the bare host the egress allowlist needs. See the gotcha
   about write 5.
4. Wire the lease verb's router arm — `afd_api::router::runner_handler`.
5. §3 report — serial after §2, same agent (spec's B1 batch).
6. **Boot wiring is still owed.** `Vault::new` takes an `Arc<Kek>` and nothing
   constructs one yet: `ENCRYPTION_MASTER_KEY` → `Kek::from_hex` belongs in
   `agentsfleetd::serve` and the handle in `plane.rs`, landing with the router
   arm. The daemon must refuse to start without it — that is what makes the
   key a field rather than a fallible global read.

## Indy's direction for the code (carry forward)

> "ensure we use the rust functional programming technique, errors, traits,
> trait impl, trait objects. don't repeat yourself. Keep absurdly simple struct"

- ✅ **Trait objects now exist and earn their place** — `Box<dyn Resolution>`,
  two impls, the shared I/O written once. The gap the last handoff recorded is
  closed.
- ✅ Errors/`Result` composition, `Option` combinator chains (`fires` is one
  expression covering five undecidable cases), pure functions split from I/O,
  newtypes guarding security invariants (`SecretString`), data-driven
  classification (`is_config_permanent`, `route`).
- ✅ DRY where the Zig repeats itself: one rules walk, one decrypt routine
  serving two statements through a column offset, one object-shape gate.
- ⚠️ **`Request<'a>` is still 9 fields** and will want a tenth when the gate's
  `Await` arm lands. That is the signal to split the pass rather than widen the
  struct — it was flagged last session and the pressure has not gone away.

## Risks / Gotchas

- **CI has never run this branch.** 11 commits, unpushed, no PR.
- **`make harness-verify` does NOT check file length.** `error/mod.rs` reached
  460 lines with every gate green this session — exactly the trap the last
  handoff named. It was split at the seam `detail.rs` already describes
  (`classify.rs` = what we decide). Rubric S6 is the real check:
  `git diff --name-only origin/main...HEAD | grep -v '\.md$' | xargs wc -l |
  awk '$1>350'`. Currently clean except `audits/logging.sh` (orly-managed),
  `Cargo.lock` (generated) and `Cargo.toml` (manifest).
- **The MS-ID gate flags `§X.Y` in source.** Two doc comments citing a section
  number failed CONFORM; bare `M178` is fine, `§4.5` is not. Name the verb, not
  the section.
- **A green harness-verify row can still mean "scanned nothing."** Read the
  file COUNT. `ERROR REGISTRY` scans Zig only; the Rust registry is covered by
  `cargo test -p afd_core`, which this session caught a real gap with — the
  problem-table parity test rejected `UZ-AGT-003` until the `.failed_dependency`
  status spelling was recorded.
- **`UZ-RUN-015` must answer 402.** The stock runner classifies a renew refusal
  by BOTH status and code — `control_plane_client_test.zig:42` pins that the
  same code on any other terminal status is NOT a budget breach.
- **Write 5 (`UPSERT core.fleet_sessions SET execution_id`) is documentation
  drift.** `data_flow.md` §C lists it among the six lease writes; the Zig lease
  path does NOT make it — `fleet_session.zig` only CLEARs a stale handle. Do
  not invent the write to match the diagram (Invariant 5 is row-equivalence
  with the code). Still an open Indy decision, now due at step 3.
- **The credit metric is a SEAM.** `debit_receive` returns `Deducted(Nanos)`
  instead of calling a meter inline. If §6/M181 never attaches the instrument,
  the metric silently never fires and no test in either milestone catches it.
- **`secrets_map` holds live tenant credentials un-wiped.** They are bound for
  the wire and a `serde_json::Value` tree has nowhere to put a destructor — the
  Zig's arena does not zero them either. What IS defended is the realistic
  leak: `Declared` has a hand-written `Debug` that renders names and the
  mintable half but never a stored value.
- **Datastores are DOWN** — `docker compose ps` empty. `make up` before any
  integration work. Ports are per-run; re-read them.
- **`KEEP_TEST_STATE=1` breaks `test_migrate_applies_and_reports_success`.**
  Single-suite use only.
- **Redis is NOT per-test.** Lease tests derive fleet ids from `process::id()`
  plus a counter; a constant id inherits the previous run's stream entries.
- **`aws-lc-sys` proven on macOS/arm64 only.** musl cross-compile is M181's.

## Open questions for Indy

1. **The SSRF retry loop** (finding 2 above). A stored `base_url` that fails
   the guard is classified TRANSIENT and re-polled forever, because the Zig's
   permanent list predates that error variant. Recommend flipping it to
   permanent and accepting that the Rust daemon writes a `gate_blocked` row the
   Zig does not — it is one line and one test. Left as the Zig has it pending
   your call.
2. **Write 5** — port the `fleet_sessions` busy-marking the diagram describes,
   or follow the Zig and leave it out? Recommend following the Zig and
   correcting `data_flow.md`, since row-equivalence is the graded claim.
3. `MAX_READY_CANDIDATES_PER_POLL` (64) as an operator knob? Analysis says no —
   `HRANDFIELD` samples uniformly so it cannot starve.
4. Can a tenant *require* an isolation class, or only be told which one it got?
   Recommendation: told, for M177.
