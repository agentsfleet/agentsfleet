# M84_006: Runner sandbox defense-in-depth — cap-drop, containment verification

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
**Provenance:** agent-generated — re-homed from M84_003 at the Orly CEO launch re-cut (Jun 05, 2026). These slices were in M84_003 originally; the CEO review cut them from the launch slice (cap-drop is landlock-coupled/risky, fd-proof asserts a non-bug, containment ties to the deferred egress work) and Indy moved them to their own spec so M84_003 is physically just the launch slice. **The credential-fd inheritance proof (former §2) was split out to its own launch-safe spec [`M84_007`](./M84_007_P1_API_RUNNER_CREDENTIAL_FD_PROOF.md) (Jun 06, 2026)** — it is proof-only and ships for launch, whereas this spec's cap-drop (§1) + containment (§3) stay deferred behind the untrusted-runner GA trigger.

> **Provenance is load-bearing.** All claims were code-grounded against `main` during the M84_003 reviews. Re-confirm at PLAN: `landlock.zig` sets no `prctl` (rides userns `CAP_SYS_ADMIN`); `network.zig`/`landlock.zig` containment mechanisms.

**Canonical architecture:** [`docs/architecture/runner_fleet.md`](../../architecture/runner_fleet.md) §Sandbox tiers / §Egress model. This spec hardens the **capability surface** and **verifies** containment; it does not change the namespace/Landlock/cgroup mechanisms (M80-owned), implement egress (M84_004), or prove fd hygiene (M84_007).

---

## Implementing agent — read these first

1. `src/runner/sandbox_args.zig` — `appendBwrap`; where the `--cap-drop ALL` flag is emitted (§1). Single-source the flag constants (RULE UFS).
2. `src/runner/engine/landlock.zig` — `applyPolicy` → `landlock_restrict_self` with **no** `prctl`; it succeeds today only via the userns `CAP_SYS_ADMIN`. Once §1 drops that cap, `no_new_privs` (shipped in M84_003) is its only remaining precondition — the coupling that makes M84_003 a hard dependency.
3. `src/runner/engine/network.zig` + `src/runner/engine/landlock.zig` — for §3: the `deny_all` empty-netns egress and the Landlock fs binds being verified.
   _(The credential-fd proof that used to be §2 — `child_process.zig` / `cgroup.zig` fd hygiene — split to [`M84_007`](./M84_007_P1_API_RUNNER_CREDENTIAL_FD_PROOF.md).)_
5. `dispatch/write_zig.md` — all `*.zig` edits.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `build(m84): runner sandbox depth — cap-drop ALL, containment verification`
- **Intent (one sentence):** Add the secondary sandbox belts deferred from the launch slice — drop all Linux capabilities and pin the network/filesystem containment guarantees as regression tests. _(The credential-fd inheritance proof split to [`M84_007`](./M84_007_P1_API_RUNNER_CREDENTIAL_FD_PROOF.md).)_
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. **Confirm M84_003's `no_new_privs` is live** before adding `--cap-drop ALL` (else Landlock fails closed on every lease); **live-probe `bwrap --cap-drop`** on the Linux lane.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`**:
  - **RULE UFS** — `--cap-drop`/`ALL` flag literals are single-sourced constants in `sandbox_args.zig`, reused in tests.
  - **RULE NDC / NLR** — §3 is **characterization-only**: it pins existing containment with regression/characterization tests, never a no-op production guard. (The proof-only credential-fd belt split to M84_007.)
  - **RULE NLG** — pre-2.0: no "legacy" framing.
- **`dispatch/write_zig.md`** — tagged-union results, `errdefer`, cross-compile both linux targets.
- **`docs/LOGGING_STANDARD.md`** — any `caps_dropped` emit follows the logfmt envelope.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | **yes** — `*.zig` edits | Read `dispatch/write_zig.md`; cross-compile both linux targets. |
| UFS | **yes** — `--cap-drop`/`ALL` literals | Named constants in `sandbox_args.zig`, reused in tests. |
| LENGTH | **maybe** — `appendBwrap` grows | Keep `appendBwrap` ≤50 lines; extract a cap-emit helper if needed. |
| LOGGING | **maybe** — new audit emit | Envelope unchanged; never log a secret. |
| LIFECYCLE | **no** | cap-drop tightens flags; adds no resource. |

---

## Overview

**Goal (testable):** The sandboxed child holds **`CapEff: 0`** (no Linux capabilities even inside its user namespace); and the existing network/filesystem containment (empty netns under `deny_all`, Landlock fs binds) is pinned by regression tests that stay green — each proven by a negative/regression test on the runner integration lane.

**Problem:** M84_003's launch slice ships the primary sandbox boundary (namespaces + Landlock + cgroup) plus `no_new_privs` and `--new-session`. Two secondary belts were cut from launch and live here: (1) **capabilities** — without `--cap-drop ALL` the child relies entirely on `--unshare-all` to neuter caps namespace-locally; a userns-escape chain would find caps available; (2) **containment** — the network/fs isolation holds today but has no regression pin, so a future mechanism change could silently weaken it. _(A third belt — the credential-fd inheritance proof — split to [`M84_007`](./M84_007_P1_API_RUNNER_CREDENTIAL_FD_PROOF.md), which ships for launch.)_

**Solution summary:** Emit `--cap-drop ALL` (gated on M84_003's `no_new_privs` so Landlock still applies) and add characterization tests pinning the `deny_all` egress + Landlock fs containment. No mechanism changes — depth and proof only.

---

## Prior-Art / Reference Implementations

- **bwrap `--cap-drop ALL`** is the canonical hardened-sandbox capability drop (Flatpak, `bubblewrap(1)`); extend the existing `appendBwrap` flag-emit style.
- The containment tests reuse the **`test-integration-runner`** lane (created in M84_003).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/sandbox_args.zig` | EDIT | `appendBwrap`: emit `--cap-drop ALL` (§1); add the cap-drop flag constants. |
| `src/runner/sandbox_args_edge_test.zig` (+ runner integration tests) | EDIT | Golden-argv `--cap-drop ALL`; Linux-only integration: `CapEff: 0`, network/fs containment. |
| `make/test-integration.mk` | EDIT | Register the new integration tests on the existing `test-integration-runner` lane. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** additive depth + proof; **patch**, behaviour-preserving (the legitimate lease is unchanged). Two independent slices sharing the integration lane.
- **Alternatives considered:** (a) **Ship in the M84_003 launch slice** — rejected at the CEO re-cut: `--cap-drop ALL` is landlock-coupled/risky and the rest is non-launch-blocking proof. (b) **Drop `--cap-drop` entirely** — rejected: it is real defense-in-depth for the untrusted tier this spec gates; keep it, just deferred.

---

## Sections (implementation slices)

### §1 — Capability drop (`--cap-drop ALL`)

`appendBwrap` emits `--cap-drop ALL` so the child holds no capabilities even inside its own user namespace. **Coupling (hard):** this removes the userns `CAP_SYS_ADMIN` that `landlock_restrict_self` relies on today, so M84_003's `no_new_privs` (set in-child before `landlock.applyPolicy`) **must** already be live, or Landlock fails closed on every lease. `--cap-drop` is bwrap-version-gated (~0.4.0+); a build whose bwrap lacks it errors out → lease fails closed.

- **Dimension 1.1** — `appendBwrap` emits `--cap-drop ALL` on sandboxed tiers → Test `test_bwrap_argv_cap_drop_all` (golden argv)
- **Dimension 1.2** — the child holds no capabilities → Test `test_child_has_no_caps` (child reads `/proc/self/status` → `CapEff: 0000000000000000`)

### §2 — File-descriptor hygiene (proof only) → SPLIT to M84_007

> **Split out (Jun 06, 2026).** The credential-fd inheritance proof — prove every daemon fd is `CLOEXEC`, no fd ≥3 reaches the child, the control-plane client holds no persistent socket — is **launch-safe** (proof-only, no new subsystem), so it shipped as its own spec [`M84_007`](./M84_007_P1_API_RUNNER_CREDENTIAL_FD_PROOF.md) rather than waiting behind this spec's untrusted-runner GA deferral. The dimension numbering below is intentionally left starting at §3 to avoid churning the surviving section's references.

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

Contract: the legitimate lease (no_new_privs live, bwrap supports `--cap-drop`) is observably unchanged; only the capability surface is tightened and the containment guarantees are pinned.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| `--cap-drop` unsupported | bwrap older than ~0.4.0 | bwrap errors on the unknown flag → `forkExec` spawn fails → lease fails closed. PLAN live-probes `bwrap --cap-drop`. |
| cap-drop without no_new_privs | M84_003 §1.5 not live | `landlock_restrict_self` fails closed → lease refused. Hard dependency on M84_003; PLAN confirms NNP first. |
| Tool needs a setuid helper | `ping`/`sudo` post-cap-drop | helper fails inside the sandbox — acceptable by design (no escalation); PLAN enumerates engine-invoked tools. |
| Containment regressed | a future mechanism change weakens netns/Landlock | the §3 characterization tests fail; the regression is in the owning spec's mechanism, pinned here. |

---

## Invariants

1. **Child holds no capabilities** — `CapEff: 0` inside `__execute`. Enforced by `test_child_has_no_caps`. (Privilege escalation is already blocked by M84_003's `no_new_privs`.)
2. **Credential-fd inheritance** — *split to [`M84_007`](./M84_007_P1_API_RUNNER_CREDENTIAL_FD_PROOF.md).* No daemon fd (the `zrn_` control-plane socket, cgroup handles) crosses `exec` into the child.
3. **Containment is pinned** — `deny_all` empty netns + Landlock fs isolation stay green; the `registry_allowlist` gap is characterized so it flips when M84_004 lands. Enforced by the §3 tests.
4. **Legitimate path unchanged** — no_new_privs-live + bwrap-supports-cap-drop produces an identical observable outcome; a golden-argv test pins the argv shape.

---

## Test Specification (tiered)

> **Lane:** the Linux-only integration tests run on the **`test-integration-runner`** lane (created in M84_003). `builtin.os.tag == .linux`-gated (`SkipZigTest` on macOS); macOS proof = cross-compile the runner TEST graph for both linux targets.

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `test_bwrap_argv_cap_drop_all` | sandboxed argv contains `--cap-drop ALL` |
| 1.2 | integration-runner | `test_child_has_no_caps` | child `/proc/self/status` → `CapEff: 0000000000000000` |
| 3.1 | integration-runner | `test_sandboxed_child_network_denied` | `deny_all` child `connect()` to external host fails at the kernel |
| 3.2 | integration-runner | `test_sandboxed_child_fs_isolation` | child: write `/etc/<x>` denied; `/home`/`/var`/`/root` absent; workspace write ok |
| 3.3 | integration-runner | `test_registry_allowlist_egress_unrestricted_today` | `registry_allowlist` child reaches an external host (pins the gap; flips when M84_004 lands) |

- **Regression:** `make test-unit-zigrunner` + the M84_003 launch-slice tests stay green; the legitimate sandboxed lease still runs end-to-end.

---

## Acceptance Criteria

- [ ] `--cap-drop ALL` on sandboxed argv; child `CapEff: 0` — verify: `test_bwrap_argv_cap_drop_all` + `test_child_has_no_caps`
- [ ] Containment pinned: `deny_all` network denied; host fs isolated; `registry_allowlist` gap characterized — verify: `test_sandboxed_child_network_denied` + `test_sandboxed_child_fs_isolation` + `test_registry_allowlist_egress_unrestricted_today`
- [ ] M84_003 `no_new_privs` confirmed live before cap-drop; `bwrap --cap-drop` probed on the Linux lane — verify: PLAN note in Discovery
- [ ] `make lint` clean · `make test-integration-runner` passes · cross-compile both linux targets
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: cap-drop present (unit golden argv)
zig build --build-file build_runner.zig test 2>&1 | grep -E "cap_drop_all"
# E2: integration lane (caps + net/fs containment)
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
- **§2 split (Jun 06, 2026):** the credential-fd inheritance proof shipped early as its own launch-safe spec [`M84_007`](./M84_007_P1_API_RUNNER_CREDENTIAL_FD_PROOF.md) (proof-only, no new subsystem), leaving this spec with only §1 cap-drop + §3 containment, both still deferred behind untrusted-runner GA. Indy decision (verbatim, Jun 06, 2026): _"do"_ (defer the egress proxy + cap-drop/containment; land the credential-fd proof now).
- **Code-grounded facts (from the M84_003 reviews):** `landlock.zig` sets **no** `prctl` (rides userns `CAP_SYS_ADMIN`) → cap-drop without no_new_privs fail-closes every lease (hard dependency on M84_003 §1.5). `registry_allowlist` is full host egress until M84_004. (The fd-vector facts moved with §2 to M84_007.)
- **PLAN decisions to bank:** confirm M84_003 `no_new_privs` is live; `bwrap --cap-drop` live-probe; enumerate engine-invoked tools needing a setuid helper.
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
- **Credential-fd inheritance proof** — split to [`M84_007`](./M84_007_P1_API_RUNNER_CREDENTIAL_FD_PROOF.md) (launch); no longer in this spec.
- **Namespace / Landlock / cgroup mechanism changes** — owned by the M80 runner-fleet specs; this spec hardens only the capability surface and pins containment.
