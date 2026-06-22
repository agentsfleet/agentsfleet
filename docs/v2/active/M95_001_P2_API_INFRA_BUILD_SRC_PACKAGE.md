# M95_001: Consolidate the Zig build graph into a src/build/ package + fix build*.zig findings

**Prototype:** v2.0.0
**Milestone:** M95
**Workstream:** 001
**Date:** Jun 22, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — build-tooling hygiene; no user-facing behaviour change, but it removes server/runner drift risk and documents a TLS-on/off matrix that today reads as a bug.
**Categories:** API, INFRA
**Batch:** B1
**Branch:** chore/m95-build-hygiene
**Test Baseline:** unit=2015 integration=201
**Depends on:** none
**Provenance:** LLM-drafted (claude-opus-4-8, Jun 22 2026) — from Indy's direction + an adversarial audit of build*.zig this session.

> **Provenance is load-bearing.** LLM-drafted — cross-check every claim against the two build graphs before EXECUTE; the Files Changed table and the SharedDeps split are derived from a live read of build.zig + build_runner.zig, not assumed.

**Canonical architecture:** greenfield build-graph structure — the shape mirrors ghostty's `src/build/` package (see Prior-Art). `docs/architecture/direction.md` governs platform constants (run-to-run determinism); no build-graph architecture doc exists, so this spec + the ghostty reference define the shape.

---

## Implementing agent — read these first

1. `/Users/kishore/Projects/oss/ghostty/src/build/main.zig` + `Config.zig` + `SharedDeps.zig` — the reference package: a barrel that `pub const`-re-exports components, a struct-with-`init()` SharedDeps built once and passed `*const` to every artifact, lowercase `*.zig` fn-namespaces. Mirror the barrel + SharedDeps + naming; do NOT import its Config.zig options-centralization (out of scope).
2. `build.zig` + `build_runner.zig` (repo root) — the two current graphs. Note the shared module set both wire separately (`log`, `contract`, `common`, `nullclaw`) and the per-graph `build_options` (different `-D` options each — NOT shared).
3. `build_pg.zig` / `build_s3.zig` / `build_fixtures.zig` — the three helpers to relocate; `fixtures` is consumed by BOTH graphs (`addDaemon` + `addRunner`).
4. `dispatch/write_zig.md` — Zig authoring discipline (PUB-surface verdict, LENGTH, cross-compile both linux targets) for the new `src/build/*.zig`.
5. `make/quality.mk` `_legacy_symbols_check` — the sibling grep-guard whose shape the new `_legacy_noun_check` + runner-isolation guard mirror.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** refactor(build): consolidate helpers into src/build/ package + fix build findings
- **Intent (one sentence):** Cut root-level build sprawl to the two entry-point graphs, build the shared module set once so server and runner can't drift, and clear the build*.zig audit findings — with zero change to either shipped binary.
- **Handshake (agent fills at PLAN):** restate the intent and list `ASSUMPTIONS I'M MAKING:` before any edit. Mismatch with the Intent above → STOP and reconcile. Seed assumption: this is a pure build-graph reorg + audit remediation; if any change alters a compiled binary's bytes (beyond the intentional VERSION-warn log line), it is out of scope.

---

## Product Clarity

1. **Successful user moment** — a developer runs `ls build*.zig` at the repo root and sees exactly **two** files (the two entry-point graphs); every helper lives under `src/build/` behind one barrel; and `grep -c 'createModule' build.zig build_runner.zig` shows the shared module set is built once in `src/build/shared.zig`, not twice.
2. **Preserved behaviour** — `zig build`, `zig build --build-file build_runner.zig` (+ `run`/`test`/`test-integration`/`test-s3`/`test-bin`/`bench-*`), `make lint-zig`/`test`/`test-integration`, and cross-compile of both linux targets all produce identical artifacts and pass exactly as before.
3. **Optimal-way check** — the ghostty package pattern (barrel + SharedDeps) is the most direct delivery. Gap to unconstrained-optimal (ghostty's full `Config.zig` options struct + `retarget()`): deliberately deferred — it would balloon scope with no immediate payoff.
4. **Rebuild-vs-iterate** — iterate. A structural reorg, not a rebuild; the two graphs and every step/artifact stay. A determinism-preserving move. Verdict: refactor.
5. **What we build** — `src/build/` (barrel `main.zig` + `pg.zig`/`s3.zig`/`fixtures.zig`/`shared.zig`), updated imports in both graphs, a runner-isolation guard, a `requireZig` guard, the folded-in `_legacy_noun_check`, and the build*.zig finding fixes.
6. **What we do NOT build** — ghostty `Config.zig` options-centralization; any pg/s3 access from the runner; merging the two graphs; renaming the two entry-point files; rewriting the OpenSSL target-detection logic; daemon version surfacing (pending decision).
7. **Fit** — compounds with the cross-compile + `lint-zig` gates; must not destabilize the runner's zero-datastore isolation boundary.
8. **Surface order** — build-tooling only; no CLI/UI surface.
9. **Dashboard restraint** — N/A (no UI).
10. **Confused-user next step** — a developer unsure where a helper went runs `cat src/build/main.zig` (the barrel enumerates every component) or reads the moved file's header.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal. Specifically: **UFS** (repeated/semantic literals → named constants single-sourced; the `S_HTTPZ` + nullclaw-engine + git-commit fixes), **NDC** (no dead code — delete old root helpers, add no unconsumed options), **ORP** (orphan sweep — zero references to old paths), **NLR** (touch-it-fix-it cleanup), **FLL** (file-length — `build.zig` is at the 350 cap; relocation drops it under).
- **`dispatch/write_zig.md`** — every new/edited `*.zig`: PUB-surface verdict for `src/build/*.zig`, file/fn length, cross-compile both linux targets.
- **`make/quality.mk` guard conventions** — the `_legacy_noun_check` + runner-isolation guard are shell grep-guards mirroring `_legacy_symbols_check`.
- Not applicable: REST guidelines, SCHEMA conventions (`_legacy_noun_check` greps `schema/` but modifies no schema), LOGGING/LIFECYCLE/ERROR-REGISTRY (build scripts, no runtime surface).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — new `src/build/*.zig` + edits to both graphs | read `dispatch/write_zig.md`; cross-compile `x86_64-linux` + `aarch64-linux` + the runner musl/macos targets |
| PUB / Struct-Shape | yes — new pub surface | `pg`/`s3`/`fixtures` stay lowercase fn-namespaces (no primary type); `shared.zig` exposes a struct-with-`init()` (SharedDeps); barrel re-exports. Document the surface. |
| File & Function Length (≤350/≤50/≤70) | yes | relocation drops `build.zig` under the cap; each new file well under 350; no fn over 50 |
| UFS (repeated/semantic literals) | yes | `S_HTTPZ`; nullclaw engine/channel + git-commit strings single-sourced in `shared.zig`; module-name `S_*` consts owned once |
| MILESTONE-ID / LOGGING / SCHEMA / UI / DESIGN TOKEN | no | build scripts only — no runtime logging, schema, or UI surface |

---

## Overview

**Goal (testable):** Root holds exactly two `build*.zig` entry points; the `log`/`contract`/`common`/`nullclaw` module set is constructed once in `src/build/shared.zig` and consumed by both graphs; `build_runner`'s graph resolves neither the `pg` nor the `s3` module (guard-enforced); every build*.zig audit finding is fixed or documented; both binaries build and cross-compile byte-identically.

**Problem:** Five `build*.zig` files at the repo root (two graphs + three helpers) read as sprawl. The two graphs each wire the same four shared modules separately, so they can silently drift. An audit surfaced finding clusters: an undocumented OpenSSL TLS-on/off matrix that reads as a bug, a raw `"httpz"` literal that escaped the `S_*` convention, a silently-swallowed VERSION read, and runner/daemon parity gaps.

**Solution summary:** Adopt ghostty's `src/build/` package — a `main.zig` barrel re-exporting the relocated `pg`/`s3`/`fixtures` helpers plus a `SharedDeps` (`shared.zig`) that builds the common module set once. Both graphs import the barrel; `pg`/`s3` stay daemon-only so the runner isolation boundary is preserved and now guard-enforced. Fold in the verified `_legacy_noun_check` guard and the build*.zig finding fixes. No behaviour change beyond a build-time warning when VERSION is unreadable.

---

## Prior-Art / Reference Implementations

- **`/Users/kishore/Projects/oss/ghostty/src/build/`** — barrel (`main.zig`), `SharedDeps.zig` (built once, passed `*const`), struct-with-`init()` components, lowercase fn-namespaces (`gtk.zig`/`zig.zig`), `requireZig` comptime guard read from `build.zig.zon`. **Mirror:** the barrel, SharedDeps, naming, and `requireZig`. **Diverge (justified):** keep TWO entry-point graphs (ghostty has one) because the runner is a compile-time security boundary; skip `Config.zig` options-centralization (deferred follow-up).
- No prior art for the OpenSSL/TLS documentation fix — it is a comment matrix describing existing behaviour, not new code.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `build.zig` | EDIT | import barrel; consume SharedDeps for the shared set; `S_HTTPZ`; hoist `build_options` module once |
| `build_runner.zig` | EDIT | import barrel; consume the same SharedDeps; warn on VERSION fallback; `-Dtest-filter` parity |
| `build_pg.zig` | DELETE | relocated → `src/build/pg.zig` |
| `build_s3.zig` | DELETE | relocated → `src/build/s3.zig` |
| `build_fixtures.zig` | DELETE | relocated → `src/build/fixtures.zig` |
| `src/build/main.zig` | CREATE | barrel: `pub const` re-export of `pg`/`s3`/`fixtures`/`shared` + `requireZig` |
| `src/build/pg.zig` | CREATE | moved `build_pg.zig` + OpenSSL TLS-matrix doc comment (§5) |
| `src/build/s3.zig` | CREATE | moved `build_s3.zig` (R2/z3 module + `test-s3` step), unchanged |
| `src/build/fixtures.zig` | CREATE | moved `build_fixtures.zig` (`addDaemon`/`addRunner`), unchanged |
| `src/build/shared.zig` | CREATE | SharedDeps: build `log`/`contract`/`common`/`nullclaw` once; single-source the nullclaw engine/channel + git-commit literals |
| `make/quality.mk` | EDIT | `_legacy_noun_check` (already done) + runner-isolation guard target, both wired into `lint-zig` |

> `build.zig.zon` `.paths` already includes `"src"`, so `src/build/` ships with no manifest edit. CI cache keys hash `build.zig`/`build_runner.zig` (kept) + `src/**/*.zig` (covers the new files) — no workflow change.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections, one Workstream — §1 relocate, §2 SharedDeps + isolation guard, §3 `requireZig`, §4 fold-in noun guard, §5 findings remediation. The reorg (§1–3) and the audit fixes (§4–5) ride one PR because they touch the same five files and Indy asked for them together.
- **Alternatives considered:** (a) full ghostty parity incl. `Config.zig` — rejected: scope balloon, no payoff now; (b) merge the two graphs behind a `-D` flag — rejected: forfeits the compile-time runner isolation; (c) rewrite `build_pg.zig`'s OpenSSL target detection to enable TLS on cross-builds — rejected: prod builds native-arch (TLS already on), and a rewrite risks both `make up` and the pre-baked CI Alpine image — documentation is the correct remediation.
- **Patch-vs-refactor verdict:** **refactor** for §1–3 (the sprawl + drift is structural); **targeted fixes** for §5. Follow-up named in Out of Scope (ghostty `Config.zig` parity) rather than mud-patched here.

---

## Sections (implementation slices)

### §1 — Relocate build helpers into a `src/build/` package — ✅ DONE

Move the three root helpers under `src/build/` behind a `main.zig` barrel; root keeps only the two entry-point graphs. Kills root sprawl; one discoverable package. **Implementation default:** lowercase `pg.zig`/`s3.zig`/`fixtures.zig` (they expose `pub fn`, no primary type) — match ghostty's namespace-file convention.

- **Dimension 1.1** — helpers relocated to `src/build/{pg,s3,fixtures}.zig`, content-equivalent; both graphs build → Test `test_build_graphs_green`.
- **Dimension 1.2** — `src/build/main.zig` barrel re-exports the three; both graphs import the barrel, not the old root paths → Test `test_no_root_helper_refs`.
- **Dimension 1.3** — old root files deleted from disk + git (Dead Code Sweep) → Test `test_old_helpers_deleted`.

### §2 — SharedDeps: build the common module set once

Extract `src/build/shared.zig` that constructs `log`/`contract`/`common`/`nullclaw` once (with `log`'s `common.clock` import); both graphs consume it. `pg`/`s3` stay daemon-only. `build_options` stays per-graph (the two binaries expose different `-D` options). This absorbs the audit's duplicated nullclaw engine/channel + git-commit literals (single-sourced here). Server and runner can no longer drift on the shared set; the isolation boundary becomes explicit + enforced.

- **Dimension 2.1** — `shared.zig` builds the shared set via one `init`; `build.zig` consumes it for agentsfleetd → Test `test_agentsfleetd_shared_deps`.
- **Dimension 2.2** — `build_runner.zig` consumes the SAME SharedDeps → Test `test_runner_shared_deps`.
- **Dimension 2.3** — runner-isolation invariant: `build_runner`'s graph resolves neither `pg` nor `s3` → Test `test_runner_isolation_guard` (a `make` grep-guard, with a negative test that wiring `pg` into the runner trips it).

### §3 — `requireZig` comptime guard

Add ghostty's `comptime { requireZig(minimum_zig_version); }` (read from `build.zig.zon`) to both graphs via the barrel. Fail fast + legibly on toolchain drift instead of a deep cryptic error.

- **Dimension 3.1** — the version-compare helper accepts `>= min`, rejects `< min` → Test `test_require_zig_compare` (pure comparison logic; a stale toolchain can't be installed in CI).

### §4 — Fold in the legacy-noun guard (already implemented) — ✅ DONE

The `_legacy_noun_check` make guard (forbids `\bzombie_id\b|\bzmb_id\b` in `src/` + `schema/`, wired into `lint-zig`, comment-line-exempt) — authored + verified this session, sitting uncommitted on this branch. Commit it as this Dimension. Ratchets against legacy-noun regression.

- **Dimension 4.1** — guard passes clean, is in the `lint-zig` chain, and fails on an injected `zombie_id`(.zig)/`zmb_id`(.sql) while ignoring `//`/`--` comment lines → Test `test_legacy_noun_guard`.

### §5 — build*.zig findings remediation

Clear the audit findings not absorbed by §2. **Implementation defaults:** the OpenSSL cluster is fixed by *documentation* (native-arch → TLS on; cross/local → TLS off by design, `-Dopenssl=true` forces on; the `{arch}-linux-gnu` paths depend on the pre-baked CI Alpine symlinks), NOT by rewriting target detection — prod builds native-arch so TLS is already on, and a rewrite risks `make up` + the CI image.

- **Dimension 5.1** — `S_HTTPZ` constant replaces all six raw `"httpz"` literals in `build.zig` (UFS) → Test `test_no_raw_httpz_literal`.
- **Dimension 5.2** — `build.zig` hoists the `build_options` module to a single `createModule()` (matching the runner), removing the double-create → Test `test_build_options_single_module`.
- **Dimension 5.3** — `build_runner.zig` emits a build warning when the VERSION read falls back to `"0.0.0"` instead of swallowing it → Test `test_version_fallback_warns`.
- **Dimension 5.4** — `build_runner.zig` exposes `-Dtest-filter` and threads `.filters` into all three runner test compilations → Test `test_runner_test_filter`.
- **Dimension 5.5** — `src/build/pg.zig` carries the OpenSSL TLS-matrix doc comment + the `-Dopenssl` override note → Test `test_openssl_intent_documented` (cross-compile unchanged).
- **Dimension 5.6** — version sourcing unified: both graphs embed `version`+`git_commit` via §2's shared helper; daemon gains `--version` printing `version (sha)` (the consumer keeping the option live, RULE NDC) → Test `test_daemon_version_surface`.

---

## Interfaces

```
src/build/main.zig (barrel) — pub const re-exports:
  pg / s3 / fixtures   the relocated helpers; existing pub fns unchanged
  shared               pub const SharedDeps
  requireZig           comptime version guard (from zig.zig)

src/build/shared.zig:
  pub const SharedDeps = struct {
    pub fn init(b, target, optimize, nullclaw_dep_opts) SharedDeps; // builds log/contract/common/nullclaw once
    pub fn module(self, name) *std.Build.Module;                    // accessor by S_* name; exposes NO pg/s3
  };
```

Contract: the relocated `pg`/`s3`/`fixtures` keep their existing public fns unchanged. SharedDeps exposes only the four shared modules; `build_options`, `pg`, `s3` are wired by the daemon graph alone.

---

## Failure Modes

| Mode | Cause | Handling (response + observable) |
|------|-------|----------------------------------|
| Stale root-path import | a missed reference to `build_pg.zig` etc. after the move | `zig build` fails fast (file not found); Dimension 1.2 grep guard catches it pre-build |
| Runner gains pg/s3 | a future edit wires `pg`/`s3` into `shared.zig` or the runner graph | runner-isolation guard (Dim 2.3) fails `lint-zig` with the offending reference |
| Stale toolchain | Zig `< minimum_zig_version` | `requireZig` comptime error with an upgrade message (Dim 3.1) |
| Legacy-noun regression | `zombie_id`/`zmb_id` reintroduced in `src/`/`schema/` | `_legacy_noun_check` fails `lint-zig` (Dim 4.1) |
| Unreadable VERSION | file renamed/missing | build warning surfaces it (Dim 5.3); was previously silent `"0.0.0"` |
| Cross-compile breakage | target-specific wiring lost in the move | both linux targets (+ runner musl/macos) cross-compiled in AC + CI |

---

## Invariants

1. Root contains exactly two `build*.zig` (`build.zig`, `build_runner.zig`) — enforced by Dead Code Sweep + the Dimension 1.2 grep (zero old-path references).
2. The shared module set is constructed exactly once (`src/build/shared.zig`) — enforced by single-source: no `createModule` of `log`/`contract`/`common`/`nullclaw` remains in either graph.
3. `build_runner`'s graph resolves neither `pg` nor `s3` — enforced by the runner-isolation `make` guard (Dim 2.3), wired into `lint-zig`.
4. No `zombie_id`/`zmb_id` in `src/`/`schema/` — enforced by `_legacy_noun_check` in `lint-zig` (Dim 4.1).
5. Both binaries build byte-identically pre/post (no behaviour change beyond the VERSION-warn line) — enforced by `test`/`test-integration`/cross-compile passing unchanged.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | integration | `test_build_graphs_green` | `zig build` and `zig build --build-file build_runner.zig` both compile after relocation |
| 1.2 | unit (grep) | `test_no_root_helper_refs` | `git grep -lE 'build_(pg\|s3\|fixtures)\.zig'` → 0 outside docs/CHANGELOG |
| 1.3 | unit (fs) | `test_old_helpers_deleted` | `test ! -f build_pg.zig && ! -f build_s3.zig && ! -f build_fixtures.zig` |
| 2.1 | integration | `test_agentsfleetd_shared_deps` | agentsfleetd builds + `make test` passes consuming SharedDeps |
| 2.2 | integration | `test_runner_shared_deps` | runner builds + its unit + integration suites pass consuming SharedDeps |
| 2.3 | unit (guard) | `test_runner_isolation_guard` | guard passes now; FAILS when `pg`/`s3` is wired into the runner graph (negative) |
| 3.1 | unit | `test_require_zig_compare` | `requireZig` accepts `>=` min, rejects `<` min (version-tuple compare) |
| 4.1 | unit (guard) | `test_legacy_noun_guard` | passes clean; fails on injected `zombie_id`(.zig)/`zmb_id`(.sql); ignores `//`/`--` comments |
| 5.1 | unit (grep) | `test_no_raw_httpz_literal` | only the `S_HTTPZ` definition contains the `"httpz"` literal in `build.zig` |
| 5.2 | integration | `test_build_options_single_module` | daemon builds; exactly one `build_opts.createModule()` in `build.zig` |
| 5.3 | unit | `test_version_fallback_warns` | the VERSION fallback path emits a warning (not silent) |
| 5.4 | integration | `test_runner_test_filter` | `zig build --build-file build_runner.zig test -Dtest-filter=__none__` runs zero tests |
| 5.5 | unit (review) | `test_openssl_intent_documented` | `src/build/pg.zig` documents the TLS matrix + `-Dopenssl`; cross-compile both linux targets still green |

**Regression:** the full existing `make test` + `make test-integration` suites must pass unchanged (binaries don't change). **Idempotency:** N/A — no retry semantics.

---

## Acceptance Criteria

- [ ] Root has exactly two build graphs — verify: `ls build*.zig` → `build.zig build_runner.zig`
- [ ] No old helper-path references — verify: `! git grep -lE 'build_(pg|s3|fixtures)\.zig' -- ':!docs' ':!CHANGELOG.md'`
- [ ] Shared set single-sourced — verify: `grep -rl 'src/lib/contract/contract.zig' build.zig build_runner.zig` → empty (only `src/build/shared.zig`)
- [ ] Runner-isolation guard passes + catches violation — verify: `make <isolation-target>` (+ negative probe)
- [ ] `_legacy_noun_check` wired + green — verify: `make _legacy_noun_check` && `make -n lint-zig | grep _legacy_noun_check`
- [ ] No raw `"httpz"` — verify: `grep -c '"httpz"' build.zig` → 1 (the `S_HTTPZ` def)
- [ ] `make lint` clean · `make test` passes · `make test-integration` passes
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` and `zig build --build-file build_runner.zig -Dtarget=x86_64-linux-musl`
- [ ] `gitleaks detect` clean · no file over 350 lines added (build.zig drops below cap)

---

## Eval Commands (post-implementation)

```bash
# E1: exactly two root build graphs
[ "$(ls build*.zig | tr '\n' ' ')" = "build.zig build_runner.zig " ] && echo PASS || echo FAIL
# E2: Build both graphs
zig build && zig build --build-file build_runner.zig && echo PASS || echo FAIL
# E3: Tests
make test && make test-integration 2>&1 | tail -3
# E4: Lint (incl. _legacy_noun_check + isolation guard)
make lint-zig 2>&1 | grep -E "✓|✗|noun"
# E5: Cross-compile
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo PASS
# E6: Orphan sweep (empty = pass)
git grep -lE 'build_(pg|s3|fixtures)\.zig' -- ':!docs' ':!CHANGELOG.md'
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `build_pg.zig` | `test ! -f build_pg.zig` |
| `build_s3.zig` | `test ! -f build_s3.zig` |
| `build_fixtures.zig` | `test ! -f build_fixtures.zig` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted import | Grep | Expected |
|----------------|------|----------|
| `build_pg.zig` / `build_s3.zig` / `build_fixtures.zig` | `git grep -nE 'build_(pg\|s3\|fixtures)\.zig' -- ':!docs' ':!CHANGELOG.md'` | 0 matches |

---

## Discovery (consult log)

- **Indy decisions (this session):** (a) fold the `_legacy_noun_check` guard into this spec rather than ship standalone (AskUserQuestion, Jun 22); (b) rename `chore/legacy-noun-ratchet` → `chore/m95-build-hygiene` and carry the guard change; (c) "fix all findings in build*.zig".
- **RESOLVED (Indy, Jun 22) — version-sourcing (audit P2):** git SHA is the canonical build identity in both binaries; semver `version` rides as a non-gating display label via §2's shared options helper (which collapses the duplicated git-commit option); the daemon gains a `--version` printing `version (sha)` — the consumer keeping the option live (RULE NDC). Tracked as Dimension 5.6.
- **Skill chain outcomes** — `/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs`: filled as run.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Diff coverage vs the Test Specification clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | Clean OR every finding dispositioned (vs spec, `dispatch/write_zig.md`, Failure Modes, Invariants) |
| After `gh pr create` | `/review-pr` | Comments addressed before human review/merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | {paste} | |
| Integration tests | `make test-integration` | {paste} | |
| Lint + guards | `make lint-zig` | {paste} | |
| Cross-compile (daemon) | `zig build -Dtarget=x86_64-linux && -Dtarget=aarch64-linux` | {paste} | |
| Cross-compile (runner) | `zig build --build-file build_runner.zig -Dtarget=x86_64-linux-musl` | {paste} | |
| Isolation guard (negative) | wire `pg` into runner → `make <isolation-target>` fails | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |

---

## Out of Scope

- Ghostty `Config.zig` options-centralization + `retarget()` — a larger parity follow-up; named here, not mud-patched.
- Merging the two graphs / any `pg`/`s3` access from the runner.
- Renaming the `build.zig` / `build_runner.zig` entry points.
- Rewriting `build_pg.zig` OpenSSL target detection (documentation is the chosen remediation; prod is native-arch TLS-on).
- Daemon version surfacing — see Discovery (pending Indy).
- The `zig-pkg/` cache and dependency pins (the `nullclaw -zmb.2` fork tag stays).
