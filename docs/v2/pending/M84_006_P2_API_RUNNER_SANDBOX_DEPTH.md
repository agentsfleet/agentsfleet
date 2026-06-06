# M84_006: Runner sandbox defense-in-depth — cap-drop, fd-proof, containment verification

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 006
**Date:** Jun 05, 2026
**Status:** PENDING
**Priority:** P2 — defense-in-depth, **deferred behind the untrusted-runner GA trigger** (becomes P1 when usezombie commits to untrusted / customer-operated runners). Not launch-blocking: v2 launches platform-operated (trusted) runners on the primary boundary (`--unshare-all` user/pid/net namespaces + Landlock + cgroup), which all ship in M84_003.
**Categories:** API
**Batch:** B1 — standalone; lands after M84_003 (depends on its `no_new_privs`).
**Branch:** {feat/m84-runner-sandbox-depth — added at CHORE(open)}
**Depends on:** **M84_003 (launch slice)** — §1 of this spec (`--cap-drop ALL`) removes the userns `CAP_SYS_ADMIN` that `landlock_restrict_self` relies on, so it requires M84_003's `no_new_privs` (M84_003 §1.5) to already be set in-child before `landlock.applyPolicy`. Relates to **M84_004 (egress)** — §3's network-egress verification asserts what M84_004 enforces.
**Provenance:** agent-generated — re-homed from M84_003 at the Orly CEO launch re-cut (Jun 05, 2026). These three slices were in M84_003 originally; the CEO review cut them from the launch slice (cap-drop is landlock-coupled/risky, fd-proof asserts a non-bug, containment ties to the deferred egress work) and Indy moved them to their own spec so M84_003 is physically just the launch slice.

> **Provenance is load-bearing.** All claims were code-grounded against `main` during the M84_003 reviews. Re-confirm at PLAN: `landlock.zig` sets no `prctl` (rides userns `CAP_SYS_ADMIN`); `cgroup.zig`/`child_process.zig` fd hygiene; `network.zig`/`landlock.zig` containment mechanisms.

**Canonical architecture:** [`docs/architecture/runner_fleet.md`](../../architecture/runner_fleet.md) §Sandbox tiers / §Egress model. This spec hardens the **capability + fd surface** and **verifies** containment; it does not change the namespace/Landlock/cgroup mechanisms (M80-owned) or implement egress (M84_004).

---

## Implementing agent — read these first

1. `src/runner/sandbox_args.zig` — `appendBwrap`; where the `--cap-drop ALL` flag is emitted (§1). Single-source the flag constants (RULE UFS).
2. `src/runner/engine/landlock.zig` — `applyPolicy` → `landlock_restrict_self` with **no** `prctl`; it succeeds today only via the userns `CAP_SYS_ADMIN`. Once §1 drops that cap, `no_new_privs` (shipped in M84_003) is its only remaining precondition — the coupling that makes M84_003 a hard dependency.
3. `src/runner/child_process.zig` + `src/runner/child_supervisor.zig` + `src/runner/engine/cgroup.zig` — for §2: prove every daemon fd is `CLOEXEC` and no fd ≥3 reaches the child.
4. `src/runner/engine/network.zig` + `src/runner/engine/landlock.zig` — for §3: the `deny_all` empty-netns egress and the Landlock fs binds being verified.
5. `dispatch/write_zig.md` — all `*.zig` edits.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `build(m84): runner sandbox depth — cap-drop ALL, fd CLOEXEC proof, containment verification`
- **Intent (one sentence):** Add the secondary sandbox belts deferred from the launch slice — drop all Linux capabilities, prove no daemon file descriptor is inherited, and pin the network/filesystem containment guarantees as regression tests.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. **Confirm M84_003's `no_new_privs` is live** before adding `--cap-drop ALL` (else Landlock fails closed on every lease); **live-probe `bwrap --cap-drop`** on the Linux lane.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`**:
  - **RULE UFS** — `--cap-drop`/`ALL` flag literals are single-sourced constants in `sandbox_args.zig`, reused in tests.
  - **RULE NDC / NLR** — §2 is **proof-only**: Zig opens are `CLOEXEC` and no daemon fd ≥3 is open at spawn, so §2 adds assertions + regression tests, never a no-op production guard.
  - **RULE NLG** — pre-2.0: no "legacy" framing.
- **`dispatch/write_zig.md`** — tagged-union results, `errdefer`, cross-compile both linux targets.
- **`docs/LOGGING_STANDARD.md`** — any `caps_dropped`/`fd_audit` emit follows the logfmt envelope.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | **yes** — `*.zig` edits | Read `dispatch/write_zig.md`; cross-compile both linux targets. |
| UFS | **yes** — `--cap-drop`/`ALL` literals | Named constants in `sandbox_args.zig`, reused in tests. |
| LENGTH | **maybe** — `appendBwrap` grows | Keep `appendBwrap` ≤50 lines; extract a cap-emit helper if needed. |
| LOGGING | **maybe** — new audit emit | Envelope unchanged; never log a secret. |
| LIFECYCLE | **no** | fd hygiene tightens flags; adds no resource. |

---

## Overview

**Goal (testable):** The sandboxed child holds **`CapEff: 0`** (no Linux capabilities even inside its user namespace); **no** non-`CLOEXEC` daemon fd and no unexpected fd ≥3 reaches it; and the existing network/filesystem containment (empty netns under `deny_all`, Landlock fs binds) is pinned by regression tests that stay green — each proven by a negative/regression test on the runner integration lane.

**Problem:** M84_003's launch slice ships the primary sandbox boundary (namespaces + Landlock + cgroup) plus `no_new_privs` and `--new-session`. Three secondary belts were cut from launch and live here: (1) **capabilities** — without `--cap-drop ALL` the child relies entirely on `--unshare-all` to neuter caps namespace-locally; a userns-escape chain would find caps available; (2) **fd inheritance** — the classic sandbox escape is inheriting an open capability (socket, db handle, control pipe) across `exec`; this is already closed (Zig CLOEXEC defaults, no daemon fd ≥3 at spawn) but is unproven by tests; (3) **containment** — the network/fs isolation holds today but has no regression pin, so a future mechanism change could silently weaken it.

**Solution summary:** Emit `--cap-drop ALL` (gated on M84_003's `no_new_privs` so Landlock still applies), add assertions + regression tests proving no fd is inherited, and add characterization tests pinning the `deny_all` egress + Landlock fs containment. No mechanism changes — depth and proof only.

---

## Prior-Art / Reference Implementations

- **bwrap `--cap-drop ALL`** is the canonical hardened-sandbox capability drop (Flatpak, `bubblewrap(1)`); extend the existing `appendBwrap` flag-emit style.
- **CLOEXEC-by-default** — Zig `std.fs`/`std.Io` opens set `O_CLOEXEC`; §2 proves the daemon holds no non-CLOEXEC fd at spawn, mirroring how `posix_spawn` users audit fd inheritance.
- The containment tests reuse the **`test-integration-runner`** lane (created in M84_003).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/sandbox_args.zig` | EDIT | `appendBwrap`: emit `--cap-drop ALL` (§1); add the cap-drop flag constants. |
| `src/runner/sandbox_args_edge_test.zig` (+ runner integration tests) | EDIT | Golden-argv `--cap-drop ALL`; Linux-only integration: `CapEff: 0`, marker/stray-fd absence, network/fs containment. |
| `make/test-integration.mk` | EDIT | Register the new integration tests on the existing `test-integration-runner` lane. |
| `src/runner/child_process.zig` | (no prod change) | §2 assertion: `forkExec` leaves `progress_node == .none`; call-site assertion only. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** additive depth + proof; **patch**, behaviour-preserving (the legitimate lease is unchanged). Three independent slices sharing the integration lane.
- **Alternatives considered:** (a) **Ship in the M84_003 launch slice** — rejected at the CEO re-cut: `--cap-drop ALL` is landlock-coupled/risky and the rest is non-launch-blocking proof. (b) **Drop `--cap-drop` entirely** — rejected: it is real defense-in-depth for the untrusted tier this spec gates; keep it, just deferred.

---

## Sections (implementation slices)

### §1 — Capability drop (`--cap-drop ALL`)

`appendBwrap` emits `--cap-drop ALL` so the child holds no capabilities even inside its own user namespace. **Coupling (hard):** this removes the userns `CAP_SYS_ADMIN` that `landlock_restrict_self` relies on today, so M84_003's `no_new_privs` (set in-child before `landlock.applyPolicy`) **must** already be live, or Landlock fails closed on every lease. `--cap-drop` is bwrap-version-gated (~0.4.0+); a build whose bwrap lacks it errors out → lease fails closed.

- **Dimension 1.1** — `appendBwrap` emits `--cap-drop ALL` on sandboxed tiers → Test `test_bwrap_argv_cap_drop_all` (golden argv)
- **Dimension 1.2** — the child holds no capabilities → Test `test_child_has_no_caps` (child reads `/proc/self/status` → `CapEff: 0000000000000000`)

### §2 — File-descriptor hygiene (proof only — RULE NDC)

`std.process.spawn` wires stdio but does not close arbitrary parent fds. Verified already closed (pipes are `pipe2(CLOEXEC)`; only fd 0/1/2 cross via `dup2`; the control-plane `http.Client` is per-call `deinit`'d; cgroup fds are `defer`-closed; the supervisor reaps before the next fork). So this is **assertions + a regression sweep**, never a production patch.

- **Dimension 2.1** — Discovery enumerates every daemon open site and records each as already-`CLOEXEC` → assertion table
- **Dimension 2.2** — a marker fd opened by the daemon is not accessible in the child (`fcntl(N, F_GETFD)` → `EBADF`) → Test `test_marker_fd_not_inherited_by_child`
- **Dimension 2.3** — the child sees no unexpected open fd ≥3 beyond wired stdio → Test `test_no_stray_fds_in_child`
- **Dimension 2.4** — `forkExec` leaves `progress_node == .none` so std's `prog_fileno = 3` path never `dup2`s a fourth fd → Test `test_forkexec_progress_node_none` (PLAN: verify the field name on 0.16 `SpawnOptions`)
- **Dimension 2.5** — the control-plane client's transient socket carries `FD_CLOEXEC` (a future pooled client cannot silently re-open the vector) → Test `test_control_plane_socket_is_cloexec`

### §3 — Sandbox containment verification (characterization — proof only)

Pins the existing network + filesystem containment so a future mechanism change in the owning specs (M80 fleet, M84_004 egress) cannot silently weaken it. No mechanism change here.

- **Dimension 3.1** — under `deny_all`, the child cannot reach the network: a `connect()` to an external host fails at the kernel (empty netns via `--unshare-all`) → Test `test_sandboxed_child_network_denied`
- **Dimension 3.2** — the child cannot read host `/home`/`/var`/`/root` (never bound) and cannot write `/etc` (RO bind); only the lease workspace + `/tmp` tmpfs are writable (Landlock wired, `landlock.zig`) → Test `test_sandboxed_child_fs_isolation`
- **Dimension 3.3** — the `registry_allowlist` egress gap is pinned as a characterization test that flips when [`M84_004`](./M84_004_P1_API_RUNNER_EGRESS_ALLOWLIST.md) lands → Test `test_registry_allowlist_egress_unrestricted_today`

---

## Interfaces

> **Illustrative — exact flags/field-names verified at PLAN.** Contract, not implementation.

```
# src/runner/sandbox_args.zig — single-sourced constants (RULE UFS)
const CAP_DROP_FLAG = "--cap-drop";
const CAP_DROP_ALL  = "ALL";
# Ordering contract (hard): M84_003's no_new_privs MUST be set in-child before
# landlock.applyPolicy; once --cap-drop ALL removes the userns CAP_SYS_ADMIN,
# no_new_privs is landlock_restrict_self's only remaining precondition.
```

Contract: the legitimate lease (no_new_privs live, bwrap supports `--cap-drop`) is observably unchanged; only the capability/fd surface is tightened and the containment guarantees are pinned.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| `--cap-drop` unsupported | bwrap older than ~0.4.0 | bwrap errors on the unknown flag → `forkExec` spawn fails → lease fails closed. PLAN live-probes `bwrap --cap-drop`. |
| cap-drop without no_new_privs | M84_003 §1.5 not live | `landlock_restrict_self` fails closed → lease refused. Hard dependency on M84_003; PLAN confirms NNP first. |
| Tool needs a setuid helper | `ping`/`sudo` post-cap-drop | helper fails inside the sandbox — acceptable by design (no escalation); PLAN enumerates engine-invoked tools. |
| Stray capability fd inherited | a daemon open site forgets `CLOEXEC` | `test_marker_fd_not_inherited_by_child` + `test_no_stray_fds_in_child` fail the build before merge. |
| Containment regressed | a future mechanism change weakens netns/Landlock | the §3 characterization tests fail; the regression is in the owning spec's mechanism, pinned here. |

---

## Invariants

1. **Child holds no capabilities** — `CapEff: 0` inside `__execute`. Enforced by `test_child_has_no_caps`. (Privilege escalation is already blocked by M84_003's `no_new_privs`.)
2. **Every daemon fd is `CLOEXEC`** — no unexpected fd ≥3 reaches the child; `progress_node` stays `.none`. Enforced by the assertion table + `test_marker_fd_not_inherited_by_child` + `test_no_stray_fds_in_child` + `test_control_plane_socket_is_cloexec`. **Proof, not patch.**
3. **Containment is pinned** — `deny_all` empty netns + Landlock fs isolation stay green; the `registry_allowlist` gap is characterized so it flips when M84_004 lands. Enforced by the §3 tests.
4. **Legitimate path unchanged** — no_new_privs-live + bwrap-supports-cap-drop produces an identical observable outcome; a golden-argv test pins the argv shape.

---

## Test Specification (tiered)

> **Lane:** the Linux-only integration tests run on the **`test-integration-runner`** lane (created in M84_003). `builtin.os.tag == .linux`-gated (`SkipZigTest` on macOS); macOS proof = cross-compile the runner TEST graph for both linux targets.

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `test_bwrap_argv_cap_drop_all` | sandboxed argv contains `--cap-drop ALL` |
| 1.2 | integration-runner | `test_child_has_no_caps` | child `/proc/self/status` → `CapEff: 0000000000000000` |
| 2.1 | (assertion) | Discovery table | each audited daemon open returns an fd with `FD_CLOEXEC` set |
| 2.2 | integration-runner | `test_marker_fd_not_inherited_by_child` | daemon opens marker fd N; child `fcntl(N, F_GETFD)` → `EBADF` |
| 2.3 | integration-runner | `test_no_stray_fds_in_child` | child `/proc/self/fd` has only 0/1/2 |
| 2.4 | unit | `test_forkexec_progress_node_none` | `forkExec` asserts `progress_node == .none`; no fd 3 |
| 2.5 | unit | `test_control_plane_socket_is_cloexec` | control-plane client socket has `FD_CLOEXEC` |
| 3.1 | integration-runner | `test_sandboxed_child_network_denied` | `deny_all` child `connect()` to external host fails at the kernel |
| 3.2 | integration-runner | `test_sandboxed_child_fs_isolation` | child: write `/etc/<x>` denied; `/home`/`/var`/`/root` absent; workspace write ok |
| 3.3 | integration-runner | `test_registry_allowlist_egress_unrestricted_today` | `registry_allowlist` child reaches an external host (pins the gap; flips when M84_004 lands) |

- **Regression:** `make test-unit-zigrunner` + the M84_003 launch-slice tests stay green; the legitimate sandboxed lease still runs end-to-end.

---

## Acceptance Criteria

- [ ] `--cap-drop ALL` on sandboxed argv; child `CapEff: 0` — verify: `test_bwrap_argv_cap_drop_all` + `test_child_has_no_caps`
- [ ] No non-CLOEXEC / stray fd inherited (proof + sweep) — verify: `test_marker_fd_not_inherited_by_child` + `test_no_stray_fds_in_child` + `test_control_plane_socket_is_cloexec` + `test_forkexec_progress_node_none`
- [ ] Containment pinned: `deny_all` network denied; host fs isolated; `registry_allowlist` gap characterized — verify: `test_sandboxed_child_network_denied` + `test_sandboxed_child_fs_isolation` + `test_registry_allowlist_egress_unrestricted_today`
- [ ] M84_003 `no_new_privs` confirmed live before cap-drop; `bwrap --cap-drop` probed on the Linux lane — verify: PLAN note in Discovery
- [ ] `make lint` clean · `make test-integration-runner` passes · cross-compile both linux targets
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: cap-drop present (unit golden argv)
zig build --build-file build_runner.zig test 2>&1 | grep -E "cap_drop_all|progress_node_none|control_plane_socket_is_cloexec"
# E2: integration lane (caps + fd + net/fs containment)
make test-integration-runner 2>&1 | tail -10
# E3: dev-loop proof — Linux-only bodies compile
zig build --build-file build_runner.zig test-integration -Dtarget=x86_64-linux 2>&1 | tail -3
# E4: no cap-drop literal re-spelled outside the constant
git grep -n '"--cap-drop"' src/runner | grep -v sandbox_args.zig | head
```

---

## Dead Code Sweep

**1. Orphaned files — none expected.**

| File to delete | Verify |
|----------------|--------|
| N/A — additive depth | — |

**2. Orphaned references.** N/A — no symbols removed.

---

## Discovery (consult log)

- **Origin (Jun 05, 2026):** re-homed from M84_003 at the Orly CEO launch re-cut. The three slices were cut from the launch slice: §1 cap-drop (landlock-coupled/risky), §2 fd-proof (asserts a non-bug), §3 containment (ties to the deferred M84_004 egress). Indy: _"create a new spec re home for 1.4, 2/5"_ — context: keep M84_003 physically the launch slice; gate this depth behind untrusted-runner GA.
- **Code-grounded facts (from the M84_003 reviews):** `landlock.zig` sets **no** `prctl` (rides userns `CAP_SYS_ADMIN`) → cap-drop without no_new_privs fail-closes every lease (hard dependency on M84_003 §1.5). §2 vector already closed (Zig CLOEXEC defaults; no daemon fd ≥3 at spawn). `registry_allowlist` is full host egress until M84_004.
- **PLAN decisions to bank:** confirm M84_003 `no_new_privs` is live; `bwrap --cap-drop` live-probe; `SpawnOptions.progress_node` field name on 0.16; enumerate engine-invoked tools needing a setuid helper.
- **Deferrals** — none beyond the workstream-level deferral (behind untrusted-runner GA, Indy-acked above).
- **Skill chain outcomes** — {`/write-unit-test`, `/review`, `/review-pr`.}

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage vs this Test Specification. | Clean. Iteration count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs Invariants, Failure Modes, `dispatch/write_zig.md`. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Runner integration | `make test-integration-runner` | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| Cross-compile (TEST graph) | `zig build --build-file build_runner.zig test-integration -Dtarget=x86_64-linux` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |

---

## Out of Scope

- **M84_003 launch slice** (env isolation, no_new_privs, new-session, argv guard, kill-domain) — the dependency, not this spec.
- **M84_004 egress enforcement** — §3 only *verifies* the `deny_all` default and *characterizes* the `registry_allowlist` gap; it does not implement egress filtering.
- **Namespace / Landlock / cgroup mechanism changes** — owned by the M80 runner-fleet specs; this spec hardens only the capability/fd surface and pins containment.
