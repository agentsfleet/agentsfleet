<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M220_001: The kernel-lane machine runs Debian 13, and the toolbox docs name only what the image holds

**Prototype:** v2.0.0
**Milestone:** M220
**Workstream:** 001
**Date:** Oct 10, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — developer tooling and doc accuracy; no product behaviour changes
**Categories:** DOCS, INFRA
**Batch:** B1 — one machine setup and documentation corrections, independent of each other
**Branch:** `build/m220-debian-kernel-vm`
**Baseline revision:** `6e4a02127b4738f205c6ba1c8d42bce42413ed2e`
**Test Baseline:** n/a — user-authorized `spec.baseline` override to conserve shared machine resources; the earlier 51-test kernel results are the measured comparison for new caller tests
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M220_001-6e4a02127b4738f205c6ba1c8d42bce42413ed2e.md`
**Depends on:** `M219_002` §6 — the toolbox already builds from the Debian 13.7 snapshot `20261010T000000Z`
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 10, 2026) from Indy's Oct 10, 2026 choice recorded in `M219_002` Discovery
**Canonical architecture:** `docs/architecture/runner_execution.md` §Toolbox

---

## Overview

**Goal (testable):** `orb -m afr-kernel cat /etc/debian_version` prints `13.` followed by a point release, `make test-runner-kernel` passes there through the real supervisor and bubblewrap callers, and the toolbox paragraph and its release-record sentence name only what the toolbox manifest installs.
**Problem:** The original kernel spike recorded Ubuntu 24.04.5, while production runner hosts run Debian 13. OrbStack now reports the named machine absent. Rebuilding it on Debian removes that host-userland difference. Three docs also disagree with the code: the toolbox page lists Chromium, Codex and Claude Code, which the image does not hold; a Continuous Integration (CI) comment calls the build debootstrap; and a Cargo comment names `debian:bookworm-slim` where the daemon image is distroless `static-debian12`.
**Solution summary:** Recreate the OrbStack machine `afr-kernel` from the `debian:trixie` image with the packages the kernel lane needs, and write its setup as a short doc so the machine can be rebuilt by anyone. OrbStack's own kernel stays, so the sandbox's kernel features do not change. Correct the three doc lines to what the code does today. Extend the existing kernel lane through the shipping supervisor, agent loop and tools, with reused fake external endpoints.

## PR Intent & comprehension handshake

- **PR title (eventual):** build: validate Debian kernel setup through real runner callers
- **Intent (one sentence):** A developer can rebuild the Debian kernel machine and prove user work through the real runner callers, while the toolbox page names the image's actual contents.
- **Handshake:** Run caller-to-sandbox kernel tests on Debian 13 and make the setup guide and toolbox descriptions match verified inputs.
- **ASSUMPTIONS I'M MAKING:** Keep `afr-kernel`; create it because OrbStack reports it absent. Validate the guide on a second fresh machine. Extra release and external documentation are waived by Indy's session override; this spec's documentation remains in scope.

## Implementing agent — read these first

1. `docs/v2/reviews/m211-toolbox-spikes.md` — records the original machine (Ubuntu 24.04.5 arm64, kernel 7.0.14-orbstack, tool versions) and what the kernel lane needs from it.
2. `scripts/toolbox/install-builder-tools.sh` — the builder packages CI installs; the machine needs the same, plus Rust and bubblewrap.
3. `make/test-unit.mk` — `test-runner-kernel` and `KERNEL_LANE_RUNNER`: what the lane runs as root on the machine.
4. `scripts/toolbox/manifest.txt` — the toolbox's real package list, which the toolbox doc must match.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/development.md` | EDIT | How to create the `afr-kernel` machine on Debian 13 and run the kernel lane there |
| `docs/v2/reviews/m211-toolbox-spikes.md` | EDIT | The machine row notes the move to Debian 13 and points at the setup doc |
| `docs/architecture/runner_execution.md` | EDIT | The toolbox paragraph names the manifest's tools only |
| `.github/workflows/test-integration-rustd.yml` | EDIT | The timeout comment says mmdebstrap; a CI file, so Indy approves the edit at EXECUTE |
| `rustd/Cargo.toml` | EDIT | The `tzdb-zoneinfo` comment names the image the daemon actually ships in |
| `rustd/crates/afr_sandbox/Cargo.toml` | EDIT | Development dependencies connect the existing kernel example to the real agent and supervisor |
| `rustd/Cargo.lock` | EDIT | Record those existing workspace dependencies for the kernel example |
| `rustd/crates/afr_sandbox/examples/kernel_lane/{main,trials,git}.rs` | EDIT | Register caller tests and reuse the existing repository fixture |
| `rustd/crates/afr_sandbox/examples/kernel_lane/supervisor{,_checks,_model,_plane,_trials}.rs` | CREATE | Drive the real supervisor, agent loop, catalog and bubblewrap with fake external edges |
| `rustd/crates/agentsfleet_runner/tests/{fake_daemon,runner_suite}.rs` | EDIT | Reuse the existing daemon and collector listener through shared test support |
| `rustd/crates/agentsfleet_runner/tests/http_server.rs` | CREATE | Extract the existing loopback listener with bounded shutdown and cleanup on drop |
| `rustd/crates/agentsfleetd/tests/support/https.rs` | EDIT | Retain and reap the existing upstream listener and its connections for kernel reuse |
| `docs/v2/{pending,active,done}/M220_001_P2_DOCS_INFRA_KERNEL_VM_RUNS_DEBIAN_TRIXIE.md` | EDIT | This spec, moved by CHORE(open) and CHORE(close) |
| `playbooks/operations/acceptance/baselines/M220_001-<revision>.md` | CREATE | The baseline evidence the header promises, named for the comparison revision |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — NLR (a touched stale line is fixed, not worked around); UFS does not fire on Markdown.
- `.orly/docs/DOCUMENTATION_RULES.md` — the architecture page and the development doc are published prose: sentence length and plain wording apply.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| CI/CD edit (`.github/workflows/**`) | yes — one comment line | Approved by Indy's session authorization to override orly; the edit changes no workflow step |
| File & Function Length (≤350/≤50/≤70) | yes | Split the kernel test fixture by model, control plane, execution and assertions |

## Prior-Art / Reference Implementations

- **Reference:** `scripts/toolbox/install-builder-tools.sh` and `playbooks/lib/runner/` — the package set and host preparation production runner hosts already use on Debian 13; the setup doc mirrors them rather than inventing a list.
- **Caller-test references:** `agentsfleetd/tests/support/fake_model.rs`, `agentsfleetd/tests/support/bundle_run.rs`, and the existing kernel Git trials. Use the real `afr_supervisor::run_fetching_from`, `afr_agent::Loop`, tool catalog and `BubblewrapEngine`. Microsoft Rust guidelines `M-TEST-UTIL`, `M-TAUTOLOGICAL-TESTS` and `M-FROM-ERROR` apply.

## Sections (implementation slices)

### §1 — The kernel-lane machine runs Debian 13 — DONE

`afr-kernel` is created as `orb create debian:trixie afr-kernel`, then given the builder tools, bubblewrap, Rust at the workspace's pinned toolchain, and passwordless sudo for the lane's `sudo -E`. OrbStack reports no existing machine, so no deletion is needed. **Implementation default:** keep the name `afr-kernel` because the session learnings and docs already call it that.

- **Dimension 1.1** — DONE — The machine reports Debian 13 → Test `kernel_vm_is_debian_trixie` (manual: `orb -m afr-kernel cat /etc/debian_version` prints `13.`; observed `13.7`)
- **Dimension 1.2** — DONE — The kernel lane passes on it → Test `test_toolbox_carries_the_tools` with the rest of `make test-runner-kernel` (Debian 13.7, current toolbox and final assertions: 56 passed; 0 failed; 0 ignored; exit 0, capture `025`; all five caller cases also passed in capture `024`)

### §2 — The setup is written down — DONE

`docs/development.md` gains the commands that create the machine and run the kernel lane on it, so it can be rebuilt by anyone who has OrbStack.

- **Dimension 2.1** — DONE — The doc's commands recreate a machine that passes 1.1 and 1.2 → Test `kernel_vm_setup_doc_rebuilds_the_machine` (`afr-kernel-m220-rebuild`: Debian 13.7, pinned Rust 1.99.0; 51 passed; 0 failed; 0 ignored; exit 0)

### §3 — The docs name what the code does — DONE

The toolbox paragraph lists the manifest's packages and uv with uvx. The release record pins their shared upstream uv archive, the only vendored input. The CI comment says mmdebstrap. The Cargo comment names the daemon's real base image, after confirming that image ships `/usr/share/zoneinfo`.

- **Dimension 3.1** — DONE — The toolbox paragraph and release-record sentence name only the manifest's tools → Test `toolbox_doc_matches_manifest` (grep, rubric R2)
- **Dimension 3.2** — DONE — No build comment says debootstrap → Test `no_debootstrap_mention` (grep, rubric R3)
- **Dimension 3.3** — DONE — No comment names `debian:bookworm-slim` → Test `no_bookworm_slim_image` (grep, rubric R4)

### §4 — User work reaches the real sandbox — DONE

Indy requested proof from the bubblewrap caller. The new kernel tests start with a lease representing a user's steer, then use the production supervisor and agent loop. Fake model and daemon endpoints are external test edges; this lane does not claim to prove daemon persistence or Slack delivery. Existing service integration tests cover those boundaries with live Postgres and Dragonfly.

- **Dimension 4.1** — DONE — A steer and lease instructions reach the model, which edits and commits the supervisor's checkout through bubblewrap → Test `test_supervisor_steer_edits_and_commits_inside_bubblewrap`; steer admission and `SKILL.md` loading are outside this proof
- **Dimension 4.2** — DONE — Memory, fleet schedules, thread messages and guarded Grafana reads use the supervisor without preparing a sandbox → Test `test_supervisor_only_tools_need_no_bubblewrap`; typed schedule creation and run-now replies, the exact fenced run request and returned event identifier are asserted
- **Dimension 4.3** — DONE — Remote Git, prohibited HTTP writes and cross-origin credentials are refused with no remote effects → Test `test_supervisor_refuses_remote_git_and_unsafe_egress`
- **Dimension 4.4** — DONE — A failed schedule or thread request is not retried, and the refusal reaches the model → Test `test_supervisor_does_not_repeat_failed_mutations`
- **Dimension 4.5** — DONE — Shutdown during a running shell ends the child and removes the sandbox's state → Test `test_supervisor_shutdown_reaps_running_bubblewrap`

All five passed in the complete 56-test Make lane and in the final seed-220
replay, one isolated native process per case. The latter includes the final
checks for all known fixture credentials and the redaction marker in child
output; each case passed once, with zero failures.

Required-test ledger, written before implementation:

| Changed unit or boundary | Required proof | Status |
|---|---|---|
| Kernel trial registration and development dependency wiring | Complete kernel lane builds and runs every new trial | passed — `025`, 56 tests |
| Supervisor lease → prompt → catalog → engine → executor | Real sandbox, lease instructions and steer observed; file read and local commit verified | passed — §4.1, `024` and `025` |
| Memory hydrate → recall → capture → report | Fenced requests, captured memory and final report observed in order | passed — §4.2, `024` and `025` |
| Schedule and message mutations | Typed creation/run results; exact fenced run request; success and failed response; one attempt per call | passed — §4.2 and §4.4, `024` and `025` |
| Guarded Grafana read | Host-bound credential filled at HTTPS transport and absent from the model; child output excludes all three fixture credentials and the redaction marker | passed — §4.1 and §4.2, `024` and `025` |
| Remote Git and prohibited HTTP paths | Specific refusals; no origin change or outbound request | passed — §4.3, `024` and `025` |
| Shutdown and error cleanup | Runner and fake servers joined; child processes, sandbox directory and cgroup removed | passed — §4.4 and §4.5, `024` and `025` |
| No production Rust or TypeScript change | Changed-production Rust floor remains 99%; canonical TypeScript coverage remains 100% | pending verification |

Won't-test decisions: allocation failure, failed loopback binding, invalid
hard-coded fixture JSON and cleanup-deadline panic branches are setup failures
in test-only support. They change no production path. The new tests cover the
observable failures that matter here: rejected effects, failed mutations and
shutdown during a running child.

## Interfaces

```
No interface changes: no route, command, flag or wire shape moves.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Kernel feature missing | OrbStack's kernel lacks a feature the lane needs on the new image | Unchanged kernel, so not expected; the lane fails loudly (it never skips) and the old machine's facts in the spike record show the expected set |
| Builder versions differ | Debian's mmdebstrap or dpkg differ from Ubuntu's | The local image's release record differs from CI's; CI stays the build of record, and the doc says so |
| Zoneinfo absent from the real image | The daemon's base image ships no `/usr/share/zoneinfo` | The Cargo comment is corrected to what is true, and a missing database is raised with Indy as a finding, not hidden |

## Invariants

Production behaviour is unchanged. Caller tests preserve sandbox confinement, host-only credentials, fenced supervisor requests, one attempt for non-idempotent mutations, and complete cleanup.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | not applicable | not applicable | not applicable | `not_applicable` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | manual | `kernel_vm_is_debian_trixie` | `orb -m afr-kernel cat /etc/debian_version` → starts with `13.`; the developer pastes the line into PR Session Notes |
| 1.2 | integration | `test_toolbox_carries_the_tools` | `make test-runner-kernel` on the Debian machine → every kernel-lane test passes |
| 2.1 | manual | `kernel_vm_setup_doc_rebuilds_the_machine` | The doc's commands on a fresh machine name → 1.1 and 1.2 hold; the developer records the machine name and lane result in PR Session Notes |
| 3.1 | unit | `toolbox_doc_matches_manifest` | `git grep -n -e chromium-headless-shell -e 'Codex and Claude Code command-line tools' -e 'uv, the coding-engine command-line tools' -- docs/architecture/runner_execution.md` → no output (today: lines 164 and 166) |
| 3.2 | unit | `no_debootstrap_mention` | `git grep -n debootstrap -- .github rustd scripts make` → no output |
| 3.3 | unit | `no_bookworm_slim_image` | `git grep -n 'bookworm-slim' -- rustd/Cargo.toml` → no output |
| 4.1 | integration | `test_supervisor_steer_edits_and_commits_inside_bubblewrap` | A steer-derived lease and scripted calls → real bubblewrap; confined child, workspace edit, file read and local commit observed |
| 4.2 | integration | `test_supervisor_only_tools_need_no_bubblewrap` | Memory, schedule, message and Grafana calls → zero engine preparations; fenced requests, memory capture before report, authenticated HTTPS read |
| 4.3 | integration | `test_supervisor_refuses_remote_git_and_unsafe_egress` | Git push, prohibited POST, wrong-origin credential and malformed arguments → exact tool refusals; no upstream request or origin change |
| 4.4 | integration | `test_supervisor_does_not_repeat_failed_mutations` | A fake daemon returns 503 to schedule and message → one attempt each; unreachable-tool result reaches the model |
| 4.5 | integration | `test_supervisor_shutdown_reaps_running_bubblewrap` | Stop the real supervisor after the shell's running marker → child processes reaped; lease directory and cgroup removed |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The kernel lane runs on Debian 13 (§1) | `orb -m afr-kernel cat /etc/debian_version` | starts with `13.` | P1 | ✅ `13.7`, capture `029`; final kernel result in Dimension 1.2 |
| R2 | The toolbox doc names only the image's tools (§3) | `git grep -n -e chromium-headless-shell -e 'Codex and Claude Code command-line tools' -e 'uv, the coding-engine command-line tools' -- docs/architecture/runner_execution.md` | no output | P1 | ✅ no matches, exit 1, capture `026` |
| R3 | No build comment says debootstrap (§3) | `git grep -n debootstrap -- .github rustd scripts make` | no output | P2 | ✅ no matches, exit 1, capture `027` |
| R4 | No comment names `debian:bookworm-slim` (§3) | `git grep -n 'bookworm-slim' -- rustd/Cargo.toml` | no output | P2 | ✅ no matches, exit 1, capture `028` |
| R5 | This spec's diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint passes | `make lint-all` | exit 0 | P0 | |
| S4 | Integration passes | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run each Verify command verbatim; Graded = ✅/❌ plus one decisive output line. Repository-command rows point at the final `orly gate pr` results in PR Session Notes.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Self-hosted Debian runners for CI: GitHub offers no hosted Debian, and a runner fleet is upkeep Indy has not asked for; CI stays on its Ubuntu runners and remains the toolbox's build of record.
- Moving the toolbox snapshot again: `M219_002` §6 owns the pin.
- Adding Chromium, Codex or Claude Code to the toolbox: a product decision, not a doc fix.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A developer runs the kernel lane locally on Debian 13 and sees the same result a production host would give.
2. **Preserved user behaviour** — The kernel lane's commands, the toolbox image and every runtime path stay as they are.
3. **Optimal-way check** — Matching the host distribution is the direct fix; the remaining gap is CI's Ubuntu runners, which this spec leaves because GitHub hosts no Debian.
4. **Rebuild-vs-iterate** — Iterate: one machine recreated, documentation corrected, and the existing kernel lane extended through real caller wiring.
5. **What we build** — A Debian 13 `afr-kernel`, its setup doc, corrected toolbox facts and caller-to-sandbox tests reusing existing fakes.
6. **What we do NOT build** — CI runner changes, toolbox package changes, a Git publication feature, or production behaviour changes.
7. **Fit with existing features** — Compounds with `M219_002` §6's Debian 13.7 toolbox; must not destabilize the kernel lane.
8. **Surface order** — N/A — no user surface; a developer machine and docs.
9. **Dashboard restraint** — N/A — no dashboard change.
10. **Confused-user next step** — The setup doc in `docs/development.md` is the self-serve rebuild.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One Section per deliverable — the machine, its doc, the drift lines — so each proves alone.
- **Alternatives considered:** Running the kernel lane in a Debian container on the Ubuntu machine (rejected: the lane needs a full init, cgroup delegation and loop devices a container hides); self-hosted Debian CI runners (rejected: upkeep with no asked-for gain).
- **Patch-vs-refactor verdict:** this is a **patch** because the lane, the image and the docs are right in shape; only the machine's distribution and three lines drifted.

## Discovery (consult log)

- **Consults** — Oct 10, 2026: Indy asked "can that be changed to use the debian latest 13?", then "Debian 13.7". The toolbox already built on trixie; the local machine and three docs did not match. `M219_002` §6 moved the snapshot; this spec carries the rest.
- **Metrics review** — No analytics or funnel playbook update required: no product or operator signal changes.
- **PLAN:** Existing builder and kernel targets supply the workflow; no new script or Make target is needed. Quality ceiling: a larger refactor adds no runtime or concurrency benefit to this machine and prose correction.
- **Surface check:** OpenAPI, product command-line interface, schema and product behaviour: unchanged. Contributor docs: setup and accuracy fixes. Release/version and external docs: waived for this internal work by Indy.
- **Discovery:** `orb -m afr-kernel cat /etc/os-release` reports `machine not found`. `scripts/toolbox/manifest.txt` installs both uv and uvx from one archive, so the spec now names both binaries.
- **Setup regression:** The first kernel command failed with permission denied because the root toolbox build created the shared cache parent. The guide now creates Cargo's directory as the normal user before that build. The second fresh machine checks this corrected ordering.
- **Host-tool regression:** The first complete lane passed 49 of 51 tests; the two Git trials failed with `No such file or directory`. Both call `host_git` to create their fixtures, and the fresh machine had no host Git. The guide now installs `git` alongside the other host tools; the sandbox still uses the manifest's Git.
- **Disk reclamation:** Concurrent builds and the kernel lane's temporary disks reduced Mac headroom to 837 MiB. `orb -m afr-kernel sudo fstrim -v /` returned freed Linux blocks to the Mac; `df -h /System/Volumes/Data` then reported 11 GiB available. The guide records this optional cleanup. This trim removed no other agent's processes or build directories.
- **Fresh-machine proof:** The corrected guide passed all 51 kernel tests on `afr-kernel` and independently on `afr-kernel-m220-rebuild`. The fresh machine needed no cache ownership repair. Its evidence was retained before the disposable machine was deleted; `afr-kernel` remains configured for Indy.
- **Shared-resource cleanup:** Later caller builds reclaimed only generated caches after checking ownership and active processes: npm's cache, an idle M213 incremental directory and this worktree's completed native Cargo target. The final kernel executable and toolbox were retained. No active build, another agent's process, source or installed dependency was removed. Cargo builds use one job, disabled incremental output and zero debug information.
- **Current-toolbox proof:** After merging `origin/main` at `5cda557a00112de60686680f3bea218e00ad2dbd`, the toolbox excludes the GitHub command-line tool. The final image and assertions passed all 56 kernel tests (`025`); all five caller cases also passed in seeded isolated-process order (`024`). Linux Clippy passed with warnings denied (`023`). All 2,482 measured execution inputs matched after `025`.
- **Scope-check correction:** The original R5 command starts at pending-spec creation inside M219, so it includes that workstream's later merged files. R5 now compares this branch with `origin/main`, matching the stated Files Changed check.
- **Approval:** Indy: "You are free to override orly, orly docs can be skipped." This covers the named workflow comment edit and extra documentation gates; hooks and signing still run. Indy also requested resource care with five agents. The `spec.baseline` override avoids duplicate comparison suites; the expanded caller tests now change example and fixture inputs. Final repository lanes still run.
- **Skill-chain outcomes:** Unit and integration audits closed the required-test ledger with real caller proof, typed supervisor responses, isolated failure injection and bounded resource cleanup. The final review and PR follow-up outcomes are recorded in Pull Request Session notes. Additional orly documentation and release files are waived by Indy.
- **Expanded scope:** Indy asked for tests starting where bubblewrap is called, covering a steer, workspace Git commit, memory, schedules, thread messages and Grafana. The kernel example now owns that proof. Direct `git push` is explicitly refused today; `propose_change` is not implemented. Rust coverage means 99% of changed production lines, as Indy confirmed.
- **Deferrals** — none. This spec is the home `M219_002`'s deferral names:
  > Indy (2026-10-10 10:00): "Bump snapshot in this PR" — context: the option read "The VM and doc fixes go to their own spec."; then "all changes have to go in 1 branch/worktree", so it lands on `docs/event-runtime-positioning`.
