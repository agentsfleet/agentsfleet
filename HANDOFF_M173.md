# Handoff — M173_001 error-path coverage sweep

> Ephemeral. Delete at CHORE(close), per `AGENTS.md` §Required outputs.

## Scope / status

Spec: `docs/v2/active/M173_001_P1_API_ERROR_PATH_COVERAGE_SWEEP.md` (IN_PROGRESS).
Goal: every unhit line in the Zig tree is either executed by a test that asserts
something, or proven unreachable and deleted, with floors raised in the same
commit.

**Indy's standing decisions (do not re-open):**
- **Full sweep on one long branch** — all four classes to zero before the PR
  opens. Staging across workstreams was offered and rejected.
- **"Cover the positive, edge cases, performance, concurrency tests"** — every
  module touched also carries a behaviour assertion, boundary cases, a
  >=100-thread contention proof, and a counter-based complexity bound where the
  module supports them. `/write-unit-test`'s Definition of Done is the checklist.

**Baseline class counts** (pre-M173 merged report, `coverage/zig/merged/cobertura.xml`):

| class | lines | mechanism |
|---|---|---|
| errdefer | 317 | `checkAllAllocationFailures` |
| failure-response | 512 | inject the failure the arm answers |
| failure-log | 298 | same tests, assert the log line |
| error-return | 130 | construct the triggering input |
| other | 1109 | triage: test / delete / annotate |
| brace | 48 | report artefact, no test owed |

- ✅ Classifier built (`scripts/classify_unhit_lines.py` + 20 self-tests) — the
  grading instrument for rubric rows R1–R4.
- 🔄 §1 allocation-failure proofs — ~12% done (~40 of 317 lines).
- 🔄 §2 failure arms — rejection arms started; **the pool-exhaustion mechanism
  is built and proven**, which unlocks the 81 pool-acquire arms.
- ⏳ §3 error returns — not started.
- ⏳ §4 branch triage — not started.
- ⏳ §5 floors — not started; must move in the same commit as the tests that
  clear them.

**Two real production leaks found and fixed** (logged in the spec's leak log):
1. `src/agentsfleetd/auth/jwks.zig` `parseJwks` — built three owned fields inside
   the `append` argument list, so a decode failure on `modulus` left `kid`
   allocated and unreferenced. The `errdefer` only walks `keys.items`, and the
   key was never appended. Compounds per JWKS refresh for the process lifetime.
2. `src/agentsfleetd/fleet_runtime/yaml_frontmatter.zig` — handed the raw
   allocator to vendored `zig_yaml`, whose `Parser.init` leaks an `ErrorBundle`
   when a later allocation fails. Upstream defect, our exposure (every library
   import + fleet-config parse). Now arena-bounded.

## Working tree

Clean. `feat/m173-error-path-coverage` == `origin/...`, nothing unpushed.
Worktree: `/Users/kishore/Projects/agentsfleet-m173` (stay inside it).

## Branch / PR (GitHub)

- Branch: `feat/m173-error-path-coverage`, contains `origin/main` (`d09243bd1`).
- PR: **none yet** — correct, the sweep is nowhere near CHORE(close).
- 12 commits; every one passed `harness-verify` + lint + the depth gate.

## Running processes

No tmux sessions. Docker infra is UP for this worktree (leave it, or `make down`):

```
agentsfleet-m173-postgres-1   :25796 -> 5432
agentsfleet-m173-redis-1      :25797 -> 6379
agentsfleet-m173-qstash-1     :25798 -> 8080
```

## Tests / checks

- ✅ `make test-integration` — `1001 passed; 8 skipped; 0 failed`.
- ✅ `zig build test` (full daemon unit suite) — green.
- ✅ Test depth gate — `unit=4226 integration=726` (baseline `unit=4205
  integration=719`), so +21 unit / +7 integration.
- ⏳ `make lint-all`, `make test-unit-all`, `make memleak`, `make check-version`
  — not run this session.
- ⏳ `make test-coverage-grade` — **not green**: the unit-coverage evidence
  digest went stale after later source edits. Re-run
  `make test-coverage-zig && make test-integration && make test-coverage-grade`
  to get a current merged report before grading R1–R4.

## Next steps

1. Re-run both coverage producers, then `make test-coverage-grade`, and
   re-classify against the FRESH merged report. Every count in this document is
   a pre-M173 baseline.
2. Extend the pool-exhaustion suite — the mechanism in
   `src/agentsfleetd/http/pool_exhaustion_integration_test.zig` is one line per
   additional endpoint. 81 arms are reachable this way; 4 are closed.
3. Build the §2.1 mechanism (terminate the backend mid-statement) for the 83
   db-failure + 80 internal-op arms. Nothing exists for it yet.
4. Continue §1 on the DB-free targets — 255 of 317 errdefer lines need no
   database. Worklist recipe is in this file under "Gotchas".
5. Only then §3 and §4, and §5 floors last.

## Risks / gotchas

- **A skipped integration test reports as PASSING.** This is the big one.
  `TestHarness.start` returns `SkipZigTest` when Postgres OR Redis is
  unconfigured, every test converts that to a skip, and the lane exits 0. Three
  tests were green here having never executed an assertion. **`make
  test-integration` is the only lane that configures both datastores** —
  `zig build test-integration` and even `make test-integration-db` (no
  `REDIS_URL_API`) skip the whole HTTP-harness family. Never trust a green tick
  on a new integration test; mutation-check it, or inject an error as the
  test's FIRST statement to prove the body runs.
- **The harness HTTP client cannot send a bodiless PUT/POST.** It fails at the
  transport and takes the whole lane down. The "Request body required" arms are
  therefore unreachable from the harness and are deliberately left open — do
  not credit them to the empty-body case, which lands on the adjacent
  malformed-JSON arm (that is the padding Dimension 4.4 bans).
- **Read the error registry before asserting a status.** `UZ-APIKEY-007` is
  409, not the 400 it reads like. `entries` in
  `src/agentsfleetd/errors/error_entries*.zig` is the source of truth.
- **A perf assertion can name the wrong quantity.** A ladder asserting
  allocation CALL count was flat failed 32/64/128 → 33/65/129; `retain`
  deliberately allocates before taking the lock to keep the critical section
  non-fallible. Assert HELD bytes. Do not "fix" code to satisfy a counter
  before checking which quantity the design actually promises.
- **Milestone markers are banned in code comments** (RULE TST-NAM). `// ...
  (M173 §2.3)` fails `harness-verify`. Put the milestone context in the spec.
- **`zig fmt` before committing** — the pre-commit `make-graph` lane fails on
  formatting with an unhelpful `_fmt_check` error.
- **Iteration is slow.** A full daemon unit rebuild is ~6 min (LLVM debug), and
  `make test-integration` is ~15 min. Batch several tests, then validate once.
- **Pre-existing flakes:** running the integration binary directly without the
  make lane's DB reset produces `IndexNotChosen` failures in
  `db.index_usage_integration_test` — empty tables make the planner pick a seq
  scan. Not a regression.
- **R6 was amended.** As authored it demanded deletion-only product diffs,
  which forbids the leak fixes the proofs exist to produce. It now allows a
  cleanup fix named in the spec's leak log. Three other spec errors were also
  amended (floor values live in `make/test.mk`, not
  `scripts/check_zig_coverage_floors.py`).

### Worklist recipe

```bash
cd scripts
python3 - <<'PY'
import sys, collections, re; sys.path.insert(0, ".")
from pathlib import Path
import classify_unhit_lines as C
found = C.classify(Path("../coverage/zig/merged/cobertura.xml"), Path(".."))
FN = re.compile(r"^(pub\s+)?fn\s+(\w+)\s*\(")
by_file = collections.defaultdict(list)
for f in (x for x in found if x.kind == "errdefer"):
    by_file[f.path].append(f.number)
for path, nums in sorted(by_file.items(), key=lambda kv: -len(kv[1])):
    lines = (Path("..")/path).read_text(errors="replace").splitlines()
    for n in sorted(nums):
        for i in range(n-1, -1, -1):
            m = FN.match(lines[i])
            if m:
                sig = lines[i].strip()
                needs_db = "*PgQuery" in sig or "pg.Conn" in sig or "conn" in sig.lower()
                print(f"{path}:{i+1} {'DB ' if needs_db else 'PURE'} {m.group(2)}")
                break
PY
```

Split at last measurement: 255 of 317 errdefer lines need no database
(131 private + 124 public); 63 need one.
