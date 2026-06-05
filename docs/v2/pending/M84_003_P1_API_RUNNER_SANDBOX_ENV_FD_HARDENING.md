# M84_003: Harden the runner sandbox — clear env, drop caps, no-new-privs, new-session, CLOEXEC proof, absolute argv[0], kill-domain

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 003
**Date:** Jun 03, 2026 (amended Jun 05, 2026 after plan-eng-review)
**Status:** PENDING
**Priority:** P1 — security boundary. Closes a daemon-credential / file-descriptor exfiltration path open to an untrusted sandboxed agent, and pins the containment kill domain. No customer-facing behaviour change; gates untrusted/local-runner General Availability (GA).
**Categories:** API
**Batch:** B1 — runs **in parallel** with M84_005 (memory); disjoint trees (`src/runner/` here vs `src/zombied/`+contract there), two shared touchpoints to coordinate (`build_runner.zig`, `make/test-integration.mk` — second to land rebases).
**Branch:** {feat/m84-runner-sandbox-hardening — added at CHORE(open)}
**Renumber note (Jun 05, 2026):** filed originally as `M84_001` — that ID is already owned by the shipped `M84_001_..._DASHBOARD_RUNNER_ENROLLMENT` (`docs/v2/done/`, PR #365), and `M84_002` by the pending fleet-operator-plane spec. Renumbered to **M84_003** at plan-eng-review to remove the collision.
**Depends on:** **M82_001 — DONE (merged #366); `std.process.spawn` is on `main` today.** No sequencing wait and no rebase risk: this workstream edits the post-0.16 `forkExec` / `appendBwrap` that already exist. (The original "sequence after M82" framing is obsolete now that M82 has landed.)
**Provenance:** agent-surfaced during the M82_001 `forkExec → process.spawn` Chief Technology Officer (CTO) / threat-model review (Jun 03, 2026), corroborated by an independent CTO review (ChatGPT), then **scope-reviewed by `plan-eng-review` (Jun 05, 2026)** which code-grounded every claim with a 36-agent adversarial workflow. All process-boundary findings are **pre-existing** (the manual fork-exec path had them too) and confirmed by code reading, **not** introduced by M82 — M82 is behaviour-preserving and explicitly does not touch them (M82 Discovery, "Sandbox-hardening" entry, Indy-acked).

> **Provenance is load-bearing.** Findings come from reading `sandbox_args.zig` / `child_process.zig` / `child_exec.zig` / `child_supervisor.zig` under an adversarial lens, not from a vulnerability report. The exact env/fd/cap surface was re-confirmed at plan-eng-review against the then-current source; re-confirm again at PLAN.

**Canonical architecture:** the host-resident `zombie-runner` execution plane (`docs/architecture/` runner-fleet docs). The sandbox model is bwrap (namespaces) + Landlock (in-child) + cgroup (kill domain); this workstream hardens the **process-boundary env / fd / capability / process-group surface** that sits underneath those, not the namespace/LSM layers.

---

## Launch re-cut (CEO review + Indy, Jun 05, 2026)

> **Strategic decision (Indy):** **untrusted / customer-operated runner GA is a post-launch expansion, NOT the v2-launch commitment.** v2 launches with **platform-operated (trusted) runners** on usezombie's own baremetal. That re-prioritizes this milestone: most of it is the unlock for a *later* tier, not a launch blocker. Launch is days away; security depth iterates after.

This spec ships in **two slices**:

- **Launch Slice (ship now — this PR):** **§1 env isolation via filtered `environ_map`** (the live credential leak) + **§1.5 `no_new_privs`** (cheap, additive — defangs setuid; does not touch Landlock) + **§1.6 `--new-session`** (one harmless bwrap flag) + **§3 `argv[0]` absolute guard** (one-line) + **§4 kill-domain** (Fix A + Fix B — a real correctness bug: a forking agent survives revocation). Small, high-value, low-risk.
- **Deferred Slice — RE-HOMED to [`M84_006`](./M84_006_P2_API_RUNNER_SANDBOX_DEPTH.md) (P2, behind untrusted-runner GA):** `--cap-drop ALL`, the fd CLOEXEC proof, and the network/fs containment verification now live in **M84_006** (which depends on this spec's `no_new_privs`). **Do not implement those from this file** — the old §1.4 dimension is removed and §2/§5 below are stubs pointing to M84_006. M84_003 is now physically just the launch slice.

> **Parallelism:** M84_003 (this spec) and **[`M84_005`](./M84_005_P1_API_RUNNER_MEMORY_CAPTURE.md)** (memory) can be implemented **in parallel** (separate worktrees). They touch largely disjoint trees — 003 is `src/runner/` process-boundary; 005 is `src/zombied/` + `src/lib/contract` + runner memory wiring. **Two shared touchpoints to coordinate:** `build_runner.zig` (003 adds the `test-integration-runner` step; 005 changes `.engines`) and `make/test-integration.mk` (003 creates the runner lane; 005 may register on it) — whoever lands second rebases.

> **Compensating control for the deferred egress/exfil risk (Indy):** the default LLM provider keys are **$-capped** — a rogue/prompt-injected agent that exfiltrates the provider key cannot spend beyond the cap, and usezombie absorbs that cut. This bounds the **LLM-spend** exfil vector for launch, which is what buys the right to defer the egress proxy ([`M84_004`](./M84_004_P1_API_RUNNER_EGRESS_ALLOWLIST.md)). **CTO caveat:** the $-cap does **not** bound a stolen **tool secret** (e.g. a GitHub Personal Access Token) — that has non-$ blast radius — so pair the cap with **least-privilege / short-lived tool secrets** so both vectors are bounded at launch.

---

## Implementing agent — read these first

1. `docs/AUTH.md` — `ZOMBIE_RUNNER_TOKEN` is the daemon's control-plane credential; §1 exists because it can currently leak into the sandbox. Auth-boundary file — `/review` the env-allowlist change specifically.
2. `src/runner/sandbox_args.zig` — `appendBwrap` (the bwrap argv builder) and `bwrapPath`; §1 (cap-drop + new-session) + §3 land here. The env isolation (§1.1/1.2) is `environ_map` in `child_process.zig`, not a bwrap flag.
3. `src/runner/child_process.zig` — `forkExec` (`std.process.spawn`), `killChild` (the `kill(-pgid)` fallback); §3 (argv[0]) + §4 (kill-tree both-fix) land here.
4. `src/runner/child_supervisor.zig` — `supervise` (the `addProcess` enrollment), `establishSandbox`; §4 (fail-closed enrollment) lands here.
5. `src/runner/child_exec.zig` — the in-child `__execute` entry; the `no-new-privs` prctl (§1, Dim 1.5) lands here next to `landlock.applyPolicy`. Documents the "secrets ride stdin, never argv/env" contract this workstream extends to the daemon's own token.
6. `src/runner/engine/landlock.zig` — context for Dim 1.5: `applyPolicy` calls `landlock_restrict_self` **without** setting `no_new_privs` (succeeds via the userns `CAP_SYS_ADMIN` path), so `no-new-privs` is *not* guaranteed by our code today.
7. `dispatch/write_zig.md` — all `*.zig` edits.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `build(m84): harden runner sandbox — environ_map filter, cap-drop, no-new-privs, new-session, CLOEXEC proof, absolute argv[0], kill-domain`
- **Intent (one sentence):** Ensure an untrusted sandboxed agent cannot read the daemon's environment (incl. `ZOMBIE_RUNNER_TOKEN`), holds no Linux capabilities and cannot gain privilege via setuid, cannot inherit a non-`CLOEXEC` daemon file descriptor or a controlling terminal, cannot influence `argv[0]` resolution — and that the containment kill domain can never be silently empty.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. **Re-confirm the env passthrough allowlist (§1, Dim 1.2) against a *verified enumeration* of every in-child env read** (not the illustrative 5) — a too-tight allowlist breaks tool execution; a too-loose one re-opens the leak.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`**:
  - **RULE UFS** — the env passthrough allowlist (var names), the daemon deny-prefix, and the bwrap flag literals (`--cap-drop`, `--new-session`) are single-sourced named constants, referenced from the builder/`forkExec` and their tests — never re-spelled. (Env isolation is `environ_map`, so there are no `--clearenv`/`--setenv` literals.)
  - **RULE NLG** — pre-2.0: no "legacy"/"compat" framing for the hardened path; the unhardened behaviour is simply replaced, not shimmed.
  - **RULE NDC / NLR** — no dead code; **§2 is proof-only** (Zig opens are `CLOEXEC` by default and no daemon fd ≥ 3 is open at spawn — see §2), so §2 adds *assertions + regression tests*, never a no-op production "guard".
- **`dispatch/write_zig.md`** — all `*.zig` edits (tagged-union results, `errdefer`, cross-compile both linux targets).
- **`docs/AUTH.md`** — `ZOMBIE_RUNNER_TOKEN` handling; the env-allowlist change is auth-boundary and must not alter how the daemon *itself* reads or sends the token, only what the child inherits.
- **`docs/LOGGING_STANDARD.md`** — any new "env_cleared"/"caps_dropped"/"fd_audit" emit follows the logfmt envelope; never log the token or a full environ dump.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | **yes** — `*.zig` edits | Read `dispatch/write_zig.md`; cross-compile both linux targets before commit. |
| UFS | **yes** — allowlist + deny-prefix + bwrap flags | Named constants in `sandbox_args.zig`, reused in tests (`// pin test` only where the literal IS the bwrap contract). |
| LENGTH (≤350/≤50/≤70) | **yes** — three hotspots | (1) `appendBwrap` grows (env emit + cap-drop + new-session): extract an env/cap emit helper so it stays ≤50 lines (the `environ_map` mechanism in Alt (a) avoids the `--setenv` loop entirely and lessens this). (2) **`child_supervisor.zig` is 326/350 today** — Fix A adds a branch + kill + error path and is the most likely 350-line **file-cap** trip: extract an `enrollOrFail` helper (or sibling-file move). (3) **`supervise` itself is ~73 lines (`child_supervisor.zig:117-189`)** — already near the **function** cap (≤50 soft / ≤70 hard) before Fix A lands *inside* it; the `enrollOrFail` extraction must pull the new branch out of `supervise`, not just out of the file. Confirm `supervise`'s measured length at PLAN. All three confirmed by Orly CTO review. |
| LOGGING | **maybe** — new audit emit | Envelope unchanged; never log secret values. |
| LIFECYCLE | **no** | No new heap/handle ownership; fd hygiene tightens flags, adds no resource. |
| SCHEMA / ERROR REGISTRY / UI / DESIGN TOKEN | **maybe ERROR REGISTRY** — §4 fail-closed reuses `ERR_RUN_SANDBOX_ESTABLISH_FAILED`; if a distinct enrollment-failure code is added, register it. No schema, no UI. |

---

## Overview

**Goal (testable):** A token planted in the daemon's environment never appears in the sandboxed child's environment or argv; the child holds **no** Linux capabilities and **`NoNewPrivs:1`**; it has no controlling terminal; **no** non-`CLOEXEC` daemon file descriptor is inherited; `forkExec` rejects a non-absolute `argv[0]` before `spawn`; and a forking agent's whole tree is reaped on revocation/timeout **even if cgroup enrollment failed** — each proven by a negative/regression test.

**Problem:** The host runner spawns an untrusted, prompt-injectable agent inside bwrap + Landlock + cgroup. Process-boundary leaks sit *underneath* that isolation:

1. **Env (§1, live P0).** `forkExec` (`child_process.zig:32`) passes no `environ_map` to `std.process.spawn` — so the child inherits the daemon environ, exposing `ZOMBIE_RUNNER_TOKEN` (the control-plane credential). *On sandboxed tiers the cross-process `/proc/<daemon>/environ` read is already shut by `--unshare-all` (pid namespace) + `--proc /proc`; the residual leak is the child's **own** environ (`cat /proc/self/environ`), which a filtered `environ_map` closes.*
2. **Capabilities / privilege (§1, defense-in-depth).** No explicit `--cap-drop ALL`; the child relies entirely on the `--unshare-all` user namespace to neuter caps namespace-locally. No `--new-session` (controlling-terminal / TIOCSTI surface). `no_new_privs` is not set by our code (Landlock succeeds via userns `CAP_SYS_ADMIN`, not NNP), so setuid binaries RO-bound into `/usr`,`/bin`,`/sbin` are a contingent risk.
3. **FD (§2, already mitigated — proof only).** `std.process.spawn` makes pipes `pipe2(CLOEXEC)`, only fd 0/1/2 cross via `dup2`, and the daemon holds no socket/db/cgroup fd open at spawn (per-call `http.Client` deinit; sequential reap-before-next-fork). No live patch — assertions + regression sweep.
4. **argv[0] (§3).** `std.process.spawn` resolves a relative `argv[0]` via the **parent** `PATH`; `buildArgv` already produces absolute paths — make the invariant fail-closed.
5. **Kill domain (§4, live containment bug).** `addProcess` enrollment failure is non-fatal (`child_supervisor.zig:159`); the child then runs in the *daemon's* cgroup (bypassing `memory.max`/`cpu.max`), and `scope.kill()` writes `cgroup.kill=1` to the **empty** exec-cgroup — which **succeeds** — so the `kill(-pid)` fallback (only inside the cgroup-write `catch`) never fires and a forking child's tree survives.

None is introduced by M82 (behaviour-preserving) — all pre-exist.

**Solution summary:** Pass the child only a **filtered `environ_map`** (the verified allowlist) at the `std.process.spawn` boundary in `forkExec` — the daemon environ never reaches the child; drop all capabilities (`--cap-drop ALL`), set `no_new_privs` in-child, and detach the controlling terminal (`--new-session`); assert (don't patch) that every daemon fd is `CLOEXEC`; assert `argv[0]` absolute (fail-closed) before `spawn`; and make the kill domain un-emptyable (fail-closed on enrollment failure **and** always also signal the process group). Behaviour for the legitimate sandboxed lease is unchanged — only the leak/escape surface is removed.

**Prioritization (CTO review + plan-eng-review, Jun 2026).** Ranked for *this* codebase: **(1)** environment sanitisation (§1, the only live credential leak), **(2)** kill-domain un-emptying (§4, live containment bug), **(3)** cap-drop / no-new-privs / new-session (§1, cheap defense-in-depth that lands in the same `appendBwrap`/`child_exec` edit), **(4)** argv[0] guard (§3), **(5)** fd CLOEXEC proof (§2 — the *cheapest* finding, not the strongest: the vector is already closed). §5 verifies the *existing* network/fs containment as M84 acceptance (mechanism owned elsewhere).

---

## Prior-Art / Reference Implementations

- **Filtered `environ_map` at `spawn`** is the canonical "child inherits only what you pass" pattern (`posix_spawn`/`execve` with an explicit envp); **bwrap `--cap-drop ALL` + `--new-session`** is the canonical hardened-sandbox flag pattern (Flatpak, `bubblewrap(1)`); the existing `appendBwrap` flag-emit style (`dup` each arg) is the in-repo pattern to extend for the two flags.
- **CLOEXEC-by-default** — Zig `std.Io`/`std.fs` file opens set `O_CLOEXEC`; the workstream's job is to *prove* the daemon holds no non-CLOEXEC fd at spawn time, mirroring how `posix_spawn` users audit fd inheritance.
- **`PR_SET_NO_NEW_PRIVS`** — set in-child via `prctl` adjacent to `landlock.applyPolicy`; the kernel mandates it for any later privilege-restricting step and it permanently defangs setuid.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/sandbox_args.zig` | EDIT | `appendBwrap`: emit `--new-session` (§1.6); host the allowlist + deny-prefix constants. **No `--clearenv`/`--setenv`** — env isolation is `environ_map` (below). (`--cap-drop ALL` re-homed to M84_006.) |
| `src/runner/child_process.zig` (env) | EDIT | `forkExec`: build a filtered `Environ.Map` (the allowlist) and pass it to `std.process.spawn` via `environ_map` (§1.1/1.2). |
| `src/runner/child_exec.zig` | EDIT | Set `PR_SET_NO_NEW_PRIVS` in the sandboxed fail-closed block, adjacent to `landlock.applyPolicy` (§1.5). |
| `src/runner/child_process.zig` | EDIT | `forkExec`: assert `argv[0]` absolute before `spawn` (§3); `killChild`: **always also** `kill(-pgid)` even on cgroup-kill success (§4). |
| `src/runner/child_supervisor.zig` | EDIT | `supervise`: **fail-closed** when `addProcess` enrollment fails — refuse the lease (kill the just-forked child, return a sandbox-establish failure) instead of warn-and-continue (§4). |
| `src/runner/sandbox_args_edge_test.zig` (+ a runner integration aggregator `*_test.zig`) | EDIT (exists) / CREATE (aggregator) | `sandbox_args_edge_test.zig` already exists — EDIT it for unit golden-argv tests; CREATE the **Linux-only integration** aggregator (planted-token, NoNewPrivs=1, no-tty, relative argv[0], kill-tree, enrollment-fail kill) registered on the new runner integration step. (caps=0, marker/stray fd, network/fs containment → M84_006.) |
| `build_runner.zig` | EDIT | Add a `test-integration` step (separate from the `test` unit step), rooted at the runner integration aggregator. |
| `make/test-integration.mk` | EDIT | Add the `test-integration-runner` lane (drives `zig build --build-file build_runner.zig test-integration`); distinct from the app `test-integration` (Docker/Postgres/Redis). |
| `docs/AUTH.md` | EDIT (small) | Note the runner token is `environ_map`-isolated (filtered allowlist) from the sandboxed child. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one atomic workstream (B1); Sections share the spawn path but have separate tests. It is a **patch** (additive hardening, behaviour-preserving for the legitimate path) — no new abstraction.
- **Mechanism (LOCKED — Indy, Jun 05, 2026): filtered `environ_map`.** Env isolation is done by building a **filtered `Environ.Map` (the verified allowlist) and passing it to `std.process.spawn` via `SpawnOptions.environ_map`** in `forkExec` — *not* bwrap `--clearenv`/`--setenv`. `SpawnOptions.environ_map` is confirmed present (`std/process.zig:368`, `?*const Environ.Map = null`). Rationale: "send the child only what it must have." It is tier-uniform (covers `dev_none`, macOS, any future direct-exec — it does not depend on bwrap), a single code path, and removes the `--setenv` value-plumbing and its UFS flag surface. `--cap-drop ALL` and `--new-session` remain bwrap flags (they have no `SpawnOptions` equivalent). Note `environ_map` and `--clearenv` do **not** stack (clearenv would wipe the map), so `--clearenv`/`--setenv` are **not** used.
- **Alternatives considered (rejected):** (a) **bwrap `--clearenv` + `--setenv <allowlist>`** — bwrap-only (no effect on a no-bwrap/direct-exec path), more code (read each daemon env value, emit a `--setenv` pair), larger `appendBwrap` + UFS flag surface; the prior rationale that it "covers a future direct-exec path" was **inverted** (it's the bwrap layer; `environ_map` is the tier-independent one). (b) **Drop env entirely** — rejected: the in-child engine reads `NULLCLAW_OBSERVER` and tools need `PATH`/`HOME`; a verified allowlist is required, not a blanket drop. (c) **Rely on bwrap's default `no_new_privs`** — rejected as contingent (version-dependent); set it explicitly in-child (Dim 1.5).

---

## Sections (implementation slices)

### §1 — Environment isolation + capability/privilege hardening for the sandboxed child

`forkExec` passes no `environ_map` today, so the child inherits the daemon environment and a prompt-injected agent can read `ZOMBIE_RUNNER_TOKEN` from its own environ and exfiltrate the daemon's control-plane credential. **Mechanism (LOCKED): build a filtered `Environ.Map` (the verified allowlist) and pass it to `std.process.spawn` via `SpawnOptions.environ_map` in `forkExec`** — "send the child only what it must have." This is **fail-closed** (the child gets exactly the allowlist, nothing else) and **tier-uniform** (independent of bwrap). While in the same `forkExec`/`appendBwrap`/`child_exec` edit, drop all capabilities, set `no_new_privs`, and detach the controlling terminal — cheap, byte-adjacent defense-in-depth ranked above the fd proof.

> **Closure model (keep in the spec so a future maintainer does not misread it).** A filtered `environ_map` shuts the child's **own**-environ read (`cat /proc/self/environ` shows only the allowlist). The **cross-process** read (`cat /proc/<daemon>/environ`) is *already* shut on sandboxed tiers by `--unshare-all` (pid namespace) + `--proc /proc` — the daemon is not in the child's pid namespace. Do not remove the namespace thinking the env filter covers the cross-proc case; it does not.

- **Dimension 1.1** — `forkExec` passes a filtered `environ_map` (the allowlist) to `std.process.spawn`; the child's environ contains only allowlisted vars → Test `test_child_env_filtered_to_allowlist`
- **Dimension 1.2** — the filtered map contains only the **verified allowlist**; the daemon deny-prefix (`ZOMBIE_*`) is never in the child's environ regardless of allowlist contents → Test `test_child_env_omits_daemon_secrets`. **The allowlist is derived at PLAN from a verified enumeration of every in-child env read** (engine config **and** tool subprocesses), not the illustrative list. *Known reads (Jun 05 sweep):* `HOME` (**load-bearing** — NullClaw config-dir → `error.HomeDirNotFound`); `NULLCLAW_OBSERVER` (`runner_observer.zig:26`, optional — safe default); the `NULLCLAW_*` config set (`config.zig applyEnvOverrides`, optional-with-defaults; provider/model come via the lease `agent_config` (`child_exec.zig:155-174`), not env). **No engine read of `PATH`/`TMPDIR`** was found, but tool subprocesses (`git`/`gh`) need `PATH` and Transport Layer Security needs the Certificate Authority bundle (RO `/etc` bind covers `/etc/ssl/certs`; some distros use `SSL_CERT_FILE`) — PLAN must confirm the tool-subprocess env, not just engine config reads. No in-child code reads `ZOMBIE_*` (deny-prefix is safe).
- **Dimension 1.3** — a token planted in the daemon environment does not reach the child's environment, proven via **both** the child's `/proc/self/environ` **and** a `getenv(ZOMBIE_RUNNER_TOKEN)` call inside `__execute` (the agent's real read path) → Test `test_planted_token_absent_from_child_env`
- *(cap-drop ALL re-homed to [`M84_006`](./M84_006_P2_API_RUNNER_SANDBOX_DEPTH.md) §1 — depends on Dim 1.5 below shipping first.)*
- **Dimension 1.5 (NEW)** — `child_exec` sets `PR_SET_NO_NEW_PRIVS` (prctl 38) in the sandboxed fail-closed block **before** `landlock.applyPolicy`. It is **additive and safe to ship alone** (it does not remove the userns `CAP_SYS_ADMIN` Landlock currently uses, so Landlock keeps working); it permanently defangs setuid binaries in the RO mounts AND pre-establishes `landlock_restrict_self`'s precondition for when M84_006's `--cap-drop ALL` later removes `CAP_SYS_ADMIN`. → Test `test_child_no_new_privs` (child reads `/proc/self/status` → `NoNewPrivs: 1`). *NNP also disables any in-sandbox tool that needs a setuid helper (`ping`/`sudo`) — acceptable by design; PLAN enumerates whether any engine-invoked tool relies on one.*
- **Dimension 1.6 (NEW)** — `appendBwrap` emits `--new-session`, detaching the controlling terminal (closes the TIOCSTI terminal-input-injection vector if a tty is ever attached) → Test `test_bwrap_argv_new_session` (golden argv) + `test_child_no_controlling_tty` (child run from a pty-allocated parent cannot `ioctl(TIOCSTI)` into the parent's terminal)
- **Dimension 1.7 (tier coverage, F6)** — the `environ_map` filter is applied in `forkExec` for **every** sandboxed tier (it is not bwrap-gated, so it cannot be skipped per-tier); the `--new-session` bwrap flag must likewise be emitted on every sandboxed tier including `registry_allowlist`. → asserted under both `deny_all` and `registry_allowlist` (env filter via the child-env test; flag via golden argv)

### §2 — File-descriptor hygiene (proof only) — RE-HOMED to M84_006

> Moved to [`M84_006`](./M84_006_P2_API_RUNNER_SANDBOX_DEPTH.md) §2. The fd CLOEXEC proof asserts a non-bug (already closed by Zig defaults) and is not launch-blocking. **Do not implement from this file.**

### §3 — `argv[0]` absolute guard

`std.process.spawn` resolves a relative `argv[0]` against the **parent** `PATH`; an absolute `argv[0]` is the invariant that closes any PATH-influence vector (a future refactor changing `"/usr/bin/bwrap"` to `"bwrap"` must not silently create a `PATH` trust dependency). `buildArgv` already produces absolute paths (bwrap path / `executablePathAlloc`); make the invariant explicit and fail-closed.

- **Dimension 3.1** — `forkExec` rejects a non-absolute `argv[0]` before `spawn` (fail-closed, reusing the sandbox-setup failure class) → Test `test_relative_argv0_rejected`

### §4 — Containment kill-tree: un-emptyable kill domain (both fixes)

The cgroup is the primary, atomic kill domain; `kill(-pgid, SIGKILL)` is the fallback when cgroup v2 is unavailable or `cgroup.kill` fails, and is the *only* kill on `dev_none`/macOS. The live bug: `addProcess` enrollment failure is non-fatal (`child_supervisor.zig:159`), so the child runs in the *daemon's* cgroup (bypassing `memory.max`/`cpu.max`); `scope.kill()` then writes `cgroup.kill=1` to the **empty** exec-cgroup, which **succeeds**, so the `kill(-pid)` fallback (only inside the cgroup-write `catch`) never fires and a forking child's tree survives.

**Decision (Indy, Jun 05): implement BOTH fixes.**

```
addProcess(child) FAILS → FAIL CLOSED: kill the just-forked child + refuse the
   lease (return sandbox-establish failure). Closes the bug AND the memory.max /
   cpu.max bypass; matches Invariant 7's fail-closed posture.            (Fix A)

killChild() → ALWAYS also kill(-pgid, SIGKILL), regardless of cgroup.kill success
   (remove the early `return` after a successful cgroup.kill). Belt-and-suspenders
   for the enroll-succeeds-then-cgroup.kill-races path.                  (Fix B)
```

> **Fix A DEPENDS on Fix B (Orly CTO adverse review, Jun 05).** Fix A's "kill the just-forked child" must **not** route through `scope.kill()`: on enrollment failure `scope` is non-null but **empty**, so `scope.kill()` writes `cgroup.kill=1` to the empty cgroup, **succeeds**, early-returns, and the child is **not killed** — the very §4 bug. So Fix A must kill via the *Fix-B-corrected* `killChild` (which always also `kill(-pgid)`) **or** signal the process group directly (`std.posix.kill(-child.id, SIGKILL)`) / null the scope before calling. Land Fix B with (or before) Fix A; an implementer who wires Fix A through today's `killChild` ships a fail-closed path that does not close.

> **Reconciliation with §1.6 `--new-session`.** `killChild`'s `kill(-pid)` targets **bwrap's** pid — the process-group leader (`.pgid = 0` → `setpgid(0,0)`). `--new-session` makes the bwrap child a *session* leader too, which reinforces (does not break) the group-kill: bwrap stays the group leader the daemon signals, and the agent's own `setsid()` cannot move bwrap out of that group.
> **Fix B pid-reuse safety.** The unconditional pgroup signal is safe **because `killChild` runs before `reaped = true; child.wait()`** — the killed child stays an un-reaped zombie (pid/pgid not reusable) until the single `wait()`. A future refactor must not move `wait()` earlier, or it reintroduces a pgid-reuse window. A post-`cgroup.kill` `kill(-pid)` may return `ESRCH` (group already dead) — harmless, already logged.

- **Dimension 4.1** — `kill(-child_pid, SIGKILL)` reaps a child that forked a grandchild and great-grandchild, **including an adversarial child that calls `setsid()`/`setpgid()` before forking**, with no escapee left running → Test `test_pgroup_kill_reaps_descendant_tree`
- **Dimension 4.2** — cgroup enrollment failure cannot silently disable the kill domain: inject an `addProcess` failure → the lease is **refused fail-closed** (Fix A) and, on the success-then-race path, `killChild` **always** also signals the process group (Fix B). The kill domain is never silently empty. → Test `test_kill_survives_cgroup_enrollment_failure`

### §5 — Sandbox containment verification — RE-HOMED to M84_006

> Moved to [`M84_006`](./M84_006_P2_API_RUNNER_SANDBOX_DEPTH.md) §3 (network egress + filesystem isolation characterization, incl. the `registry_allowlist` gap pin that flips when [`M84_004`](./M84_004_P1_API_RUNNER_EGRESS_ALLOWLIST.md) lands). Proof-only, not launch-blocking. **Do not implement from this file.** The launch-relevant note survives in §1.7 / the launch-re-cut banner: under `registry_allowlist` the tenant's own secrets remain exfiltratable until M84_004 — covered at launch by the $-capped provider keys + least-privilege tool secrets.

---

## Interfaces

> **Illustrative — exact signatures/field-names verified at PLAN against the Zig 0.16 stdlib** (Orly CTO review, Jun 05: the `prctl` enum coercion and the `SpawnOptions.progress_node` field name are PLAN-verify items).

```
# src/runner/child_process.zig — filtered environ_map at the spawn boundary (LOCKED mechanism)
// Build an Environ.Map containing ONLY the allowlist, pass it to spawn:
//   var env = Environ.Map.init(alloc); defer env.deinit();
//   for (ENV_PASSTHROUGH_ALLOWLIST) |name| if (daemon_env.get(name)) |v| try env.put(name, v);
//   _ = try std.process.spawn(io, .{ .argv = argv, .environ_map = &env, ... });
// SpawnOptions.environ_map: ?*const Environ.Map = null  (std/process.zig:368 — confirmed)

# src/runner/sandbox_args.zig — single-sourced constants (RULE UFS); env is NOT done via bwrap flags
const CAP_DROP_FLAG    = "--cap-drop";
const CAP_DROP_ALL     = "ALL";
const NEW_SESSION_FLAG = "--new-session";
const ENV_DENY_PREFIX  = "ZOMBIE_"; // asserted absent from the child's environ regardless of allowlist
// ENV_PASSTHROUGH_ALLOWLIST is DERIVED AT PLAN from a verified in-child env-read enumeration
// (engine config AND tool subprocesses). Illustrative only (do NOT ship as-is without the audit):
//   { "HOME", "NULLCLAW_OBSERVER", "PATH", "SSL_CERT_FILE", "LANG" }

# src/runner/child_exec.zig — in-child, sandboxed fail-closed block, BEFORE landlock.applyPolicy
// PR_SET_NO_NEW_PRIVS = 38; structural guarantee (Landlock does not set it for us).
// NOTE: prctl's first arg is i32, NOT the PR enum, in Zig 0.16 → coerce explicitly.
if (std.os.linux.prctl(@intFromEnum(std.os.linux.PR.SET_NO_NEW_PRIVS), 1, 0, 0, 0) != 0)
    return SANDBOX_FAIL_EXIT;
// ORDER IS LOAD-BEARING: NNP must be set BEFORE landlock_restrict_self. Once Dim 1.4
// (--cap-drop ALL) removes the userns CAP_SYS_ADMIN that restrict_self relies on today,
// no_new_privs becomes its ONLY remaining precondition — 1.4 and 1.5 must land together.

# src/runner/child_process.zig — fail-closed guard before spawn
if (!std.fs.path.isAbsolute(argv[0])) return error.SandboxArgvNotAbsolute;

# src/runner/child_process.zig — killChild ALWAYS also signals the group (Fix B)
// cgroup.kill is primary; the pgroup signal is unconditional belt-and-suspenders.

# src/runner/child_supervisor.zig — fail-closed enrollment (Fix A)
// addProcess failure → kill child + return failed(.startup_posture); never warn-and-continue.
```

Contract: the legitimate execution path (allowlisted env present, absolute `argv[0]`, successful cgroup enrollment) is byte-for-byte unchanged in observable behaviour; only the leak/escape surface is removed.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Allowlist too tight | a var the engine/tools need is omitted | tool execution fails inside the sandbox; caught by the integration smoke test that runs a `PATH`-needing tool under the filtered `environ_map`; allowlist widened deliberately at PLAN |
| Allowlist too loose | a secret-bearing var slips into the allowlist | `test_bwrap_argv_omits_daemon_secrets` asserts the `ZOMBIE_*` deny-prefix is absent regardless of allowlist contents |
| `--new-session` on a non-bwrap tier | bwrap flag where there is no bwrap (`dev_none`) | gated on `sandboxed`; `dev_none` unchanged (out of scope, trusted-dev). The `environ_map` filter is tier-uniform and harmless on `dev_none`. |
| `no_new_privs` prctl fails | kernel rejects the prctl | `child_exec` returns `SANDBOX_FAIL_EXIT` (fail-closed, Invariant 7) — the lease is classified a sandbox failure, never run |
| Tool needs a setuid helper | `ping`/`sudo`/`mount` post-`no_new_privs` | the helper fails inside the sandbox — **acceptable by design** (untrusted agents get no privilege escalation); PLAN enumerates whether any engine-invoked tool relies on one |
| Relative `argv[0]` reaches spawn | a future `buildArgv` change emits a relative path | `test_relative_argv0_rejected` + the runtime guard fail-close the lease |
| cgroup enrollment fails | transient `/sys/fs/cgroup` write error | **Fix A** refuses the lease fail-closed (kills the child); the kill domain is never silently empty, and the child never runs unmetered |
| Group-kill misses a descendant | child re-parents / breaks its own pgroup | `test_pgroup_kill_reaps_descendant_tree` (incl. adversarial `setsid`) fails; cgroup tree-kill remains the primary domain regardless |

---

## Invariants

1. **No daemon secret in the child environment** — `ZOMBIE_*` (incl. `ZOMBIE_RUNNER_TOKEN`) never appears in the child's environ or argv, on every sandboxed tier incl. `registry_allowlist`. Enforced by `test_bwrap_argv_omits_daemon_secrets` + `test_planted_token_absent_from_child_env`.
2. **Child cannot gain privilege** — `NoNewPrivs: 1` (Dim 1.5) inside `__execute`; setuid binaries in the RO mounts are inert. (`CapEff: 0` via `--cap-drop ALL` is re-homed to [`M84_006`](./M84_006_P2_API_RUNNER_SANDBOX_DEPTH.md).)
3. **No controlling terminal** — `--new-session` detaches it (Dim 1.6); TIOCSTI injection is closed.
4. **Every daemon fd is `CLOEXEC`** — re-homed to [`M84_006`](./M84_006_P2_API_RUNNER_SANDBOX_DEPTH.md) §2 (proof-only; already closed by Zig defaults).
5. **`argv[0]` is always absolute** — runtime guard + `test_relative_argv0_rejected`.
6. **Kill domain is never silently empty** — enrollment failure refuses the lease fail-closed (Fix A); `killChild` always also signals the process group (Fix B). Enforced by `test_kill_survives_cgroup_enrollment_failure` + `test_pgroup_kill_reaps_descendant_tree`.
7. **Legitimate path unchanged** — the allowlisted env + absolute argv[0] + enrolled case produces an identical observable outcome; a golden-argv test pins the argv shape (now incl. the new flags).
8. **Single-owner reap (spawn-migration handle-ownership contract)** — the supervisor is the **sole** reaper: exactly one `wait()`, guarded by `reaped`. The `process.Child` wrapper from `std.process.spawn` is **non-authoritative** — `child.wait()` is never called outside the supervisor's reap path, and `child.kill()` is never called (containment is cgroup-atomic via `killChild`). Enforced as a code comment at the reap site in `child_supervisor.zig`. No new test — the spawn-mechanics tests live in **M82_001 Batch 6** (M82 is DONE).

---

## Test Specification (tiered)

> **Lane (plan-eng-review F8 / Indy CTO decision):** the Linux-only integration tests run on a **dedicated runner integration lane** — `make test-integration-runner` → `zig build --build-file build_runner.zig test-integration` — **not** the app `test-integration` (Docker/Postgres/Redis) and **not** the fast `test-unit-zigrunner` lane. Rationale: these tests fork real bwrap children, mount `/proc`, fault-inject cgroup enrollment, and `kill(-pid)` a fork tree — a distinct privileged-Linux execution environment (bwrap + cgroup v2 + user-ns), which is a genuine distinct caller need. They are `builtin.os.tag == .linux`-gated (`SkipZigTest` on macOS). **macOS dev-loop proof = cross-compile the runner TEST graph**: `zig build --build-file build_runner.zig test-integration -Dtarget=x86_64-linux && -Dtarget=aarch64-linux` (compile-check the Linux-only bodies; real execution is the CI Linux lane).

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_child_env_filtered_to_allowlist` | the `environ_map` passed to `spawn` contains exactly the allowlist (no other daemon vars) |
| 1.2 | unit | `test_child_env_omits_daemon_secrets` | the child's environ contains no `ZOMBIE_*` var regardless of allowlist contents |
| 1.3 | integration-runner | `test_planted_token_absent_from_child_env` | daemon env has `ZOMBIE_RUNNER_TOKEN=probe`; child's `/proc/self/environ` AND in-child `getenv` lack `probe` |
| 1.5 | integration-runner | `test_child_no_new_privs` | child `/proc/self/status` → `NoNewPrivs: 1` |
| 1.6 | unit + integration-runner | `test_bwrap_argv_new_session` / `test_child_no_controlling_tty` | argv contains `--new-session`; child from a pty parent cannot `TIOCSTI`-inject |
| 1.7 | unit | `test_env_filter_and_new_session_under_registry_allowlist` | filtered `environ_map` applied AND `--new-session` present under BOTH `deny_all` AND `registry_allowlist` |
| 3.1 | unit | `test_relative_argv0_rejected` | `forkExec` with a relative `argv[0]` → `error.SandboxArgvNotAbsolute`, no spawn |
| 4.1 | integration-runner | `test_pgroup_kill_reaps_descendant_tree` | child (incl. one that `setsid`s) forks grandchild+great-grandchild; `kill(-pid)` → all reaped |
| 4.2 | integration-runner | `test_kill_survives_cgroup_enrollment_failure` | inject `addProcess` failure → lease refused fail-closed (Fix A); success-then-race → pgroup also signalled (Fix B) |

> Tests for cap-drop (old 1.4), fd-proof (old 2.x), and containment (old 5.x) moved to **[`M84_006`](./M84_006_P2_API_RUNNER_SANDBOX_DEPTH.md)**.

- **Regression:** the existing runner suite (`make test-unit-zigrunner`) + the app `make test` must pass unchanged — the legitimate sandboxed lease still runs end-to-end.
- **Idempotency/replay:** N/A.

---

## Acceptance Criteria

- [ ] Filtered `environ_map` passed to `spawn`; child environ is allowlist-only; `ZOMBIE_*` absent — verify: `test_child_env_filtered_to_allowlist` + `test_child_env_omits_daemon_secrets`
- [ ] Allowlist derived from a verified in-child env-read enumeration (engine + tool subprocesses, not the illustrative list); a `PATH`-needing tool runs under the filter — verify: integration smoke + the PLAN enumeration recorded in Discovery
- [ ] Planted-token integration test green — verify: `make test-integration-runner` (`test_planted_token_absent_from_child_env`)
- [ ] Child has `NoNewPrivs:1` — verify: `test_child_no_new_privs`
- [ ] `--new-session` present under BOTH `deny_all` and `registry_allowlist` (Dim 1.7) — verify: `test_env_filter_and_new_session_under_registry_allowlist`
- [ ] Child has no controlling terminal — verify: `test_child_no_controlling_tty`
- [ ] Relative `argv[0]` rejected fail-closed — verify: `test_relative_argv0_rejected`
- [ ] Kill domain never silently empty (both fixes) — verify: `test_kill_survives_cgroup_enrollment_failure` + `test_pgroup_kill_reaps_descendant_tree`
- [ ] (cap-drop `CapEff:0`, fd-proof, network/fs containment → re-homed to [`M84_006`](./M84_006_P2_API_RUNNER_SANDBOX_DEPTH.md))
- [ ] New `test-integration-runner` lane wired (`build_runner.zig` step + `make/test-integration.mk`); runner TEST graph cross-compiles for both linux targets
- [ ] `make lint` clean · `make test-unit-zigrunner` + `make test-integration-runner` pass · cross-compile both linux targets
- [ ] `gitleaks detect` clean · no file over 350 lines added
- [ ] `docs/AUTH.md` notes the token is `environ_map`-isolated (filtered allowlist) from the sandbox

---

## Eval Commands (post-implementation)

```bash
# E1: env-filter + new-session present, secrets absent (unit)
zig build --build-file build_runner.zig test 2>&1 | grep -E "env_filtered_to_allowlist|omits_daemon_secrets|new_session|no_new_privs"
# E2: runner unit + app suites (legitimate path unchanged)
make test-unit-zigrunner 2>&1 | tail -5 && make test 2>&1 | tail -5
# E3: runner integration lane (planted token + caps + NNP + no-tty + fd + kill-tree + net/fs)
make test-integration-runner 2>&1 | tail -8
# E4: dev-loop proof — Linux-only test bodies compile (cross-compile the TEST graph)
zig build --build-file build_runner.zig test-integration -Dtarget=x86_64-linux 2>&1 | tail -3
# E5: no daemon secret literal leaked into argv builder
git grep -n 'ZOMBIE_RUNNER_TOKEN' src/runner/sandbox_args.zig | head
```

---

## Dead Code Sweep

**1. Orphaned files — none expected.**

| File to delete | Verify |
|----------------|--------|
| N/A — additive hardening | — |

**2. Orphaned references.** N/A — no symbols removed.

---

## Discovery (consult log)

- **Origin (Jun 03, 2026):** surfaced in the M82_001 `forkExec → process.spawn` CTO/threat-model walkthrough. Process-boundary findings (env inheritance → token exfil; non-CLOEXEC fd inheritance; relative-`argv[0]` PATH resolution). M82 is behaviour-preserving and does not change them.
  - **Indy go (verbatim, Jun 03, 2026):** _"ensure the sandbox-hardening followup is done (separate spec)"_ — context: file the findings as their own security spec, not folded into the toolchain bump.
- **CTO review — ChatGPT (Jun 03, 2026):** concurred with the architecture (spawn for creation, cgroup as authoritative kill boundary, raw-fd supervisor preserved) and ranked the hardening above the migration. Folded in: env allowlist + fail-closed deny-prefix; FD hygiene as the named "strongest" finding; the kill-tree regression; `--cap-drop ALL` and `--new-session` as cheap defense-in-depth; env-inheritance and FD-inheritance as the top attack classes.
- **CTO adversarial review — Orly (Jun 03, 2026, code-grounded multi-agent):** post-refutation verdicts ranked env (§1, P0) > fd (§2, mitigated/proof) > argv (§3, guard) > kill-tree (§4, caveat). CSPRNG cross-spec note belongs to M82 (DONE).
- **plan-eng-review (Jun 05, 2026, 36-agent code-grounded workflow + adversarial refutation):** every `file:line` claim re-verified against `main`; candidate escapes adversarially refuted. Outcomes:
  - **Refuted (do not re-litigate):** `/proc/1/environ` token read after clearenv (REFUTED — `--unshare-all` pid-ns + `--proc` makes the daemon invisible and bwrap is not pid 1 in the child ns); ptrace cross-read (closed by pid-ns); pooled `http.Client` socket leak (per-call `deinit`); workspace inter-lease residue (per-lease lifecycle in `daemon/loop.zig`); in-memory lease-secret residue (different boundary). 
  - **Confirmed live:** §1 env leak (`child_process.zig:32` no `environ_map`; no `--clearenv`); §4 empty-cgroup kill bug (worse than written — fork-failed `addProcess` also bypasses `memory.max`/`cpu.max`).
  - **Re-shaped:** §2 → proof-only (no production patch — RULE NDC); §1 allowlist → derived from verified enumeration (the in-child engine's only optional read is `NULLCLAW_OBSERVER`; `PATH`/`HOME`/`TMPDIR` carry the load; NullClaw `config.zig:940-980` reads `NULLCLAW_*` but the runner injects provider/model via the lease, not env — PLAN to confirm).
  - **Promoted to Dimensions (F2):** `--cap-drop ALL` (1.4), explicit `PR_SET_NO_NEW_PRIVS` (1.5 — Landlock does NOT set NNP for us; `landlock.zig` succeeds via userns `CAP_SYS_ADMIN`), `--new-session` (1.6).
  - **Indy decisions (verbatim, Jun 05, 2026):**
    - §4 kill-domain fix — _"Both (fail-closed + always-pgroup)"_ → Fix A + Fix B (Dim 4.2).
    - cap-drop / no-new-privs / new-session — _"Promote all three to Dimensions"_ → Dims 1.4/1.5/1.6.
    - §5 containment verification — _"Keep §5 in M84"_ → retained, framing tightened to characterization-only (mechanism owned by M80).
    - test lane — _"Orly CTO decide and tell me so i can approve. I am looking at a clean separation? leaning towards 2. so the existing test-integration for the runner runs as is."_ → **CTO decision: dedicated `test-integration-runner` lane** (option 2). Justified against the no-new-make-target rule by a distinct privileged-Linux execution environment (bwrap + cgroup v2 + user-ns) that neither the unit lane nor the app integration lane provides. **Pending Indy approval to land this spec.**
  - **Renumber (F7):** `M84_001` → `M84_003` (collision with the shipped enrollment spec + the pending fleet-operator-plane spec).
- **Orly CTO adversarial review of the rewritten spec (Jun 05, 2026, code-grounded):** verdict **YELLOW → fixed to GREEN-ready**. Threat model correct, every `file:line` claim held, lane decision justified, kill-domain both-fix proven reap-safe, §1.2 clearenv premise confirmed. Fixes folded in:
  - **P0 — `prctl` interface line would not compile.** Zig 0.16 `prctl(option: i32, ...)` rejects the `PR` enum literal → corrected to `@intFromEnum(std.os.linux.PR.SET_NO_NEW_PRIVS)`; Interfaces block marked illustrative.
  - **P1 — `--cap-drop ALL` ↔ `no_new_privs` coupling** made explicit (NNP before `landlock_restrict_self`; 1.4 can't land without 1.5). `bwrap --cap-drop` version-probe + setuid-tool breakage added to Failure Modes/PLAN.
  - **P1 — LENGTH:** `child_supervisor.zig` (326/350) flagged as the likely file-cap trip under Fix A → `enrollOrFail` helper extraction banked.
  - **P2 — `--new-session` ↔ kill-domain** reconciled (bwrap stays the pgroup leader); Fix B pid-reuse safety pinned to the `wait()`-after-`kill()` ordering.
  - **P2/P3 — test hygiene:** Dim 1.7 got an explicit named test + Acceptance line; Dim 2.4 split into its own `test_forkexec_progress_node_none`; `SpawnOptions.progress_node` field-name flagged PLAN-verify.
- **Orly CTO adverse review — second pass (Jun 05, 2026, post-egress-rescope, code-grounded against `main`).** Every `file:line` re-verified; Zig 0.16 stdlib API surface read directly. Outcomes folded in:
  - **`SpawnOptions.environ_map` EXISTS** (`std/process.zig:368`, `?*const Environ.Map = null`) → §1 mechanism **LOCKED to filtered `environ_map`** (Indy, Jun 05; Decomposition "Mechanism (LOCKED)"). The prior "`--clearenv` covers a future direct-exec path" rationale was **inverted** (`--clearenv` is bwrap-only); `--clearenv`/`--setenv` are not used.
  - **§4 Fix A depends on Fix B** — Fix A's kill must not route through the empty-`scope.kill()` bug; note added under §4.
  - **LENGTH** — `supervise` itself is ~73 lines (near the fn cap) before Fix A lands inside it; LENGTH row updated.
  - **§1.2 allowlist** — verified in-child env sweep: `HOME` load-bearing (config-dir → `error.HomeDirNotFound`), `NULLCLAW_OBSERVER` + `NULLCLAW_*` optional; **no engine read of `PATH`/`TMPDIR`**, but tool subprocesses (`git`/`gh`) need `PATH` and TLS needs the CA bundle (RO `/etc` covers `/etc/ssl/certs`; some distros use `SSL_CERT_FILE`). PLAN enumeration must cover **tool-subprocess** env, not just engine config reads.
  - **§5.1 reworded**; egress gap now owned by the filed **M84_004**. Baremetal deployment → residual is outbound exfil, not lateral movement.
  - **Confirmed live (held under code-grounding):** §1 env leak (`child_process.zig:32`), §4 empty-cgroup kill (`cgroup.zig` `kill()` succeeds on empty cgroup; `child_supervisor.zig:159` warn-and-continue), §1.5 Landlock sets **no** `prctl` (`landlock.zig` rides userns `CAP_SYS_ADMIN`).
- **Remaining security gaps (for the implementing agent, beyond the Sections):**
  1. **`macos_seatbelt` is an unimplemented placeholder** — `establishSandbox` fail-closes any non-Linux host for a non-`dev_none` tier (`child_supervisor.zig:201`). **DEFERRED — Indy (Jun 03, 2026): _"I deferred this, since seatbelt is deprecated long back"_.** macOS isolation story: `dev_none` for trusted local use; untrusted agents on a Mac run inside a Linux VM/guest where the real tiers apply. Keep a test pinning *"macOS + non-`dev_none` → lease refused"* so a self-reported tier can never imply isolation that does not exist.
  2. **Network egress allowlist** — now **filed as [`M84_004`](./M84_004_P1_API_RUNNER_EGRESS_ALLOWLIST.md)** (own-netns child + default-deny DNS-pinning proxy; supersedes the "nftables roadmap" framing). Closes the §5.1 gap; M84_003 keeps only the characterization test that flips when M84_004 lands.
  3. **Inherited stderr (fd 2)** — the child's fd 2 is the daemon's real stderr (`.stderr = .inherit`); in-child engine logs interleave into the operator stream. Low severity (log noise / weak per-lease attribution), not a secret capability. Out of strict scope; note only.
  4. **Unbounded activity-frame flood on fd 1** — the child controls the frame protocol on its own stdout; `readResult` caps total result bytes (`MAX_RESULT_BYTES`) but not activity-frame count/rate. Frame-protocol hardening, out of strict M84 scope; note for a follow-up.
- **PLAN decisions to bank** — the final env passthrough allowlist (verified enumeration); confirm §2 stays proof-only after the PLAN fd-audit; confirm no `NULLCLAW_*` is daemon-static-via-env and relied upon. **`RUNNER_NETWORK_POLICY` is a PARENT-only read (`config.zig:70`) and must NOT be in the child allowlist.** Verify the Zig 0.16 `prctl` enum coercion (`@intFromEnum`), the `SpawnOptions.progress_node` field name, and `bwrap --cap-drop` support before authoring those lines.
- **Consults** — {Architecture / Legacy-Design / gate-flag triage: question + Indy's decision, as they arise.}
- **Skill chain outcomes** — {`/write-unit-test`, `/review`, `/review-pr` results.}
- **Deferrals** — every "deferred to follow-up" needs an Indy-acked verbatim quote here (seatbelt above is acked; the stderr + frame-flood items are *noted, not deferred-from-scope* — they were never in scope).

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits the negative/regression-test coverage vs this Test Specification (the eight invariants). | Clean. Iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs `docs/AUTH.md`, `dispatch/write_zig.md`, Failure Modes, Invariants (esp. deny-prefix completeness, the allowlist enumeration, the both-fix kill domain). | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR against the immutable diff. | Comments addressed before human review/merge. |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Runner unit | `make test-unit-zigrunner` | {paste snippet} | |
| App suite (regression) | `make test` | {paste snippet} | |
| Runner integration (token + caps + NNP + fd + kill-tree + net/fs) | `make test-integration-runner` | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| Cross-compile (prod + TEST graph) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && zig build --build-file build_runner.zig test-integration -Dtarget=x86_64-linux` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |

---

## Out of Scope

- **`dev_none` tier env filtering** — that tier runs with no isolation by design (trusted dev); §1 applies only to sandboxed tiers. The release-mode guard (`main.zig`) already refuses `dev_none` as the prod default.
- **Namespace / Landlock / cgroup-membership model changes** — this workstream hardens only the process-boundary env/fd/capability/process-group surface; the bwrap namespace set, the in-child Landlock policy, and the cgroup kill domain are unchanged (owned by the M80 runner-fleet specs). **§5 adds end-to-end containment *verification* (network + fs) as M84 acceptance — proof only, no mechanism change.**
- **Network egress allowlist** — filed as **[`M84_004`](./M84_004_P1_API_RUNNER_EGRESS_ALLOWLIST.md)** (own-netns child + default-deny DNS-pinning proxy enforcing operator-declared `allow_hosts`); `--share-net` semantics are untouched *here*. §5.1 *verifies* the `deny_all` default and *characterizes* the `registry_allowlist` gap; M84_004 implements the enforcement.
- **Inherited stderr re-plumbing + activity-frame rate caps** — noted in Discovery; frame-protocol / log-plumbing hardening, separate from env/fd inheritance.
- **Rotating / scoping the runner token** so a leak is lower-impact — a control-plane credential-model change, not a runner-side patch.
