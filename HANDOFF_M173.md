# Handoff — M173_001 error-path coverage sweep

> Ephemeral. Delete at CHORE(close), per `AGENTS.md` §Required outputs.

## ⚠️ Read this before writing a single proof

**The work list is wrong more often than it is right, and it cost this session
three wasted proofs.** `scripts/classify_rung_callers.py` resolves at FILE level
over the **import** graph. An import taken for a *type or constant* marks the
whole file long-lived, because an import graph cannot tell a type reference from
a call. `observability/semconv.zig` does `const Mode = @import("../state/tenant_provider.zig").Mode;`
— one enum — and that alone marks five `state/**` files `repeating`.

Every rung proven this session against that list turned out arena-backed at
FUNCTION level:

| Proof written | File said | Function's real callers | Verdict |
|---|---|---|---|
| `vault_metadata_alloc_test.zig` | `repeating` | `loadMetadata` has ONE caller, a handler | cosmetic |
| `secret_probe_alloc_test.zig` | `repeating` | reaches handlers only, via `tenant_provider` → `fleet/service.zig` → `handlers/runner/lease.zig` | cosmetic |
| (`integration_grant_lookup`, not written) | `repeating` | `fleet/service.zig:218` calls `approvedSet(hx.alloc, …)` — the arena is IN the call | cosmetic |

The proofs are sound and mutation-checked. They are simply not the work Indy
asked for. **`secret_probe_alloc_test.zig`'s commit message asserts it is on a
leak-capable path. That assertion is FALSE and is not yet corrected in the
spec** — do that first if you touch this area.

**Until the instrument is fixed, never write a proof without running:**

```
python3 scripts/classify_rung_callers.py --why <file-relative-to-src>
git grep -n '<the function you are about to prove>' -- 'src/**/*.zig'
```

and confirming at least one caller passes something other than `hx.alloc`.

## Scope / status

Spec: `docs/v2/active/M173_001_P1_API_ERROR_PATH_COVERAGE_SWEEP.md` (IN_PROGRESS).

**§1 was re-cut this session on Indy's decision** (verbatim quote in Discovery):
prove every rung that can leak in production, in BOTH tiers, and fix none of the
cosmetic ones. That exposed the Dimensions were aimed by directory while the
leak-capable rungs live elsewhere — old D1.2 owned 122 handler rungs, none of
which can leak, while 189 leak-capable rungs had no Dimension at all.

- ✅ Caller/allocator audit done, instrument in the repo (`classify_rung_callers.py`
  + self-test, 9 tests green).
- ✅ §1 Dimensions re-cut by caller class; 1.5 now "zero leak-capable rungs
  unproven"; new 1.6 (boundary cannot rot) and 1.7 (hit-but-unasserted rungs).
- ✅ R1 regraded, R1b/R1c/R1d added.
- 🟢 **Dimension 1.1 is probably ALREADY DONE.** Of its 8 files only
  `repair_verifications` (19 rungs) has a genuine long-lived caller
  (`repair_verification_dispatcher`), and it was already proven before this
  session by `repair_verifications_unwind_integration_test.zig`. The other 29
  rungs are arena-backed at function level. **Verify this before declaring it.**
- ⏳ Dimensions 1.2, 1.3 — ~229 rungs, NEVER caller-checked, sitting behind the
  same inflated signal. Expect a large fraction to be cosmetic.
- ⏳ 1.4, 1.6, 1.7 untouched. §2–§5 untouched.

## The open decision

**Fix the classifier to resolve at FUNCTION level (call graph, not import graph)
before writing any more proofs.** That is my recommendation and Indy has not
ruled. Everything downstream depends on the work list, and on this session's
evidence it is wrong more often than right. The alternative — hand-checking each
function — is exactly the manual step that failed three times tonight while being
done carefully.

## Working tree

Clean. `feat/m173-error-path-coverage`, **2 commits ahead of origin**
(`b29f47490`, `f77599f09` — both docs/tooling, hooks-green locally, not yet pushed).

Eight commits this session; six pushed through `7c587963b`.

## Branch / PR (GitHub)

- Branch: `feat/m173-error-path-coverage`. No PR yet — this branch is meant to LAND.
- `origin/main` merged this session (`d0617c999`); pre-PR gate satisfied at that point.
- **Never force-push.**

## Running processes

`agentsfleet-m173-{postgres,redis,qstash}-1` UP on **25796 / 25797 / 25798**,
migrated (47 versions), left running deliberately. `make down` when finished.
`agentsfleetd-api` is NOT running — it cannot boot without the four variables
below, which is expected.

No tmux. `buildx_buildkit_ci-zig-builder0` is up.

## Tests / checks

- ✅ `make check-version` — `all versions match 0.26.2`.
- ⚠️ `make lint-all` — passed, but **STALE**: it ran BEFORE
  `classify_rung_callers*.py` existed, and pre-commit does not treat `.py` as
  lint-relevant, so those files' self-test has never run under `lint-governance`.
  **Re-run it.**
- 🛑 `make test-unit-all` — started, then **killed deliberately** when Indy
  pointed out that grading a still-growing branch means re-running every gate at
  CHORE(close) anyway. Ungraded.
- ⏳ `make memleak`, cross-compile ×2, `gitleaks detect` — never run.
- ✅ Zig unit graph green at each commit (`69/69` filtered runs); depth gate
  `unit=4284 integration=737` against the 4205/719 CHORE(open) baseline.
- ✅ CLI unit lane `1624 pass, 0 fail` on Bun 1.4.0 AND 1.3.14.
- ⏳ Classifier coverage report still stale — re-measure before grading R1–R4.

## Next steps

1. Push the 2 local commits.
2. Correct the false leak-capable claim in the `secret_probe` commit's wake —
   record it in Discovery beside the other three corrections.
3. Get Indy's ruling on function-level resolution (see The open decision).
4. Verify and then declare Dimension 1.1 done, or find what is left.
5. Only then: Dimensions 1.2/1.3, caller-checked per function.
6. Grade ALL gates once, at the end — not before.

## Risks / gotchas

### Bun version drift

Local `bun` resolves through the GLOBAL mise config to `latest` (1.4.0); CI pins
**1.3.14**; the repo pins nothing. `repl.ts` is guarded so both pass, but any
other 1.4 behaviour change surfaces locally and not in CI, or vice versa. Prefix
with `mise exec bun@1.3.14 --` to reproduce CI. Indy declined a repo pin.

### The traps, all in the spec's Discovery

1. **An optional rung is only an allocation site when its column is non-null.**
   The fixture IS the proof. `vault_metadata_alloc_test.zig` is the worked
   example — its mutation leaks exactly 17 bytes, the length of
   `"openai-compatible"`.
2. **The counting run COMMITS.** `checkAllAllocationFailures` runs once on a
   working allocator to count sites. If the function writes, that run commits and
   every failing run afterwards takes the replay branch. Reset at the top of each
   run, through the connection, never the failing allocator. `repair_verifications`
   sidesteps it differently — a hand-rolled `FailingAllocator` loop that advances
   the clock past the stale-reclaim window each iteration.
3. **A randomised generator aborts the proof** as `NondeterministicMemoryUsage`.
   Drive the inner function and inject a fixed-length one.
4. **Mutation-check by deleting the RUNG.** The only signal separating a real
   proof from a decorative one.
5. **A rung that RUNS is not a rung that WORKS** (new, Dimension 1.7). 194 of 301
   leak-capable rungs are HIT and therefore invisible to §1's other Dimensions;
   46 of those are in files with no failure-injection test at all. The worked
   example is `secret_reference_txn.begin` — its `errdefer txn.abort()` was
   executed by an existing test that asserted nothing about it, and deleting the
   rung left everything green. The harm is not a leak: the pooled connection is
   left `idle in transaction`, holding its snapshot and blocking vacuum.

### Two failure-injection mechanisms, not one

Grepping only `checkAllAllocationFailures` MISSES existing proofs — this session
twice concluded a file was unproven when it was not. `repair_verifications` and
`tenant_model_entries` use a hand-rolled `std.testing.FailingAllocator` loop.
Always grep for **both**.

### Harness traps

- **A skipped integration test reports as PASSING.** Mutation-check every new one.
- **`TEST_FILTER` is per-graph.** It filters only files registered in
  `integration_tests.zig`. Anything registered through `src/agentsfleetd/tests.zig`
  is UNIT-graph and matches nothing there. For those:

  ```
  ZIG_GLOBAL_CACHE_DIR=~/.cache/agentsfleet/zig-global-cache \
  ZIG_LOCAL_CACHE_DIR=.tmp/zig-local-cache LIVE_DB=1 \
  TEST_DATABASE_URL="postgres://agentsfleet:agentsfleet@localhost:25796/agentsfleetdb?sslmode=disable" \
  zig build test -Dtest-filter=<token> --summary all
  ```

  `--summary all` is load-bearing: read `Build Summary: N/N steps succeeded; M/M
  tests passed`. A zero-match run prints like a pass. Note `-Dtest-filter` often
  does NOT narrow as expected — a run reporting 69/69 is the whole registered set.
- **`make test-integration TEST_FILTER=…` exits non-zero on a clean run** — the
  tally check does not recognise `All N tests passed.`. Read the line above it.
- **To raise the datastores without a lane:** `make _ensure-test-infra`, then
  `zig build run -- migrate` with `DATABASE_URL_MIGRATOR` set. There is no
  `migrate` build step; it is `run -- migrate`.
- **Never run a lane while a Zig commit is in its hooks** — pre-commit runs a real
  filtered `make test-integration` under kcov.
- **A filtered lane clobbers the merged report.** Re-run both producers before grading.
- **Pre-commit does not treat `.py` as lint-relevant**, so a new
  `scripts/*_test.py` is not run by the hook. Only `make lint-all` catches it.

### `make up` needs four variables

`OIDC_ISSUER`, `OIDC_AUDIENCE`, `AUTH_SESSION_CODE_PEPPER`, `AUDIT_LOG_PEPPER`
(both peppers 64 hex) in `.env.agentsfleetd.local`, via `provision-env-1password`.
`make up` now waits for health and reports this rather than advertising a URL
nothing is listening on. No dev default was invented and no scanner suppression
added — Indy's call.

### The fleet-delete follow-up — start from the code, not the write-up

Withdrawn this session; its premise was false. `ingress/qstash.zig` answers
`hx.ok(200, accepted:true)` for EVERY outcome and logs `.schedule_missing` at
**debug**, so an orphan schedule fires forever, unobserved and billed. Three
coupled decisions recorded in Discovery, including a separate live defect:
`DELETE` on a fleet nobody killed first cancels every schedule and THEN answers
409. `create.zig` carries the same shape.

### Known residue

`inline_test_lines` drops lines inside a `test {}` block but not helpers beside
one: **86 lines** of test support in the coverage denominator, ≈0.03 points of
rate inflation. Matters to §5 — a floor raised on an inflated rate cannot be met
once the inflation goes.

### Not ours

`deploy (dev)` / `cli-acceptance-dev` red on `main` since Aug 19.
