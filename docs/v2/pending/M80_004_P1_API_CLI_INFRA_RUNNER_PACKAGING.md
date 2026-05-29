# M80_004: Ship zombie-runner as a deployable product — macOS sandbox, distribution, operator CLI

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 004
**Date:** May 27, 2026
**Status:** PENDING
**Priority:** P1 — without a macOS sandbox, a build/ship pipeline, and an operator CLI, the runner binary exists but cannot be deployed or operated.
**Categories:** API, CLI, INFRA
**Batch:** B1
**Branch:** {feat/mNN-name — added when work begins}
**Depends on:** M80_002 (the runner binary + engine fold-in this packages), M80_001 (the frozen contract the CLI speaks)
**Provenance:** agent-generated (Opus 4.7, May 27, 2026 — from the `runner_fleet.md` S3 roadmap row, remaining-scope after M80_002 absorbed the binary + engine)

> **Provenance is load-bearing.** LLM-drafted from the roadmap. The implementing agent cross-checks every claim against `src/runner/` — the binary and Linux sandbox already exist; this workstream is the macOS backend, the pipeline, and the CLI that M80_002 did NOT ship.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (S3 row + the fork-sandboxed-child model) — the sandbox-tier and distribution shape live there.

---

## Implementing agent — read these first

1. `src/runner/child_supervisor.zig` + `src/runner/sandbox_args.zig` — the Linux fork + bubblewrap model and the `establishSandbox` fail-closed contract (Invariant 7) the macOS backend must mirror tier-for-tier.
2. `src/runner/engine/landlock.zig` — the Linux in-child mandatory policy; the macOS Seatbelt backend is its counterpart (apply-or-refuse before the agent runs).
3. `docs/CLI_DX_PILLARS.md` — the 7 Pillars the operator CLI obeys: command→handler→renderer split, handler purity, output-as-a-service, structured errors, auto-JSON when piped. The CLI mirrors `zombiectl`'s endpoint flag, not a bespoke `--mothership`.
4. `deploy.sh` + the `zombie-runner.service` unit (migrated in M80_002 `63670d09`) — the host install surface the distribution pipeline targets.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** ship zombie-runner: macOS Seatbelt backend + distribution pipeline + operator CLI
- **Intent (one sentence):** make the runner deployable and operable on real hosts — a working sandbox on macOS, a reproducible build/ship pipeline, and a CLI an operator drives.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent and list `ASSUMPTIONS I'M MAKING: …`. Mismatch with Intent → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (no dead sandbox stub left when the real backend lands), UFS (sandbox-tier values single-sourced off the enum; CLI flag names shared verbatim with `zombiectl`).
- **`docs/ZIG_RULES.md`** — the macOS backend is `*.zig` (tagged-union results, multi-step `errdefer`, cross-compile both targets; `*.zig` ≤350 lines / fn ≤50).
- **`docs/CLI_DX_PILLARS.md`** — the runner CLI surface: handler purity, renderer-owned output, structured JSON errors.
- **`docs/LIFECYCLE_PATTERNS.md`** — sandbox setup/teardown is a lifecycle (acquire-or-fail-closed, idempotent destroy).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — macOS backend + CLI subcommands are `*.zig` | cross-compile x86_64 + aarch64 (Linux unaffected); read ZIG_RULES |
| PUB / Struct-Shape | yes — a new `seatbelt` module with a `pub fn applyProfile` | shape verdict per surface; mirror `landlock.zig`'s pub shape |
| File & Function Length | yes | the seatbelt profile builder splits like `sandbox_args.zig`'s `appendBwrap` if it nears the cap |
| UFS | yes — tier names + CLI flag spellings | derive tiers from `contract.protocol.SandboxTier`; share the endpoint-flag constant with `zombiectl` |
| LIFECYCLE | yes — sandbox apply/destroy | fail-closed apply; idempotent teardown; mirror `establishSandbox` |
| LOGGING | yes — sandbox + CLI emits | logfmt with `error_code` on fail-closed; no secret in logs |
| ERROR REGISTRY | yes — a macOS-sandbox-unavailable code | reuse/extend `UZ-RUN-*` via the registry + `client_errors.zig` mirror |

---

## Overview

**Goal (testable):** on macOS, a lease with the `macos_seatbelt` tier runs its agent child inside a Seatbelt profile that denies network and confines filesystem writes to the lease workspace, and refuses the lease (fail-closed, `agent_error`) if the profile cannot be applied — asserted by `test_seatbelt_confines_workspace_and_denies_net` (Linux host runs it as skip).

**Problem:** the runner builds and runs on Linux (bubblewrap + Landlock + cgroups), but on macOS the sandbox is a no-op that fails closed, there is no pipeline to build and ship the binary to hosts, and operators have no CLI to enroll/inspect a runner. The runner is built but not shippable.

**Solution summary:** add a macOS Seatbelt backend behind the existing `SandboxTier` seam (a real `macos_seatbelt` enforcement path, fail-closed like Landlock); add a distribution pipeline that cross-builds both binaries for both arches and packages them for the `zombie-runner.service` host install; add operator subcommands to the runner binary (enroll, status, doctor) that speak the frozen contract over the same endpoint flag `zombiectl` uses.

---

## Prior-Art / Reference Implementations

- **API (sandbox)** → `src/runner/engine/landlock.zig` + `child_supervisor.establishSandbox` — the Linux fail-closed pattern; the Seatbelt backend mirrors its shape (apply-before-agent, refuse-on-failure).
- **CLI** → `docs/CLI_DX_PILLARS.md` + `zombiectl/src/` command/handler/renderer split; the runner CLI reuses `zombiectl`'s endpoint-flag convention.
- **INFRA** → `deploy.sh` + `release.yml`/`deploy-dev.yml` (M80_002 `63670d09` migrated these to `zombie-runner`); the pipeline extends them, not a new system.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/engine/seatbelt.zig` | CREATE | macOS Seatbelt profile builder + `applyProfile` (counterpart to `landlock.zig`) |
| `src/runner/child_supervisor.zig` | EDIT | `establishSandbox` gains the macOS branch (Seatbelt) instead of `error.SandboxUnavailable` |
| `src/runner/sandbox_args.zig` | EDIT | macOS child argv (sandbox-exec wrapper) alongside the bubblewrap branch |
| `src/runner/cmd/*.zig` | CREATE | operator subcommands (enroll / status / doctor) on the runner binary |
| `.github/workflows/release.yml`, `deploy.sh` | EDIT | cross-build + package both binaries both arches; ship to the runner host |
| `src/zombied/errors/error_entries.zig`, `error_registry.zig` | EDIT | a macOS-sandbox-unavailable `UZ-RUN-*` code if a distinct one is warranted |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three independent slices (sandbox / pipeline / CLI) behind the seams M80_002 already built, so each lands without touching the others.
- **Alternatives considered:** (a) fold the macOS backend into M80_002 — rejected, M80_002 was already one large PR and macOS sandboxing is a distinct, Linux-independent surface; (b) a separate Node CLI for the runner — rejected, it duplicates `zombiectl`'s contract client and breaks the single-endpoint-flag convention.
- **Patch-vs-refactor verdict:** **additive** — new backend behind an existing seam (`SandboxTier`), new subcommands, pipeline extension. No refactor of M80_002's code.

---

## Sections (implementation slices)

### §1 — macOS Seatbelt sandbox backend

Delivers a real `macos_seatbelt` enforcement tier so a Mac host can run leases sandboxed, not just fail closed. Mirrors the Landlock contract: the profile is applied in the child before the agent runs; an unappliable profile refuses the lease. **Implementation default:** drive the profile via the macOS sandbox facility the codebase already targets for the tier; deny network, allow read of system paths, confine writes to the lease workspace — because that matches the bubblewrap policy on Linux (parity across tiers).

- **Dimension 1.1** — on macOS, `macos_seatbelt` confines child writes to the lease workspace and denies network → Test `test_seatbelt_confines_workspace_and_denies_net`
- **Dimension 1.2** — a Seatbelt profile that cannot be applied refuses the lease (fail-closed, `agent_error`, sandbox `UZ-RUN-*`), never runs the agent unsandboxed → Test `test_seatbelt_unavailable_fails_closed`
- **Dimension 1.3** — `dev_none` on macOS still runs bare (dev only); the prod `dev_none` startup guard (M80_002) is unchanged → Test `test_macos_dev_none_unchanged`

### §2 — Distribution & CI pipeline

Delivers a reproducible build that cross-compiles both binaries for both arches and packages `zombie-runner` for the host install the `zombie-runner.service` unit expects. Why: a runner that can't be built-and-shipped reproducibly can't be operated.

- **Dimension 2.1** — CI cross-builds `zombie-runner` for x86_64-linux + aarch64-linux and publishes the artifact → Test `test_release_pipeline_emits_both_arch_runner_artifacts` (CI assertion)
- **Dimension 2.2** — `deploy.sh` installs the arch-correct binary + `/etc/default/zombie-runner` env and (re)starts the unit idempotently → Test `test_deploy_install_is_idempotent`

### §3 — Operator runner CLI

Delivers operator subcommands on the runner binary (enroll / status / doctor) that speak the frozen contract over the same endpoint flag `zombiectl` uses. Why: operators need to enroll and inspect a host without hand-crafting HTTP.

> **Option B reconciliation (M80_005).** `enroll` is an **operator** action, not the daemon self-registering. The host daemon never calls `POST /v1/runners` on boot — it reads a pre-minted `zrn_` from `ZOMBIE_RUNNER_TOKEN` and goes straight to the lease loop (M80_005 §3). `zombie-runner enroll` is the operator-run convenience that calls `POST /v1/runners` (now gated by the `platform_admin` claim — M80_005 §2) and writes the env file; it must authenticate with a platform-admin Clerk JWT, and a tenant `admin`/`zmb_t_` caller gets `403`. See `docs/AUTH.md` (Runner token → Provisioning).

- **Dimension 3.1** — `zombie-runner status` reports registration + current lease state as human text, and as JSON when stdout is piped → Test `test_runner_cli_status_human_and_json`
- **Dimension 3.2** — `zombie-runner enroll` (operator-run, platform-admin Clerk JWT) mints/stores the runner token via the contract and writes the env file; a non-platform-admin caller gets `403` and a transport failure returns a structured error with a `suggestion` → Test `test_runner_cli_enroll_structured_error`

---

## Interfaces

```
zombie-runner status   [--endpoint URL] [--json]   → registration + lease state
zombie-runner enroll   --endpoint URL              → operator-run; POST /v1/runners (platform_admin JWT, mints zrn_), writes /etc/default/zombie-runner. NOT called by the daemon on boot (Option B)
zombie-runner doctor   [--endpoint URL]            → preflight: env present, control plane reachable, sandbox tier appliable

Seatbelt backend (internal):
  seatbelt.applyProfile(workspace: []const u8) !void   — mirror of landlock.applyPolicy; errors → fail closed
Endpoint flag: the SAME constant zombiectl uses (UFS — shared verbatim), not --mothership.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Seatbelt profile unappliable | macOS sandbox facility denies / errors | lease refused unrun, `agent_error` + sandbox `UZ-RUN-*`; no unsandboxed execution (Invariant 7) |
| Cross-build fails one arch | toolchain/target regression | release pipeline fails loud; no partial artifact published |
| Deploy on a re-run | unit already installed/running | install is idempotent — same binary/env → no-op; changed → restart, no duplicate unit |
| CLI control-plane unreachable | zombied down / bad endpoint | structured JSON error with `suggestion`/`retry`; non-zero exit; no stack dump |
| CLI run with stdout piped | LLM/script consumer | auto-JSON (Pillar) — never human text into a pipe |

---

## Invariants

1. No tier but `dev_none` ever runs the agent without an established sandbox — enforced by `establishSandbox` returning an error on the macOS path too (compiler + `test_seatbelt_unavailable_fails_closed`), identical to the Linux contract.
2. The runner CLI shares the endpoint-flag identifier with `zombiectl` verbatim — enforced by UFS (a single shared constant; a divergent literal trips the gate).
3. Handlers contain no `process.exit`/`console.log`-equivalent direct I/O — enforced by the CLI-pillars handler-purity check.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_seatbelt_confines_workspace_and_denies_net` | a child writing outside the workspace or opening a socket fails; inside-workspace write succeeds (macOS host; Linux skips) |
| 1.2 | unit | `test_seatbelt_unavailable_fails_closed` | profile-apply error → `establishSandbox` errors → result `agent_error`, failure `startup_posture` |
| 1.3 | unit | `test_macos_dev_none_unchanged` | `dev_none` on macOS returns no scope; release-build guard still rejects `dev_none` |
| 2.1 | e2e | `test_release_pipeline_emits_both_arch_runner_artifacts` | CI job produces x86_64 + aarch64 runner artifacts |
| 2.2 | integration | `test_deploy_install_is_idempotent` | second `deploy.sh` run with same inputs → no change; changed binary → restart |
| 3.1 | e2e | `test_runner_cli_status_human_and_json` | `status` piped → JSON; tty → human; both report the same state |
| 3.2 | e2e | `test_runner_cli_enroll_structured_error` | unreachable endpoint → JSON error with `suggestion`, non-zero exit |

Regression: the Linux sandbox path (bubblewrap/Landlock/cgroups) is unchanged — M80_002's sandbox tests must stay green. Idempotency: `deploy.sh` re-run (2.2).

---

## Acceptance Criteria

- [ ] macOS lease runs sandboxed or fails closed — verify: `test_seatbelt_confines_workspace_and_denies_net` + `test_seatbelt_unavailable_fails_closed`
- [ ] Both binaries cross-compile both arches — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` and the same for `build_runner.zig`
- [ ] `make lint` clean · `make test` passes · `make test-integration` passes
- [ ] Runner CLI auto-JSON when piped — verify: `zombie-runner status --endpoint … | jq .`
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: macOS sandbox fail-closed unit — zig build --build-file build_runner.zig test -Dtest-filter="seatbelt" && echo PASS || echo FAIL
# E2: Build  — zig build && zig build --build-file build_runner.zig
# E3: Tests  — make test && make test-integration
# E4: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build --build-file build_runner.zig -Dtarget=x86_64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

N/A — no files deleted. The current macOS branch in `establishSandbox` (`error.SandboxUnavailable`) is replaced in-place by the Seatbelt path (RULE NLR touch-it-fix-it), not left as a dead stub.

---

## Discovery (consult log)

> **Empty at creation.** Append consults, skill-chain outcomes, and Indy-acked deferral quotes as work proceeds.

- **Provenance (May 27, 2026):** authored alongside M80_003/005/006 at Indy's request to formalize the remaining runner-rollout roadmap as specs before M80_002's CHORE(close).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | audits coverage vs this Test Specification (esp. the macOS skip arms + CLI e2e) | clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | adversarial review vs Invariant 7, ZIG_RULES, CLI pillars | clean OR every finding dispositioned |
| After `gh pr create` | `/review-pr` | review-comments the open PR | comments addressed before merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | {paste at VERIFY} | |
| Integration | `make test-integration` | {paste at VERIFY} | |
| Runner CLI e2e | `zombie-runner status --json` | {paste at VERIFY} | |
| Lint | `make lint` | {paste at VERIFY} | |
| Cross-compile | `zig build --build-file build_runner.zig -Dtarget=aarch64-linux` | {paste at VERIFY} | |

---

## Out of Scope

- Container-nested tier hardening — tracked separately if the fleet needs it.
- Operator-assigned trust + workspace authz — M80_005.
- Fleet inventory / heartbeat reassignment / lease renewal — M80_006.
- Placement scheduler / autoscale — M80_007.
