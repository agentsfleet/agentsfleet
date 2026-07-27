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

**Zig coverage.** `make test-coverage-zig` runs the daemon, runner, and three shared-library unit binaries under kcov. Each component must produce a non-empty Cobertura report. The merged report lives under `coverage/zig/merged` and must meet `ZIG_COVERAGE_MIN_LINES`.

**Integration roots.** `make test-unit-agentsfleetd` runs `src/agentsfleetd/tests.zig`. `make test-integration` prepares isolated services once and runs `src/agentsfleetd/integration_tests.zig`, which owns the live PostgreSQL, Redis, and QStash imports. The runner's Linux kernel root remains independently selectable.

**Memleak lanes.** `make memleak` leak-gates all three test graphs concurrently, each named in its own lane prefix: **`agentsfleetd`** (daemon), **`runner`** (`build_runner.zig` unit tests — integration tests fork real children and are excluded), and **`lib`** (three `src/lib` binaries). Component failures are collected and reported before the final lifecycle proof. On Linux, Valgrind is blocking and propagates its exit code. On macOS, the Zig testing allocator is blocking. The advisory `leaks` rerun happens only when one lightweight preflight proves that the host can inspect child processes. `-Dopenssl=false` is a root-graph option only; the runner graph links no OpenSSL and rejects the flag. A final **`boot-drain`** lane migrates the test database, exports the integration environment, runs the filtered daemon lifecycle test under the blocking gate, and requires its run-proof marker.

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
