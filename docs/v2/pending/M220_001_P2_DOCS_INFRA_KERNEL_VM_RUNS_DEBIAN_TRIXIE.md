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
**Status:** PENDING
**Priority:** P2 — developer tooling and doc accuracy; no product behaviour changes
**Categories:** DOCS, INFRA
**Batch:** B1 — one machine rebuild and three doc lines, independent of each other
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** `M219_002` §6 — the toolbox already builds from the Debian 13.7 snapshot `20261010T000000Z`
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 10, 2026) from Indy's Oct 10, 2026 choice recorded in `M219_002` Discovery
**Canonical architecture:** `docs/architecture/runner_execution.md` §Toolbox

---

## Overview

**Goal (testable):** `orb -m afr-kernel cat /etc/debian_version` prints `13.` followed by a point release, `make test-runner-kernel` passes there, and no doc names a tool the toolbox manifest does not install.
**Problem:** The local kernel-lane machine runs Ubuntu 24.04.5 while production runner hosts run Debian 13, so a host-userland difference can pass locally and fail on a host. Three docs also disagree with the code: the toolbox page lists Chromium, Codex and Claude Code, which the image does not hold; a CI comment calls the build debootstrap; and a Cargo comment names a bookworm deploy image.
**Solution summary:** Recreate the OrbStack machine `afr-kernel` from the `debian:trixie` image with the packages the kernel lane needs, and write its setup as a short doc so the machine can be rebuilt by anyone. OrbStack's own kernel stays, so the sandbox's kernel features do not change. Correct the three doc lines to what the code does today.

## PR Intent & comprehension handshake

- **PR title (eventual):** build: the kernel-lane machine runs Debian 13, and toolbox docs match the image
- **Intent (one sentence):** A developer's kernel lane runs on the same distribution as a production runner host, and a reader of the toolbox page learns what the image actually holds.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/v2/reviews/m211-toolbox-spikes.md` — records the current machine (Ubuntu 24.04.5 arm64, kernel 7.0.14-orbstack, tool versions) and what the kernel lane needs from it.
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

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — NLR (a touched stale line is fixed, not worked around); UFS does not fire on Markdown.
- `.orly/docs/DOCUMENTATION_RULES.md` — the architecture page and the development doc are published prose: sentence length and plain wording apply.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| CI/CD edit (`.github/workflows/**`) | yes — one comment line | Ask Indy in session before the edit; a "no" leaves the comment and records the reason in Discovery |
| File & Function Length (≤350/≤50/≤70) | no | Only documentation and comments change |

## Prior-Art / Reference Implementations

- **Reference:** `scripts/toolbox/install-builder-tools.sh` and `playbooks/lib/runner/` — the package set and host preparation production runner hosts already use on Debian 13; the setup doc mirrors them rather than inventing a list.

## Sections (implementation slices)

### §1 — The kernel-lane machine runs Debian 13

`afr-kernel` is deleted and recreated as `orb create debian:trixie afr-kernel`, then given the builder tools, bubblewrap, Rust at the workspace's pinned toolchain, and passwordless sudo for the lane's `sudo -E`. The machine is disposable: nothing on it is the only copy of anything. **Implementation default:** keep the name `afr-kernel` because the session learnings and docs already call it that.

- **Dimension 1.1** — The machine reports Debian 13 → Test `kernel_vm_is_debian_trixie` (manual: `orb -m afr-kernel cat /etc/debian_version` prints `13.`)
- **Dimension 1.2** — The kernel lane passes on it → Test `test_toolbox_carries_the_tools` with the rest of `make test-runner-kernel` (kernel lane, run on the machine)

### §2 — The setup is written down

`docs/development.md` gains the commands that create the machine and run the kernel lane on it, so it can be rebuilt by anyone who has OrbStack.

- **Dimension 2.1** — The doc's commands recreate a machine that passes 1.1 and 1.2 → Test `kernel_vm_setup_doc_rebuilds_the_machine` (manual: follow the doc on a fresh machine name)

### §3 — The docs name what the code does

The toolbox paragraph lists the manifest's packages and uv. The CI comment says mmdebstrap. The Cargo comment names the daemon's real base image, after confirming that image ships `/usr/share/zoneinfo`.

- **Dimension 3.1** — No doc names a toolbox tool the manifest lacks → Test `toolbox_doc_matches_manifest` (grep, rubric R2)
- **Dimension 3.2** — No build comment says debootstrap → Test `no_debootstrap_mention` (grep, rubric R3)
- **Dimension 3.3** — No comment names a bookworm deploy image → Test `no_bookworm_deploy_image` (grep, rubric R4)

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

N/A — no invariants: the work changes a developer machine and documentation.

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
| 3.1 | unit | `toolbox_doc_matches_manifest` | `git grep -nE 'Chromium\|chromium-headless-shell\|Claude Code' -- docs/architecture/runner_execution.md` → no output |
| 3.2 | unit | `no_debootstrap_mention` | `git grep -n debootstrap -- .github rustd scripts make` → no output |
| 3.3 | unit | `no_bookworm_deploy_image` | `git grep -n 'bookworm' -- rustd/Cargo.toml` → no output |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The kernel lane runs on Debian 13 (§1) | `orb -m afr-kernel cat /etc/debian_version` | starts with `13.` | P1 | |
| R2 | The toolbox doc names only the image's tools (§3) | `git grep -nE 'Chromium\|chromium-headless-shell\|Claude Code' -- docs/architecture/runner_execution.md` | no output | P1 | |
| R3 | No build comment says debootstrap (§3) | `git grep -n debootstrap -- .github rustd scripts make` | no output | P2 | |
| R4 | No comment names a bookworm deploy image (§3) | `git grep -n 'bookworm' -- rustd/Cargo.toml` | no output | P2 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
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
4. **Rebuild-vs-iterate** — Iterate: one machine recreated, three lines corrected.
5. **What we build** — A Debian 13 `afr-kernel`, its setup doc, and three corrected doc lines.
6. **What we do NOT build** — CI runner changes, toolbox package changes, any code change.
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
- **Skill-chain outcomes** — pending.
- **Deferrals** — none. This spec is the home `M219_002`'s deferral names:
  > Indy (2026-10-10 10:00): "Bump snapshot in this PR" — context: the option read "The VM and doc fixes go to their own spec."; then "all changes have to go in 1 branch/worktree", so it lands on `docs/event-runtime-positioning`.
