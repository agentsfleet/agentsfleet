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

**Class counts** — Aug 21, 2026 re-measurement, after re-running both producers
(`make test-coverage-zig`, `make test-integration`) and `make test-coverage-grade`
(exit 0: merged 91.16% / floor 89, `agentsfleetd` 90.78% / floor 90, `lib` 95.30%,
`runner` 92.64%; integration 1003 passed, 8 skipped, 0 failed):

| class | Aug 20 authored | Aug 21 measured | mechanism |
|---|---|---|---|
| errdefer | 318 | **308** | `checkAllAllocationFailures` |
| failure-response | 512 | **510** | inject the failure the arm answers |
| failure-log | 298 | **304** | same tests, assert the log line |
| error-return | 132 | **128** | construct the triggering input |
| other | 1110 | **1093** | triage: test / delete / annotate |
| brace | 44 | **16** | report artefact, no test owed |
| **total** | **2414** | **2359** | |

**Read the deltas as NET, not as work done.** A proof that catches a leak
produces a fix that adds `errdefer` rungs — the `parseJwks` fix alone added 6 —
and the arms those fixes introduce carry log lines, which is why `failure-log`
moved *up*. Grade fresh-report to fresh-report from here; the Aug 20 column is
the authored baseline, not a comparison basis.

- ✅ Classifier built (`scripts/classify_unhit_lines.py` + 20 self-tests) — the
  grading instrument for rubric rows R1–R4.
- ✅ §2.2 pool exhaustion — the mechanism is a 50-row probe table covering 50
  endpoints with zero misses. HTTP-reachable acquire arms went 68 → ~21.
- 🔄 §1 allocation-failure proofs — 5 ladders landed this session (cron trigger,
  signed webhook, fleet network, whole-config, tar bundle) on top of the earlier
  work. `errdefer` class 308 and falling.
- ⏳ §3 error returns — not started.
- ⏳ §4 branch triage — not started. Largest remaining class (1093 + 15 brace).
- ⏳ §5 floors — not started; must move in the same commit as the tests that
  clear them. NOTE: the `agentsfleetd` target raise 90 → 92 exists ONLY as
  uncommitted work in the MAIN worktree (`~/Projects/agentsfleet`), not on this
  branch. Re-author it here when §5 lands; do not edit the other worktree.

**Landed Aug 21 (commit `1fb96d3a7`)** — the approved runner-twin fix, plus the
second half it turned out to need:

1. `src/runner/bundle_extract.zig` `readEntry` — the `WriteFailed` -> `OutOfMemory`
   switch, matching the daemon twin. ✅
2. Spec Files Changed carries the `src/runner/**/*.zig` EDIT row. ✅
3. `test_extract_support_files_unwinds_without_leaking` beside it. ✅ **Verified
   load-bearing**: reverting the switch makes it fail, with the trace running
   through `catch return error.CorruptArchive` into `checkAllAllocationFailures`.
4. Two leak-log rows — the twin, and the collapse below. ✅

**The twin fix alone was invisible.** `MaterializeResult` was `enum { ready,
failed }`, so `materializeBundle` folded plane refusal, malformed archive and out
of memory into one static detail covering both stages; a hosted user has no
runner log, so that line was everything they got. It now carries
`MaterializeFailure` (`download` / `malformed` / `memory`) — `causeOf` blames the
host for an allocation failure and the stage for everything else, `detailFor`
maps each to its own cause line. That restores the runner's own convention (six
`DETAIL_*` in `child_supervisor_result.zig`, fourteen in `selftest.zig`); this
call site was the only one that collapsed them.

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

Clean but for this handoff. `feat/m173-error-path-coverage` == `origin/...`;
nothing unpushed.
Worktree: `/Users/kishore/Projects/agentsfleet-m173` (stay inside it).

## Branch / PR (GitHub)

- Branch: `feat/m173-error-path-coverage`. It contains `8a8a69345`, but **`origin/main`
  has since moved to `faf563fa3`** — the branch is BEHIND. Merging it is a pre-PR
  gate, not urgent now, but do it before CHORE(close) and never force-push.
- PR: **none yet** — correct, the sweep is nowhere near CHORE(close).
- 21 commits, 7 of them from the Aug 21 session:

```
88a8d1d02 starve runner enrollment, the one runner route a session reaches
047992cc8 fix: a bundle import under memory pressure blamed the archive
80b9c54f9 prove the whole-config ladder, and record the session's traps
3fc037469 read the validators, then starve the six writes that rejected
7519fa5f1 starve the bodied writes whose bodies clear validation
27ec6f148 prove the cron, signed-webhook and network ladders
fed95b9c5 starve the pool against 35 endpoints, not four
```

## Running processes

No tmux (`tmux ls` -> no server). Docker infra is UP for this worktree:

```
agentsfleet-m173-postgres-1   :25796 -> 5432
agentsfleet-m173-redis-1      :25797 -> 6379
agentsfleet-m173-qstash-1     :25798 -> 8080
```

**A full re-measurement is IN FLIGHT** (started 09:53, ~45 min), launched from the
worktree root as:

```
make test-coverage-zig && make test-integration && make test-coverage-grade
```

It measures the POST-fix tree. Read the digest trap below before touching `src/`
or committing Zig while it runs — a docs-only commit is safe (the pre-commit hook
skips harness-verify when nothing lint-relevant is staged), a Zig one is not.

## Tests / checks

- ✅ `make test-coverage-grade` — exit 0. Merged 91.28% (floor 89),
  `agentsfleetd` 90.78% (floor 90), `lib` 95.30%, `runner` 92.64%.
- ✅ `make test-integration` — 1003 passed; 8 skipped; 0 failed.
- ✅ Filtered lane `make test-integration TEST_FILTER=pool_exhaustion` — all 83
  pass, probe loop reports zero misses across 50 endpoints.
- ✅ Test depth gate — `unit=4231 integration=726` (CHORE(open) baseline
  `unit=4205 integration=719`), so +26 unit / +7 integration.
- ✅ `make test-unit-agentsfleet-runner` — 670 pass, 3 skip (was 667).
- ✅ Depth gate at commit time — `unit=4235 integration=726` (baseline 4205/719).
- ⏳ `make lint-all`, `make test-unit-all`, `make memleak`, `make check-version`
  — NOT run this session. Every rubric S-row is still ungraded.

## Next steps

1. Read the in-flight lane's result, THEN re-classify with the two probes in the
   Worklist recipe below. Expect ~21 HTTP-reachable acquire arms; confirm rather
   than assume. If the lane was interrupted, re-run it whole — a filtered lane
   clobbers the merged report, and any `src/` edit refuses recorded evidence
   (`--source-path src`).
2. The remaining acquire arms are structural, not more rows:
   - 5 runner-self arms need `runner_bearer_mw` wired into `seedAndHarness`, or
     a second starvation test on the runner harness. **Pick deliberately.**
   - ~10 need webhook-signature fixtures (`identity_events_clerk`,
     `identity_events_delete`, `slack/events`, `ingress/github`).
   - 2 are SSE `authorize` paths, 2 are rollback-only paths inside create.
3. §1 continues. 308 `errdefer` lines; the PURE/DB split needs reading, not the
   heuristic (see the trap below).
4. §3 and §4 remain untouched — ~1,240 lines across ~250 files. This is still a
   multi-session milestone.

### Traps found Aug 21, 2026

- **A class count is NET, not progress.** Proofs that catch leaks produce fixes
  that add `errdefer` rungs, and the arms those fixes introduce carry log lines.
  `failure-log` went UP (298 → 304) during a session that closed 33 arms.
- **Grep the catch BODY, not the acquire line.** `pool.acquire() catch |err| {`
  runs on the success path, so it is almost never unhit. Walk back up to 6 lines
  from each unhit line instead.
- **DB-free is not dependency-free.** The signature heuristic (look for
  `pg.Conn` / `PgQuery` / `pool` across the WHOLE signature, not just its first
  line) still overcounts: `queue/redis_pool.zig` has no `pg.Conn` and needs a
  live Redis; `cron/Store.zig`'s row helpers take a `pg` row type. Treat the
  heuristic as a first filter and read the function before committing to it.
- **Read the validator before writing the body.** Six bodied probes answered 400
  from a rejection arm on plausible-looking bodies. Catalogue writes validate
  rates before acquiring; the runner patch needs exactly one of `action` /
  `assigned_policy`; the tenant model-entry patch validates `model_id`, not
  `secret_ref`. A 400 looks like a working test and colours the wrong line.
- **`PATCH {workspace}/fleets/{id}` answers 200 with the pool starved.** Not a
  bug: `patch.zig:65-74` short-circuits a patch carrying none of `config_json`,
  `status`, `trigger_markdown`, `source_markdown` to a no-op 200 before any
  acquire. Reaching its arm needs one of those four fields.
- **The filtered lane cannot report success when nothing skips.** Zig prints
  `All N tests passed.` with zero skips, and the lane's tally check only
  recognises `N passed; M skipped; K failed.` — so a clean `make test-integration
  TEST_FILTER=...` run ends with "reported no passing tests". The tests DID run;
  read the line above it. The full lane always has skips, so it reads fine.
- **Never run a lane while a commit is in its hooks.** The pre-commit self-test
  `scripts/check_zig_coverage_lanes_test.py` runs a REAL `make test-integration`
  inside itself. A lane running concurrently changes the coverage evidence
  `source_digest` underneath it, and the self-test fails on a mismatch it was
  never testing for — a red commit with nothing wrong in the diff.
- **The runner-self arms are not reachable from `seedAndHarness`.** It never
  wires `runner_bearer_mw`, so `heartbeat`, `self`, `memory` (x2) and
  `credentials_mint` answer 401 before any acquire. The runner token is a static
  fixture (`RUNNER_TOKEN_PREFIX ++ "f" ** 64`), not a mint — but it resolves only
  in a harness that registers the lookup, as
  `handlers/runner/memory_loop_integration_test.zig:81` does. Closing those five
  means widening the shared seed fixture or standing up a second starvation test
  on the runner harness; pick deliberately.
- **A filtered lane clobbers the merged report.** It rewrites
  `coverage/zig/integration` from a narrowed run, so re-run both producers in
  full before grading anything.

- **A tar-fixture built INSIDE an allocation-failure proof fails it with no
  product defect at all.** `std.tar.Writer` over an allocating writer converts
  OOM to `error.WriteFailed`, and `checkAllAllocationFailures` requires the
  function under test to answer `OutOfMemory`. Build fixtures outside the proof
  and pass them in as `extra_args`. Same shape bit the config proof: an INVALID
  fixture (a `write` repository binding with no `repository_base` in authoring
  mode) fails on the SUCCESS path and reads exactly like a defect. Read the
  trace before believing a red proof.

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

Two probes. The first splits the `errdefer` class; the second finds acquire arms.
Run BOTH from the repo root — the classifier resolves the report path relative to
the working directory and fails loudly (not silently) from anywhere else.

```bash
# errdefer worklist, split by whether the enclosing fn needs a connection.
# Reads the WHOLE signature, not just its first line: a multi-line signature
# carrying `conn: *pg.Conn` on line 3 scans as DB-free otherwise, which is how
# the earlier "255 of 317 need no database" figure was produced. Real split is
# 150 PURE / 158 DB. Still only a FIRST FILTER — `queue/redis_pool.zig` has no
# `pg.Conn` and needs a live Redis; row helpers take a `pg` row type. Read the
# function before committing to a target.
python3 - <<'PY'
import sys, collections, re
sys.path.insert(0, "scripts")
from pathlib import Path
import classify_unhit_lines as C
found = C.classify(Path("coverage/zig/merged/cobertura.xml"), Path("."))
FN = re.compile(r"^(pub\s+)?fn\s+(\w+)\s*\(")
def signature(lines, i):
    out = []
    for k in range(i, min(i + 14, len(lines))):
        out.append(lines[k])
        if lines[k].rstrip().endswith("{"):
            break
    return "\n".join(out)
by_file = collections.defaultdict(list)
for f in found:
    if f.kind == "errdefer":
        by_file[f.path].append(f.number)
rows = []
for path, nums in by_file.items():
    lines = (Path(".")/path).read_text(errors="replace").splitlines()
    for n in sorted(nums):
        for i in range(n-1, -1, -1):
            m = FN.match(lines[i].lstrip())
            if m:
                sig = signature(lines, i)
                db = ("pg.Conn" in sig) or ("PgQuery" in sig) or ("pool" in sig.lower())
                rows.append((path, n, "DB" if db else "PURE", m.group(2)))
                break
pure = [r for r in rows if r[2] == "PURE"]
print(f"errdefer {len(rows)}  PURE={len(pure)}  DB={len(rows)-len(pure)}")
for path, c in collections.Counter(r[0] for r in pure).most_common(25):
    print(f"{c:>3}  {path}")
PY

# acquire arms: walk BACK from each unhit line looking for the opener. Grepping
# for unhit `pool.acquire() catch` LINES finds ~1 hit in the whole tree, because
# that line runs on the success path too. It is the catch BODY that never ran.
python3 - <<'PY'
import sys, collections
sys.path.insert(0, "scripts")
from pathlib import Path
import classify_unhit_lines as C
found = C.classify(Path("coverage/zig/merged/cobertura.xml"), Path("."))
cache = {}
def src(p):
    if p not in cache:
        try: cache[p] = (Path(".")/p).read_text(errors="replace").splitlines()
        except Exception: cache[p] = []
    return cache[p]
hits = collections.defaultdict(list)
for f in found:
    lines = src(f.path); i = f.number - 1
    if i >= len(lines): continue
    for j in range(i, max(-1, i-6), -1):
        if "pool.acquire() catch" in lines[j]:
            hits[f.path].append(f.number); break
hnd = {p: v for p, v in hits.items() if "/http/handlers/" in p}
oth = {p: v for p, v in hits.items() if "/http/handlers/" not in p}
print(f"HTTP-reachable {sum(map(len, hnd.values()))} lines / {len(hnd)} files")
print(f"background     {sum(map(len, oth.values()))} lines / {len(oth)} files")
for p, v in sorted(hnd.items()):
    print(f"  {len(v)}  {p}")
PY
```
