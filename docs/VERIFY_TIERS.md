# VERIFY — Tiers, performance, hygiene

> Parent: [`../AGENTS.md`](../AGENTS.md) §VERIFY. Verification Gate (`dispatch/verify.md`) enforces; `make` targets are canonical.

**FIRST: `/write-unit-test`** — audits diff coverage vs spec's Test Specification (or changed surface when no spec). Iterate until clean. Skipping = CHORE(close) violation.

## Correctness tiers (do not skip)

| Tier | Command | When |
|---|---|---|
| 1 | `make test-unit-all` | Every EXECUTE iteration; start of VERIFY. Unit-only — never substitutes for 2/3. Zig-only fast loop: `make test-unit-agentsfleetd`. |
| 2 | `make test-integration` | Diff touches `src/agentsfleetd/http/**`, `src/agentsfleetd/db/**`, `src/agentsfleetd/fleet/**`, `src/agentsfleetd/observability/**`, `*_integration_test.zig`, schema, migrations. Before COMMIT. |
| 3 | `make test-integration` | ≥1× per branch from clean state (after `make down`) before ship-ready. Mandatory when schema changes pre-v2.0. Tier 2 passing + 3 failing = state pollution; fix isolation. |

## Test delta — VERIFY ends by reporting coverage growth

CHORE(open) recorded the branch-point counts in the spec header (`**Test Baseline:** unit=<N> integration=<M>`, from `make _lint_zig_test_depth`, which also writes `.tmp/agentsfleetd-test-depth.txt`). VERIFY ends with the same command and this required row in the verification block:

```
Test Delta: unit <N₀>→<N₁> (+x) · integration <M₀>→<M₁> (+y) vs CHORE(open) baseline
Lacking:    <changed surfaces whose tests did not grow, or "none">
```

- **Delta is deterministic; Lacking is judgment (🔵)** — walk the diff's changed modules and name every one whose coverage did not move (e.g. "EgressScope Linux paths — integration lane pending"). "none" must be earned, not defaulted.
- **Zero/negative unit delta while the diff adds non-trivial code** → justify in the row (pure refactor, test consolidation) or return to EXECUTE. Deleting tests to go green is a violation, not a justification.
- Spec predates this rule (no `Test Baseline:` line) → reconstruct from the branch point (run the counter at the CHORE(open) commit or on `origin/main`) and add the line to the spec in the same commit as the VERIFY report.

## Performance / leak (before PR)

| Gate | Command | When |
|---|---|---|
| Leak | `make memleak` | Server lifecycle (`src/agentsfleetd/http/**`, `src/agentsfleetd/cmd/serve.zig`), allocator wiring, cross-thread heap ownership. |
| Bench (local) | `make bench` | When the diff touches request-path code, allocator wiring, or startup/shutdown sequencing. |
| Bench (dev) | `API_BENCH_URL=https://api-dev.agentsfleet.net/healthz make bench` | After deploy to dev. |

**Memleak lanes.** `make memleak` leak-gates all THREE test graphs, each named in its own lane prefix: **`agentsfleetd`** (daemon), **`runner`** (`build_runner.zig` unit tests — the integration tests fork+exec real children and are excluded, invalid under valgrind without `--trace-children`), and **`lib`** (three `src/lib` artifacts: `agentsfleet-lib-tests` / `-logging-tests` / `-call-deadline-tests`). On Linux the blocking gate is valgrind (`--leak-check=full`, exit-code-propagating); on macOS it is the blocking `std.testing.allocator` run per binary plus advisory `leaks` (unreliable under System Integrity Protection — a timing- or allocation-heavy test may spuriously fail the `leaks` pass, which is non-blocking by design). `-Dopenssl=false` is a ROOT-graph option only (valgrind chokes on OpenSSL's pools); the `runner` graph links no OpenSSL and rejects the flag. A final **`boot-drain`** lane migrates the test DB, exports the integration env, builds the daemon test binary filtered to the boot→SIGTERM→drain lifecycle test, runs it under the gate against live pg + TLS redis, and greps a run-proof marker — so that DB-gated lifecycle test cannot silently skip under valgrind and leave the leak claim vacuous. The general daemon lane deliberately runs with NO DB env (skips DB tests); un-skipping the full suite under valgrind would be a 10–30× slowdown, so only the one lifecycle test is un-skipped, via `-Dtest-filter`.

Knobs (`make/test-bench.mk`): `API_BENCH_METHOD`, `_DURATION_SEC`, `_CONCURRENCY`, `_TIMEOUT_MS`, `_MAX_ERROR_RATE`, `_MAX_P95_MS`, `_MAX_RSS_GROWTH_MB`.

**Memleak evidence:** paste final `make memleak` line into PR Session Notes OR cite CI URL. Branches touching `src/agentsfleetd/http/**`/`src/agentsfleetd/cmd/serve.zig`/allocator wiring → last 3 lines verbatim. No "trust me".

## Hygiene (always, before PR)

`make lint-all` (hard); `make _lint_zig_pg_drain` (also folded into `make lint-all` via `lint-zig`) + cross-compile `x86_64-linux` + `aarch64-linux` (any `*.zig` touched); cross-layer orphan sweep (RULE ORP — every renamed/deleted symbol → 0 hits across schema/Zig/JS/tests/docs in non-historical files); `gitleaks detect` before any Zig-including commit; 350-line / 50-fn-line check via:

```bash
git diff --name-only origin/main \
  | grep -v -E '\.md$|^vendor/|_test\.|\.test\.|\.spec\.|/tests?/' \
  | xargs -I{} sh -c 'wc -l "{}"' \
  | awk '$1 > 350'
```

**Other:** after refactors, list newly dead code before removing — `NEWLY UNREACHABLE: <symbol/file> — <why now dead>. Remove? Confirm.` **Greptile learning capture:** each finding → "Could this recur?" If yes, add compact rule (Rule/Why/Tags/Ref) to RULES.md same commit. Never defer.
