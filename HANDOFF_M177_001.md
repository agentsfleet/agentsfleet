# Handoff — M177_001 runner control plane · §2's money gates land

Ephemeral. Delete at CHORE(close). Replaces the prior handoff.

## Scope / Status

Porting `/v1/runners` to `agentsfleetd-rs`. Spec
`docs/v2/active/M177_001_P0_API_RUNNER_CONTROL_PLANE_PARITY.md`, IN_PROGRESS.

- ✅ CHORE(open), crypto/dependency foundation, **§1 complete**
- ✅ **§2 assignment half** — claim, fence, reclaim, selection pass, lease row
- ✅ **§5 claim-time half** — stored `config_json` → typed `FleetConfig`
- ✅ **§2 money gates (this session)** — payer → balance → fleet budget →
  receive debit, plus the `Admission` outcome type the whole pass composes on
- ⏳ **§2's remainder — NEXT.** Provider resolution, `secrets_map`, the
  approval gate, the run-estimate debit, `ExecutionPolicy`. See Next Steps.
- ⏳ §3 report, §4 activity/memory/bundles/mint, §6 sweepers, §7 harness

## Working Tree

Worktree `/Users/kishore/Projects/agentsfleet-m177-runner-plane`.
**CLEAN.** Nothing staged, modified, or untracked.

```
## feat/m177-runner-control-plane-parity
f1b5be6ce refactor(rustd): split the admission pass at its seams        ← this session
63d756553 feat(rustd): the money gates decide — and say which kind of no ← this session
843b6fbf2 docs(m177): handoff — §5 lands, §2's money half unblocked
42dcbee78 feat(rustd): fleet config resolves
feec3292e the runner plane answers — guard, heartbeat, and the lease claim
```

**8 commits ahead of `origin/main`, none pushed. No PR. CI has never run this
branch.** Everything below is local.

## What this session built

`afd_fleet`, 2,018 new lines across 12 files:

```
src/money/            the unit, the windows, the reads, the charge
  nanos.rs      321   Nanos + SliceRates + slice_charge (pure)
  window.rs     177   jiff-backed UTC-month floor + rolling day (pure)
  rates.rs      280   Posture, Estimate, catalogue rate resolution
  budget.rs     231   Spend, Verdict, covers() (pure) + the drain read
  charge.rs     167   the usage_ledger write
  wallet.rs     113   payer + wallet reads
  store.rs       81   `Accounts` — the pool + entropy
src/lease/admit/      the ordered pass
  mod.rs        240   Admission / Billed / Refusal / Transient / money_gates
  posture.rs    140   OnFault + the four Gate constants + absorb
  gates.rs      112   balance + fleet_budget
src/sql/billing.rs 183 verbatim SQL, five statements
```

39 unit tests, all datastore-free.

### Design decisions worth not re-deriving

1. **`Admission` is a 3-arm sum type, and the terminal write is the caller's.**
   `runBilling` answers `?Billed`, where whether an event was terminally killed
   depends on whether the function happened to call `blockEvent` before
   returning null — six of ten exits do. Admit / Refuse / Retry now say it.
2. **Fault posture is a `Gate` value, not a `catch`.** `posture.rs` declares
   what each gate's own failure decides; `Gate::absorb` is the only code acting
   on it. Two tests assert the fail-open/fail-closed asymmetry with no database.
   This generalises what `budget.zig` already discovered with its `BudgetRead`
   union — Rust needs no union, because a fault is `Err` and a verdict is `Ok`.
3. **`Accounts`, not `Ledger`.** `afd_db::migrate::Ledger` already exists and is
   the SCHEMA-MIGRATION ledger. Two `Ledger` types in one workspace is an
   ambiguous import; the name is taken.
4. **`jiff` over `chrono`/`time`, against the download numbers** (161.8M/161.3M
   vs 56.6M recent, all measured Aug 25). A civil date is a type distinct from
   an instant, which is what the UTC-month window turns on. Confined to ONE
   function, so a swap stays cheap. Same shape of trade as the `garde` pick.
5. **No rate cache.** `model_rate_cache.zig` is ~300 lines of fixed-capacity
   table plus a collision proof. `LOAD_RATE_WITH_REVISION` already returns the
   rate AND its generation in one snapshot, so reading fresh costs one round
   trip — fewer than the Zig's cached path — with no generation to compare and
   no eviction. §3's renew prices per slice and may want one for LATENCY; that
   would be an optimisation over a correct read, not a coherence mechanism.
6. **NOT `rust_decimal`.** The authored ceiling is `f64` (`Dollars(f64)`, same
   as the Zig), the ledger is `i64` nanos, and the daemon this must stay
   row-equivalent with computes `@round(dollars * 1e9)`. A decimal between them
   rounds differently at the boundary — correct in the abstract, a divergence in
   the only thing Invariant 5 grades. The argument for decimals belongs to the
   schema.
7. **A `FirstDelivery` witness was built and REMOVED.** `Delivery` is already a
   two-variant enum matched exhaustively one line from the call; the witness
   re-stated a guarantee the compiler already enforced.

## Spec amendments made this session

- **§5 split along its call sites.** Every non-test caller traced:
  `parseStoredFleetConfig` is called by the lease verb (`fleet_session.zig:129`)
  and the mint verb (`credentials_mint_scope.zig:66`) — both M177's — plus
  `cron_sync.zig` (M180). `parseTriggerMarkdownWithJson` is called by
  `fleets/create.zig`, `fleets/patch_txn.zig`, `slack/channel_fleet.zig` (M178)
  and `fleet_library/importer.zig` (M179) — **none this milestone's**. So
  Dimension 5.1 is now the STORED corpus, M178 gains Dimension 3.4 for the
  frontmatter corpus, and the `serde_norway` decision travels with it.
- **M181 §5 gains the metrics PIPELINE.** It was written as "the transport", on
  the reading that M176 §6 shipped everything it plugs into. M176 shipped
  SPANS; there is no metric instrument anywhere in the Rust tree, so a transport
  would export an empty payload and §4's `test_metric_continuity` stays
  ungradeable. ~1,450 Zig lines replaced by `opentelemetry_sdk`'s `metrics`
  feature (a flag on a crate already in the lock). New Dimensions 5.4 / 5.5.
- **M177 Dimension 6.4 re-scoped** to the overflow BOUND, not the spelling: the
  OTel spec marks overflow `otel.metric.overflow=true` where the Zig uses an
  `_other` label. Pinning `_other` in a P0 row here would force M181's SDK
  decision by the back door. M181's 5.5 asserts the spelling.
- Two clusters were nearly filed as gaps and are NOT: `otel_logs.zig` is already
  decided in M181 §5 (logfmt on stderr, Collector `filelog` receiver), and
  `library_read_counters.zig` + siblings (~680 lines) are TEST-ONLY.

## Tests / Checks

| Gate | Command | Result |
|---|---|---|
| CONFORM | `make harness-verify` | ✅ ALL GATES GREEN (19 files, then 3) |
| **LOGGING (whole repo)** | `bash audits/logging.sh` | ✅ **CLEAN — `rust-direct=0`** |
| lint | `cargo clippy --workspace --all-features --all-targets` | ✅ clean |
| fmt | `cargo fmt --all -- --check` | ✅ |
| unit (crate) | `cargo test -p afd_fleet --lib` | ✅ 39 passed |
| unit (afd_core) | `cargo test -p afd_core` | ✅ incl. registry parity |
| unit (repo) | `make test-unit-all` | ⏳ **deferred by Indy** — run once at the end |
| integration | `make test-integration-rustd` | ⏳ **deferred by Indy** — same |

> Indy's instruction this session: do NOT run `make test-unit-all` or
> `make test-integration-rustd` per-change; run both **once**, after all 7
> sections are complete. Commit after each section.

**The 6 known logging false positives are GONE.** orly 0.7.1 (`orly update
--no-hooks`) fixed four; `banner.rs` and `fatal.rs` — where writing to a stream
IS the design — now carry the sanctioned `// logging: <reason>` annotation.
`PROMPT_ORLY_LOGGING_GATE_RUST.md` is deleted.

## Next Steps

1. **Provider resolution** — the one input `money_gates` takes rather than
   resolves. ~1,180 unported Zig lines: `tenant_provider_resolver.zig` (231),
   `secret_probe.zig` (197), `secrets/crypto_store.zig` (266), `vault.zig`
   (326), `base_url_guard.zig` (160). Produces `{posture, provider, model,
   api_key}`. **The key must be `Zeroizing`** — that is what replaces the Zig's
   `committed` flag + `secureZero` + `defer` dance, and it is what makes
   "the key we billed is the key we deliver" structural.
   Two strategies (platform default vs self-managed) — a natural trait with two
   impls, per Indy's direction below.
2. **`secrets_map`** — `secrets_resolve.zig` (160) + the `mintable`
   classification. Invariant 3: the provider key never enters this map.
3. **Approval gate** — `fleet/approval_gate.zig` (289) plus its Redis subsystem
   (`approval_gate_async`, `_resolver`, `_park`, `_detail`, `_route`,
   `_constants`). The third gate, and the only one that can answer *pending*.
   Note `Admission` has no `Await` arm yet — add it when this lands.
4. **The run-estimate debit** — the DOCUMENTED divergence (spec §2). Needs a
   wallet-drain statement the Zig has no issue-time counterpart for. `charge.rs`
   already notes where a real transaction belongs.
5. **`ExecutionPolicy` + `fleet_sessions`** — `service_execution_policy.zig`
   (104). See the gotcha below about write 5.
6. Wire the lease verb's router arm — `afd_api::router::runner_handler`.
7. §3 report — serial after §2, same agent (spec's B1 batch).

## Indy's direction for the code (carry forward)

> "ensure we use the rust functional programming technique, errors, traits,
> trait impl, trait objects. don't repeat yourself. Keep absurdly simple struct"

Where this stands and where it does not, honestly:

- ✅ Errors/`Result` composition, exhaustive matches, `Option` combinators,
  pure functions separated from I/O, newtypes (`Nanos`), data-driven policy
  (`Gate`/`OnFault`).
- ✅ Repetition collapsed in the split commit (`absorb(&fault).ok_or(fault)`).
- ⚠️ **No traits or trait objects yet.** The natural first home is step 1's
  provider resolver — two strategies behind one interface, mirroring
  `afd_fleet_runtime`'s existing `&dyn ProviderRegistry`. Do it there.
- ⚠️ **`Request<'a>` has 9 fields and `Charged<'a>` has 7.** Both are parameter
  bundles, not domain objects. They are simple (all `Copy`, no methods, no
  invariants) but they are not *small*. If they grow further, that is the signal
  to split the pass rather than widen the struct.

## Risks / Gotchas

- **CI has never run this branch.** 8 commits, unpushed, no PR.
- **`make harness-verify` does NOT check file length.** FLL is not in the staged
  gate set — `admit.rs` reached 414 lines and every gate stayed green. Rubric S6
  is the real check: `git diff --name-only origin/main...HEAD | grep -v '\.md$'
  | xargs wc -l | awk '$1>350'`. Currently clean except `audits/logging.sh`
  (orly-managed), `Cargo.lock` (generated) and `Cargo.toml` (manifest).
- **A green harness-verify row can still mean "scanned nothing."** Read the file
  COUNT. Note `ERROR REGISTRY` reported "no source files in scope" while
  `error_code.rs` was staged — that gate appears to scan Zig only. The Rust
  registry is covered by `cargo test -p afd_core` instead, which DID catch two
  real omissions this session (a missing problem-table entry, and a missing
  402 → `.payment_required` mapping).
- **`UZ-RUN-015` must answer 402.** The stock runner classifies a renew refusal
  by BOTH status and code — `control_plane_client_test.zig:42` pins that the
  same code on any other terminal status is NOT a budget breach.
- **Write 5 (`UPSERT core.fleet_sessions SET execution_id`) is documentation
  drift.** `data_flow.md` §C lists it among the six lease writes; the Zig lease
  path does NOT make it — `fleet_session.zig` only CLEARs a stale handle. Do not
  invent the write to match the diagram (Invariant 5 is row-equivalence with the
  code). Worth an Indy decision at §3.
- **The credit metric is a SEAM.** `debit_receive` returns `Deducted(Nanos)`
  instead of calling a meter inline the way `service_billing.zig` does. If
  §6/M181 never attaches the instrument, the metric silently never fires and no
  test in either milestone catches it. Recorded in the spec's Metrics table.
- **Datastores are DOWN** — `docker compose ps` empty. `make up` before any
  integration work. Ports are per-run; re-read them, do not reuse old numbers.
- **`KEEP_TEST_STATE=1` breaks `test_migrate_applies_and_reports_success`.**
  Single-suite use only.
- **Redis is NOT per-test.** Lease tests derive fleet ids from `process::id()` +
  a counter; a constant id inherits the previous run's stream entries.
- **`aws-lc-sys` proven on macOS/arm64 only.** musl cross-compile is M181's.
- **The FLL cap bites constantly.** `nanos.rs` is at 321 and `config/gates.rs`
  at 329. Check `wc -l` before adding.

## Open questions for Indy

1. **Write 5 above** — port the `fleet_sessions` busy-marking the diagram
   describes, or follow the Zig and leave it out? Recommend following the Zig
   and correcting `data_flow.md`, since row-equivalence is the graded claim.
2. `MAX_READY_CANDIDATES_PER_POLL` (64) as an operator knob? Analysis says no —
   `HRANDFIELD` samples uniformly so it cannot starve.
3. Can a tenant *require* an isolation class, or only be told which one it got?
   Recommendation: told, for M177.
