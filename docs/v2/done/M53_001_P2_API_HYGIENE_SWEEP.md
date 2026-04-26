# M53_001: Hygiene Sweep — Atomics, Mutex Pairing, Maybe(T), errno Logs, snake_case Lint

**Prototype:** v2.0.0
**Milestone:** M53
**Workstream:** 001
**Date:** Apr 26, 2026
**Status:** DONE (partial — §2 landed; §1/§3/§4/§5 deferred, see Discovery)
**Priority:** P2 — codebase hygiene. Comment-and-grep sweep across the Zig surface to land five small, independently-bisectable correctness/style improvements.
**Categories:** API
**Batch:** B1 — independent of in-flight milestones. No public-API or schema changes.
**Branch:** feat/m53-hygiene-sweep
**Depends on:** M40 (worker substrate vendored `src/sys/errno.zig` + `src/sys/error.zig` — this spec adopts them).

**Coordinates with:** M52_001 (Bun Vendor Utilities). M52 §2 replaces the mutex+lock pattern in `src/cmd/worker_watcher.zig` with `UnboundedQueue`. If M52 lands first, this spec's §2 mutex audit on that file is largely moot — re-grep at PLAN. If this spec lands first, M52 rebases against the audited code. Either order works; the second spec to land re-verifies its diff against the first's gates.

**Canonical architecture:** N/A — no architectural change. Pure hygiene sweep.

---

## Implementing agent — read these first

1. `docs/ZIG_RULES.md` — the rule source for atomics commentary, mutex/`defer` pairing, `Maybe(T)` adoption boundary, and naming convention. Specifically: atomic-ordering rule (look for atomics section), pg-drain-style "defer immediately after acquire" pattern, and the new `Maybe(T)` policy in the post-M40 sys/fs paths.
2. `src/sys/errno.zig` — `errno.nameOf(rc)` is the replacement for `@errorName(err)` in syscall logging paths.
3. `src/sys/error.zig` — defines `Maybe(T)`. Adopt only in NEW sys/fs code. Existing DB / HTTP `!T` returns stay.
4. `.zlint.yaml` (or wherever the snake_case lint rule lives — locate before flipping). Snake_case is currently disabled; this spec flips it on and fixes the resulting violations.

If a referenced file does not exist, surface that in the PLAN output before EXECUTE — do not invent a config path.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal.
- `docs/ZIG_RULES.md` — atomics ordering commentary, mutex `defer` pairing, `Maybe(T)` adoption boundary, snake_case casing rule, `errno.nameOf` over `@errorName` in syscall logs.

---

## Anti-Patterns to Avoid

This spec is a sweep. The trap is bundling unrelated cleanup. Each section's diff must contain ONLY that section's concern. If the agent finds incidental issues outside scope, file a follow-up spec — do not fold in.

---

## Overview

**Goal (testable):** Five mechanical Zig-wide sweeps land as five sequential commits on one branch: every atomic op carries a `// safe because:` justification, every `lock()` is followed by `defer unlock()`, all new sys/fs paths return `Maybe(T)`, every syscall-error log uses `errno.nameOf`, and the snake_case lint is on with zero violations. `make lint`, `make test`, `make test-integration`, `make memleak`, and cross-compile (x86_64-linux + aarch64-linux) all green.

**Problem:** Five pieces of latent debt accumulated through M40. None blocks shipping; together they form review friction and slow down rule enforcement on new code.

- Atomics in `src/queue/`, `src/cmd/worker_*`, observability counters carry `.acquire` / `.release` ordering with no comment naming the synchronization contract.
- A scattering of `mutex.lock()` calls don't `defer mutex.unlock()` on the immediately-following non-blank line — the codebase otherwise uniformly pairs them.
- New post-M40 sys/fs code mixes `!T` return shapes with the vendored `Maybe(T)` pattern, defeating the boundary.
- Several syscall error sites (`pg`/`redis` connect, executor process management, file I/O) log via `@errorName(err)` instead of `errno.nameOf(rc)` — the latter gives clean POSIX names.
- ZLint's snake_case rule is disabled and at least some non-snake_case identifiers have crept in.

**Solution summary:** One branch, five commits, one section per concern. Each commit is a focused refactor; reviewer reads one diff and one rule at a time. No production semantics change — all five are mechanical or comment-only.

---

## Files Changed (blast radius)

> Final file list emerges from the EXECUTE-phase grep. Estimates below; agent prints exact list per section before each commit.

| File set | Action | Why |
|------|--------|-----|
| Files using atomics (grep `\.(acquire|release|monotonic|unordered)\b`) | EDIT | Add `// safe because:` comments naming the paired side |
| Files using mutexes (grep `\.lock\(\)`) | EDIT | Pair with `defer .*\.unlock\(\)` on next non-blank line |
| New post-M40 sys/fs files | EDIT | Adopt `Maybe(T)` return shape |
| Syscall-error logging sites (grep `@errorName` near `connect`/`open`/`read`/`write`/`spawn`) | EDIT | Replace with `errno.nameOf(rc)` |
| `.zlint.yaml` + snake_case violators | EDIT | Flip lint rule on; rename identifiers to satisfy |

**Estimate**: 25–60 files total across all five sections. Anything bigger and a section gets split out — surface in PLAN.

---

## Sections (implementation slices)

### §1 — Atomic-ordering commentary

Every `\.(acquire|release|monotonic|unordered)\b` site gets a `// safe because: <pairing>` comment within 3 lines. The comment names BOTH sides — which other site does the matching `.release` store, which does the matching `.acquire` load. `monotonic` and `unordered` get a comment justifying why ordering is unnecessary (e.g. "metrics counter, no other side reads via this address").

**Implementation default:** comment goes on the line directly above the atomic op unless that breaks readability of a chained expression.

### §2 — Mutex `defer unlock` pairing

Every `\.lock\(\)` is followed on the next non-blank line by `defer <self>\.unlock\(\)`. Audit every site; fix any deviation. If a site genuinely needs a non-defer manual unlock (rare — usually a sign of a too-large critical section), refactor the critical section into a helper rather than tolerate the unpaired call.

### §3 — `Maybe(T)` adoption in new sys/fs paths

Grep new (post-M40) code under `src/sys/fs/`, `src/sys/proc/`, `src/sys/net/` for `!T` return shapes. Migrate to `Maybe(T)` from `src/sys/error.zig`. **Do not retrofit** existing DB or HTTP code — that's explicitly out of scope.

**Implementation default:** if the function already returns a Zig error union and is only called by other sys/fs code, migrate. If it's called from HTTP handlers or DB code, leave it.

### §4 — `errno.nameOf` in syscall error logs

Grep for `@errorName(err)` adjacent to syscall failure logs (Redis connect, pg connect, executor process management, file I/O). Replace with `errno.nameOf(rc)` from `src/sys/errno.zig`. The `rc` value comes from the syscall's return — if the call site doesn't already have it, a tiny refactor preserving the original `err` handling is fine.

**Implementation default:** keep the `err` value too — log both Zig error name AND POSIX name when both are available.

### §5 — snake_case lint flip + renames

Locate the snake_case lint rule (likely `.zlint.yaml`). Flip it on. Run `make lint` — fix every reported violation by renaming the identifier and every reference.

**Implementation default:** if a violation is in a vendored file (`src/sys/`, `src/util/strings/` per their Bun-import header), preserve original casing and add a per-file lint exemption rather than fork the upstream identifier.

**Commit ordering**: §1 → §2 → §3 → §4 → §5. §5 last because rename diffs hide other regressions if landed alongside.

---

## Interfaces

N/A — sweep changes no public function signatures, no HTTP routes, no on-the-wire shapes. (§3 may rename internal `!T` to `Maybe(T)` on functions that have no callers outside `src/sys/`, which is by definition not a public surface.)

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Atomic comment names wrong pairing | Author misread call graph | `/review` skill + integration tests catch behavior change; comments don't change behavior so no runtime risk |
| Defer pairing introduces double-unlock | Existing manual unlock not removed when `defer` added | Compiler/runtime catches at first test pass; tier-3 integration mandatory |
| `Maybe(T)` migration breaks call site | Caller expected `!T` shape | `make test` + cross-compile catches; revert that file's commit if isolated |
| `errno.nameOf` called with non-errno rc | rc came from a non-syscall path | `nameOf` returns "UNKNOWN" or similar — verify the helper's contract before substitution |
| snake_case rename misses a reference | grep was too narrow | Build fails — fix and re-push |

---

## Invariants

1. Every atomic op site has a `// safe because:` comment within 3 lines — enforced by a one-liner grep gate added to `make lint` (or run pre-PR).
2. Every `lock()` is followed by `defer …unlock()` on the next non-blank line — enforced by grep gate.
3. snake_case lint rule is `error` not `warn` not `off` — enforced by ZLint config.

If a grep gate is impractical for any of the above (e.g. multi-line expressions confuse `grep`), document the trade-off in the section's commit message and rely on the `/review` skill.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_atomic_comment_grep` | Grep gate returns zero atomic-ops without a nearby `// safe because:` comment |
| `test_mutex_defer_grep` | Grep gate returns zero `\.lock\(\)` without `defer .*\.unlock\(\)` on the next non-blank line |
| `test_maybe_t_sys_fs` | The migrated sys/fs functions return `Maybe(T)` per the type system — compiler enforces |
| `test_errno_name_log_format` | Sample syscall failure produces a log line containing the POSIX name (e.g. `ECONNREFUSED`) not just the Zig error name |
| `test_zlint_snake_case_enforced` | `make lint` exits non-zero on a fixture file containing a CamelCase identifier |

Existing `make test` + `make test-integration` continue to pass — that IS the regression test for §1, §2, §3, §4 (no behavior should change).

---

## Acceptance Criteria

- [ ] §1 grep gate clean — verify: `make lint` (or the gate it wires)
- [ ] §2 grep gate clean — verify: `make lint`
- [ ] §3 — every new sys/fs function returns `Maybe(T)` — verify: agent lists migrated functions in commit message; reviewer cross-checks
- [ ] §4 — `@errorName` not present in syscall error log lines — verify: `grep -rn '@errorName' src/ | grep -i 'connect\|spawn\|open' | head` returns 0 lines
- [ ] §5 — `make lint` passes with snake_case rule enabled
- [ ] `make test` passes
- [ ] `make test-integration` passes (tier 2)
- [ ] `make down && make up && make test-integration` passes (tier 3, branch-level)
- [ ] `make memleak` clean
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean
- [ ] No file over 350 lines added; no function over 50 lines

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: atomic-ordering commentary present
rg '\.(acquire|release|monotonic|unordered)\b' src/ -B2 -A1 | rg -v 'safe because' | head

# E2: mutex defer pairing
rg -n '\.lock\(\)' src/ -A1 | rg -v 'defer.*unlock' | head

# E3: errno adoption — no @errorName near syscall failure logs
rg -n '@errorName' src/ | rg -i 'connect|spawn|open|read|write' | head

# E4: snake_case lint enforced
make lint 2>&1 | rg -i 'snake' | head

# E5: build + tests
zig build && make test && make test-integration

# E6: cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E7: memleak
make memleak 2>&1 | tail -3

# E8: gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

N/A — no files deleted. §5 renames identifiers; the renames are full-codebase grep-and-replace, which serves as its own orphan check (build fails on stale references).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill |
|------|-------|
| After implementation, before CHORE(close) | `/write-unit-test` — confirms grep-gate tests + the `Maybe(T)` regression test exist |
| Before CHORE(close) | `/review` — adversarial pass against ZIG_RULES.md, especially atomic ordering and mutex pairing |
| After `gh pr create` | `/review-pr` |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Lint (incl. snake_case) | `make lint` | | |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | | |
| Memleak | `make memleak` | | |
| Gitleaks | `gitleaks detect` | | |
| 350L gate | `wc -l` | | |

---

## Out of Scope

- Pub audit (M54_001).
- Allocator-ownership documentation/struct field additions (M54_001).
- String utility adoption — StringBuilder / StringJoiner / SmolStr (M55_001).
- Pub `///` doc-comments — explicitly deferred per author decision Apr 26, 2026.
- Retrofitting `Maybe(T)` into existing DB / HTTP `!T` return shapes — explicitly out per the boundary in `src/sys/error.zig` and ZIG_RULES.md.

---

## Discovery — Apr 26, 2026 PLAN-phase findings

PLAN grep over the actual tree found four of the five sections infeasible as written. §2 alone landed in this spec; the others split out. Recording here so the follow-up specs inherit context.

### §1 — atomic-ordering commentary: scope explosion

Spec estimate: 25–60 files total across all five sections.
Reality: §1 alone is 454 production atomic-op sites across 71 files (525 sites with tests). Adding `// safe because: <pairing>` comments that *correctly name the matching site* — which is what the spec demands, not a rubber-stamp — requires real call-graph analysis per site, not a grep sweep. Bundled into one workstream this is unreviewable.

**Disposition:** split into per-subsystem follow-ups. Suggested partition: `executor/`, `observability/`, `queue/` + `events/`, `cmd/worker*` + `state/`, plus a residual sweep for the rest. Each becomes its own workstream with its own grep gate.

### §3 — `Maybe(T)` adoption in new sys/fs paths: directories don't exist

Spec premise: post-M40 `src/sys/fs/`, `src/sys/proc/`, `src/sys/net/` carry `!T` returns to migrate.
Reality: `src/sys/` contains only `errno.zig` and `error.zig`. None of those subdirectories exist on `main`. M40 vendored the helpers but no callers landed yet.

**Disposition:** revisit when the first sys/fs caller lands. Either fold into that caller's spec or file a new workstream then.

### §4 — `errno.nameOf` in syscall error logs: no rc-bearing sites

Spec premise: replace `@errorName(err)` with `errno.nameOf(rc)` at syscall failure log sites.
Reality: the codebase uses `std.posix.*` wrappers exclusively (`std.posix.accept`, `std.posix.poll`, `std.posix.connect`, etc.). These wrappers consume the errno internally and return Zig error unions — no rc is exposed at the call site. The only `std.os.linux.*` usage is `getpid()`. The only `std.c._errno()` reference is inside `src/sys/errno.zig` itself.

Substituting `errno.nameOf(rc)` at any of the 25 candidate log sites would require rewriting `std.posix.*` calls as raw `std.os.linux.*` calls and threading rc forward — far outside this spec's "comment-and-grep" framing.

**Disposition:** the `errno.nameOf` helper stays useful for any future code that does call raw syscalls, but the existing surface has nothing to convert. Close as no-op unless a future spec introduces raw-syscall paths.

### §5 — snake_case lint flip: rule does not exist in zlint v0.7.9

Spec premise: flip the snake_case rule on in `.zlint.yaml` and fix violations.
Reality: the project's zlint config is `zlint.json` (not `.zlint.yaml`). zlint v0.7.9 (`/Users/kishore/bin/zlint`) reports both `snake_case` and `naming-convention` as unknown rule names. The four rules its config recognizes are `avoid-as`, `no-catch-return`, `suppressed-errors`, `unsafe-undefined` — none related to identifier casing.

**Disposition:** either upgrade zlint to a version with a casing rule (no-op research follow-up), or write a lightweight Python lint pass alongside `lint-zig.py`. Either way, out of scope here.

### §2 — landed

Two unpaired `.lock()` sites — `src/events/bus.zig` (cond-wait pattern) and `src/cmd/worker_watcher.zig:cancelZombie` (UAF-window manual unlock). Both refactored by extracting the locked critical section into a defer-pairing helper, per the spec's preferred fix. No behavior change. Build + bus.zig tests green. Commit `b2da9e00`.
