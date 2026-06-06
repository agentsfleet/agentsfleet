# M84_007: Runner credential file-descriptor inheritance proof

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 007
**Date:** Jun 06, 2026
**Status:** DONE
**Priority:** P1 — security boundary, **launch-targeted** (not deferred). Proof-only: it pins an already-correct property (no daemon file descriptor crosses `exec` into the sandboxed child) with regression tests, so a future change cannot silently re-open a credential-inheritance escape. Ships for launch precisely because it adds no new subsystem and no new behaviour — only assertions over the existing `forkExec`/spawn path.
**Categories:** API
**Batch:** B1 — standalone; the proof rides the existing `test-integration-runner` lane.
**Branch:** feat/m84-credential-fd-proof
**Depends on:** **M84_003 (sandbox env/fd hardening)** — its code is in `main` (`child_process.forkExec`, the env-allowlist filter, and the `test-integration-runner` lane in `build_runner.zig` / `make/test-integration.mk` this proof extends). No code dependency on the deferred specs.
**Provenance:** agent-generated — this proof began as M84_006 §2, was briefly proposed as M84_004 §5, then split into its own launch-safe spec (Jun 06, 2026) because the egress proxy (M84_004 §1–§4) and cap-drop/containment (M84_006) stay **deferred behind the untrusted-runner GA trigger**, whereas this proof is launch-safe and ships now. The credential-**out** channel (egress) is M84_004; the credential-**in**-via-fd channel is this spec — the daemon's own credential never reaching the child in the first place.

> **Provenance is load-bearing.** Every claim was code-grounded against `main` in the worktree: `forkExec` calls `std.process.spawn` with `.stdin=.pipe / .stdout=.pipe / .stderr=.inherit / .pgid=0` and **no** `progress_node` (so it defaults to `std.Progress.Node.none` — `std/process.zig` SpawnOptions); `LoopbackClient` (`control_plane_client.zig`) holds only `base_url` + `io`, opening a `std.http.Client` per call and freeing it before return; `sandbox_integration_test.zig` already forks real children to prove the env-allowlist filter. Re-confirm at PLAN.

**Canonical architecture:** [`docs/architecture/runner_fleet.md`](../../architecture/runner_fleet.md) §Sandbox tiers. This spec verifies the file-descriptor surface of the existing process boundary; it changes no mechanism.

---

## Implementing agent — read these first

1. `src/runner/sandbox_integration_test.zig` — the Linux-only real-process proof file (already wired to `test-integration` in `build_runner.zig`). The `"a planted daemon token never reaches a real spawned child's environment"` test + the `spawnIo` / `readToEnd` helpers are the exact pattern to mirror for the marker-fd and stray-fd proofs.
2. `src/runner/child_process.zig` — `forkExec` → `std.process.spawn`. Confirms the spawn options that cross into the child (only the stdio pipes; no `progress_node`).
3. `src/runner/daemon/control_plane_client.zig` — `LoopbackClient`; confirms the `zrn_`-bearing control-plane socket is per-call (`std.http.Client.fetch`) and the struct holds no persistent fd field.
4. `build_runner.zig` — the `runner_integration_tests` artifact (root `sandbox_integration_test.zig`, step `test-integration`); confirms new integration tests in that file need **no** build wiring.
5. `dispatch/write_zig.md` — all `*.zig` edits.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `test(m84_007): prove no daemon credential fd crosses exec into the sandboxed child`
- **Intent (one sentence):** Pin, with regression tests, that the sandboxed child inherits **only** wired stdio (fd 0/1/2) — no daemon-held file descriptor (the `zrn_` control-plane socket, cgroup/datastore handles) crosses `exec` — so a prompt-injected agent has no inherited capability to replay calls as the runner or reach the data plane.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm the marker-fd probe + `/proc/self/fd` enumeration run on the Linux CI lane and that the unit structural assertion compiles on macOS.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`**:
  - **RULE NDC** — proof-only: the inherited-fd vector is already closed (Zig `O_CLOEXEC` defaults; only fd 0/1/2 wired; control-plane client per-call-freed; cgroup fds `defer`-closed). This spec adds **assertions + regression tests, never a production guard** — no dead/no-op code.
  - **RULE NLR** — any file touched (`sandbox_integration_test.zig`, `control_plane_client.zig`) is left at or above its current cleanliness; no opportunistic refactor.
  - **RULE UFS** — the marker-fd probe value and any literal flags/markers are single-sourced named constants in the test, reused across asserts (mirror the existing `PLANTED_TOKEN`).
  - **RULE TST-NAM** — test block names carry no `M84_007` / `§` / dimension IDs (descriptive prose only).
  - **RULE NLG** — pre-2.0: no "legacy" framing.
- **`dispatch/write_zig.md`** — tagged-union results, `errdefer`, cross-compile both linux targets (the test graph).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | **yes** — `*.zig` edits | Read `dispatch/write_zig.md`; cross-compile the runner TEST graph for both linux targets. |
| LENGTH (≤350/≤50/≤70) | **maybe** — `sandbox_integration_test.zig` grows | Keep each test ≤50 lines; reuse `spawnIo`/`readToEnd`; if the file nears 350, split the fd proofs into a sibling integration file added to the `test-integration` artifact. |
| UFS | **maybe** — marker/probe literals | Named constants in the test (mirror `PLANTED_TOKEN`). |
| PUB | **no** | No new public surface; tests reuse the already-`pub` `buildChildEnviron`/spawn path. |
| LIFECYCLE | **no** | Each test opens its own marker fd and `defer`-closes it; adds no daemon-owned resource. |
| LOGGING | **no** | No new log emit. |

---

## Overview

**Goal (testable):** A real child spawned through the same `std.process.spawn` path `forkExec` uses sees exactly fds `0/1/2` in `/proc/self/fd`; a marker fd opened by the parent daemon returns `EBADF` from `fcntl(N, F_GETFD)` in the child; and `LoopbackClient` provably holds no persistent file-descriptor field — each proven by a test that fails the build if the property regresses.

**Problem:** The classic sandbox escape is inheriting an open capability — a socket, database handle, or control pipe — across `exec`. In this runner the daemon holds the `zrn_` runner token on a `std.http.Client` socket (`control_plane_client.zig`), plus cgroup and (in other build configs) datastore handles. An inherited control-plane socket would let a prompt-injected agent replay calls to `zombied` **as the runner** (cross-tenant blast radius — tenant scoping is not yet enforced at the lease layer); an inherited datastore handle would be direct data-plane access bypassing the API. The vector is **already closed** (Zig `O_CLOEXEC` defaults; only fd 0/1/2 cross via `dup2`; the control-plane client is per-call-`deinit`'d; cgroup fds are `defer`-closed; the supervisor reaps before the next fork) — but it is **unproven**, so a future open site that forgets `CLOEXEC`, or a pooled/persistent control-plane client, could silently re-open it.

**Solution summary:** Add real-process regression tests on the existing `test-integration-runner` lane that fork a child the way `forkExec` does and assert the child inherits only wired stdio, plus a structural unit assertion that the control-plane client carries no persistent socket. No mechanism change — proof only.

---

## Prior-Art / Reference Implementations

- **`sandbox_integration_test.zig`** — the existing `"planted daemon token never reaches a real spawned child"` test forks an actual child via `std.process.spawn` and inspects its observable state; the fd proofs mirror it exactly (same `spawnIo`/`readToEnd` helpers, same `SkipZigTest`-on-non-Linux gate).
- **CLOEXEC-by-default fd-inheritance audit** — the standard `posix_spawn`/`fork`+`exec` hardening check (enumerate the child's `/proc/self/fd`, assert no unexpected inherited fd); mirrors how sandboxes verify no capability leaks across `exec`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/sandbox_integration_test.zig` | EDIT | Add the Linux-only marker-fd-not-inherited + no-stray-fd real-process proofs (mirror the planted-token test). |
| `src/runner/daemon/control_plane_client.zig` | EDIT | Add the structural unit test (+ `comptime` assertion) that `LoopbackClient` holds no persistent fd-bearing field; document the per-call-freed socket invariant. |

> No `build_runner.zig` or `make/test-integration.mk` change — `sandbox_integration_test.zig` is already the `test-integration` artifact root; the unit test rides the normal runner test graph.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** additive proof; **patch**, behaviour-preserving (the legitimate lease is unchanged). One Section, four Dimensions sharing the existing lanes.
- **Alternatives considered:** (a) **Leave it in M84_006/M84_004** — rejected: both are deferred behind untrusted-runner GA, and this proof is launch-safe and should not be gated behind work it does not need. (b) **Fold into M84_003** — rejected: M84_003 is already merged (#370) and closing; reopening it to append a section is messier than a focused standalone spec. (c) **A standalone marker-fd test only** — rejected: the `/proc/self/fd` enumeration is the catch-all that also covers a future progress-fd-3 regression, so both belong.

---

## Sections (implementation slices)

### §1 — No daemon credential file descriptor crosses `exec`

A child spawned the way `forkExec` spawns inherits only the deliberately-wired stdio. Prove it three ways (real-process marker, real-process enumeration, structural client assertion) and record the daemon open-site audit.

- **Dimension 1.1** — a marker fd opened by the parent (default flags) is not accessible in the spawned child → Test `test_marker_fd_not_inherited_by_child` (child `fcntl(N, F_GETFD)` → `EBADF`)
- **Dimension 1.2** — the child sees no unexpected open fd ≥ 3 beyond wired stdio → Test `test_no_stray_fds_in_child` (child `/proc/self/fd` lists only `0/1/2`; this subsumes any `progress_node` fd-3 path, which `forkExec` leaves at `.none`)
- **Dimension 1.3** — the control-plane client holds no persistent socket: `LoopbackClient` has only `base_url` + `io`; every `std.http.Client` is per-call and freed before any fork → Test `test_control_plane_client_holds_no_persistent_fd` (structural / `comptime`)
- **Dimension 1.4** — Discovery enumerates every daemon open site (control-plane socket, cgroup fds, lease pipes) and records each as `CLOEXEC`-by-default or `defer`-closed-before-fork → assertion table in Discovery

---

## Interfaces

> **Illustrative — exact probe mechanism verified at PLAN.** Contract, not implementation. This spec adds **no production interface**; it asserts existing behaviour.

```
# Proof contract (observed, not changed):
#   spawn(child) via the forkExec path  =>  child /proc/self/fd == { 0, 1, 2 }
#   parent opens marker fd N (default flags)  =>  child fcntl(N, F_GETFD) == EBADF
#   LoopbackClient fields ⊆ { base_url, io }  (no socket/fd-bearing field; sockets are per-call + freed)
#   forkExec spawn options: progress_node == std.Progress.Node.none  (no fd 3)
```

Contract: the legitimate sandboxed lease is observably unchanged; only the file-descriptor surface is now pinned by tests.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| A daemon open site forgets `CLOEXEC` | future code adds a non-`CLOEXEC` open before fork | `test_marker_fd_not_inherited_by_child` + `test_no_stray_fds_in_child` fail the build before merge. |
| A pooled / persistent control-plane client is introduced | a refactor adds a long-lived socket field to `LoopbackClient` | `test_control_plane_client_holds_no_persistent_fd` fails — the structural invariant breaks at compile/test time. |
| `std` default flips to set a progress fd | toolchain bump re-introduces an fd-3 progress sink | `test_no_stray_fds_in_child` catches the unexpected fd 3. |
| Test runs on macOS | no `/proc`, no real netns | Linux-gated (`SkipZigTest`); macOS proof = cross-compile the runner TEST graph for both linux targets. |

---

## Invariants

1. **Only wired stdio crosses `exec`** — a child spawned via the `forkExec` path lists exactly fds `0/1/2`; no daemon fd ≥ 3 is inherited. Enforced by `test_no_stray_fds_in_child`.
2. **Daemon-opened fds are `CLOEXEC`** — a parent marker fd is `EBADF` in the child. Enforced by `test_marker_fd_not_inherited_by_child`.
3. **The control-plane client is fd-stateless** — `LoopbackClient` holds no persistent socket/fd field; every credential-bearing socket is per-call and freed before any fork, so none is live at spawn. The structural test is a build-time tripwire: it pins the struct to exactly `{base_url, io}` and raises `@compileError` on any added field, so a future pooled/persistent-socket field cannot land unreviewed. It asserts struct *shape*, not transitive fd-ownership — a refactor that made an existing field fd-bearing would still need human review (the `@compileError` message names that obligation).

---

## Test Specification (tiered)

> **Lane:** the Linux-only proofs run on the **`test-integration-runner`** lane (`zig build --build-file build_runner.zig test-integration`, the `make test-integration-runner` target) — they fork real children and read `/proc/self/fd`, a Linux runtime. `builtin.os.tag == .linux`-gated (`SkipZigTest` on macOS). macOS dev-loop proof = cross-compile the runner TEST graph for both linux targets. The structural unit test runs everywhere (normal runner test graph).

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration-runner | `test_marker_fd_not_inherited_by_child` | parent opens marker fd N (default flags); child `fcntl(N, F_GETFD)` → `EBADF` |
| 1.2 | integration-runner | `test_no_stray_fds_in_child` | child enumerates `/proc/self/fd` → only `0`, `1`, `2` present (no fd ≥ 3) |
| 1.3 | unit | `test_control_plane_client_holds_no_persistent_fd` | `LoopbackClient` fields ⊆ `{ base_url, io }`; no socket/fd-bearing field (structural / `comptime`) |
| 1.4 | (assertion) | Discovery table | each daemon open site recorded `CLOEXEC`-by-default or `defer`-closed-before-fork |

- **Regression:** the existing runner suite (`make test-unit-zigrunner`) + the M84_003 env-filter integration proofs stay green; the legitimate sandboxed lease still spawns and runs.
- **Idempotency/replay:** N/A.

---

## Acceptance Criteria

- [x] Child inherits only wired stdio — verify: the no-stray-fd proof compiles on both linux targets; runs on CI (Linux-gated, skips on the macOS dev host)
- [x] Daemon-opened fds are not inherited — verify: the marker-fd proof compiles on both linux targets; runs on CI (Linux-gated, skips on the macOS dev host)
- [x] Control-plane client holds no persistent fd — verify: `test_control_plane_client_holds_no_persistent_fd` (passes natively)
- [x] Daemon open-site audit recorded — verify: Discovery assertion table populated
- [x] `make lint-zig` clean · `make test-unit-zigrunner` passes · cross-compile both linux targets compile-clean · `make test-integration-runner` on CI
- [x] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: the fd proofs are present and named (TST-NAM clean — prose names, no milestone IDs)
git grep -nE 'marker fd does not cross exec|inherits only wired stdio|holds no persistent file descriptor' src/runner
# E2: runner unit graph (structural assertion) — runs on macOS too
make test-unit-zigrunner 2>&1 | tail -5
# E3: integration lane (marker fd + stray-fd enumeration) — Linux
make test-integration-runner 2>&1 | tail -10
# E4: dev-loop proof — Linux-only bodies compile from macOS
zig build --build-file build_runner.zig test-integration -Dtarget=x86_64-linux 2>&1 | tail -3
zig build --build-file build_runner.zig test-integration -Dtarget=aarch64-linux 2>&1 | tail -3
# E5: gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

**1. Orphaned files — none expected.**

| File to delete | Verify |
|----------------|--------|
| N/A — additive proof, no file removed | — |

**2. Orphaned references.** N/A — no symbols removed.

---

## Discovery (consult log)

- **Origin (Jun 06, 2026):** this proof was M84_006 §2, briefly proposed as M84_004 §5, then split into this launch-safe spec. Indy decision (verbatim, Jun 06, 2026): _"do"_ — context: the recommendation to defer the egress proxy (M84_004 §1–§4) and cap-drop/containment (M84_006) behind untrusted-runner GA, while landing the proof-only credential-fd belt now. Both deferred specs keep their `Out of Scope` cross-reference pointing here.
- **Code-grounded facts (worktree `main`):** `forkExec` → `std.process.spawn` with `.stdin/.stdout=.pipe`, `.stderr=.inherit`, `.pgid=0`, no `progress_node` (defaults `.none`); `LoopbackClient` fields are only `base_url` + `io`, one `std.http.Client` per verb, response freed before return; `sandbox_integration_test.zig` already forks real children (`spawnIo`, `readToEnd`, `SkipZigTest` on non-Linux) and is the `test-integration` artifact root, so the new integration tests need no build wiring.
- **Daemon open-site audit (Dimension 1.4 — done):**
  - control-plane `std.http.Client` socket (`control_plane_client.zig` `post`/`get`) → opened per verb, `client.deinit()` before return; the struct holds no fd field (pinned by the structural test).
  - cgroup scope fds (`engine/cgroup.zig`) → `defer`-closed within their scope; not held across `forkExec`.
  - lease stdio pipes (`child_process.forkExec` → `std.process.spawn`) → `dup2` to child `0/1`; parent ends are `pipe2(CLOEXEC)` (std), so they do not cross into the child.
  - `SpawnOptions.progress_node` → left default `std.Progress.Node.none` by `forkExec`, so std opens no progress fd 3 (covered empirically by the no-stray-fd proof).
- **Implementation notes (Jun 06, 2026):** the integration proofs slot into the existing `sandbox_integration_test.zig` (mirroring the planted-token test) — no `build_runner.zig`/`make` change. `/proc/self/fd/*` are symlinks whose pipe/socket targets are not stat-able, so the child uses `[ -L ]` (symlink test, opens no fd) not `[ -e ]`. The marker fd is `/dev/null` via `std.Io.Dir.openFileAbsolute` (CLOEXEC by default). The structural assertion is comptime-folded (`const known = comptime (...)`) so an added field is a `@compileError`, not a live runtime branch.
- **OWASP agent-trust-boundary coverage (Jun 06, 2026):** the sandboxed child is the prompt-injected-agent threat actor. The "deny it an inherited credential capability" boundary (OWASP LLM Excessive Agency · secrets-never-reach-the-agent · tool-permissions-scoped-per-invocation · fail-closed) is tested across both inbound channels: **env** — the planted-token test (M84_003, same file) proves the `zrn_` control-plane secret and its `ZOMBIE_RUNNER_TOKEN` key never appear in the child's `/proc/self/environ`; **fd** — this spec's three proofs (marker not inherited, no stray fd, client holds no persistent socket). The remaining channels are out of scope here and tracked elsewhere: **filesystem** secret-file isolation = M84_006 Landlock (deferred); **egress** exfiltration = M84_004 (deferred), both behind untrusted-runner GA.
- **Review outcomes (Jun 06, 2026):**
  - `/write-unit-test` — clean; the three proofs map 1:1 to the Test Specification + the §1.4 audit table. The marker test carries a positive control (`ok` proves the probe sees a live fd), so it is not vacuous.
  - `/review` (adversarial, fresh-context) — two findings dispositioned: (a) the no-stray-fd test scanned a hardcoded `3..20` window → **fixed**, widened to a `3..63` `[ -L ]` loop (directory enumeration is avoided because `opendir()` on `/proc/self/fd` adds a transient entry); (b) both integration tests spawn via `std.process.spawn` (the syscall path `forkExec` uses) rather than calling `forkExec` directly → **accepted as a deliberate limitation**, consistent with the existing planted-token env test in this file: `forkExec` needs bwrap/sandbox scaffolding that the test lane cannot stand up, and the property under test (CLOEXEC + stdio wiring at the syscall layer) is faithfully exercised. The structural test's claim was softened from "proof of fd-statelessness" to "build-time tripwire on struct shape" (Invariant 3).
- **Deferrals** — none. Any "deferred to follow-up" needs an Indy-acked verbatim quote here.
- **Skill chain outcomes** — `/write-unit-test`: clean. `/review`: 2 findings, both dispositioned (1 fixed, 1 accepted limitation). `/review-pr` + `kishore-babysit-prs`: run post-PR.

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits the proof vs this Test Specification. | Clean. Iteration count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs Invariants, Failure Modes, `dispatch/write_zig.md`. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR against the immutable diff. | Comments addressed before human review/merge. |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Runner unit (incl. structural fd test) | `make test-unit-zigrunner` | Unit tests passed (`zig build test` 235 pass / 6 skip) | ✅ |
| Lint | `make lint-zig` | ZLint 0 errors / 0 warnings across 397 files; format + 350-line + all gates green | ✅ |
| Cross-compile (TEST graph) | `zig build --build-file build_runner.zig test-integration -Dtarget=x86_64-linux && aarch64-linux` | both targets compile clean (7/9 steps; binaries emitted). The run step is execution-barred on the macOS host — the Linux test binary cannot run here ("host system is unable to execute binaries from the target"); it executes on the CI Linux lane | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found (2426 commits scanned) | ✅ |
| Runner integration (marker + stray fd) | `make test-integration-runner` | Linux-only — runs on CI (`SkipZigTest` on macOS dev host) | ⏳ CI |

---

## Out of Scope

- **M84_004 egress allowlist** (own-netns + DNS-pinning proxy) — the credential-**out** channel; deferred behind untrusted-runner GA. This spec is the credential-**in**-via-fd channel.
- **M84_006 cap-drop + containment verification** — deferred behind untrusted-runner GA.
- **The env-allowlist filter** — already shipped + tested in M84_003 (`buildChildEnviron`); this spec proves the *fd* surface, not the *env* surface.
- **Enforcing tenant scoping at the lease layer** — the reason an inherited control-plane socket would be cross-tenant; that is a separate fleet-scope workstream, named here as the amplifying factor, not fixed.
