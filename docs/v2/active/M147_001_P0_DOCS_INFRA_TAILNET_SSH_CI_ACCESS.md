<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M147_001: CI reaches the bare-metal workers over Tailscale SSH under its own tag

**Prototype:** v2.0.0
**Milestone:** M147
**Workstream:** 001
**Date:** Jul 29, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — `deploy-worker-dev` has failed on every push to `main` since Jul 28, 2026; the bare-metal runner has shipped no build since.
**Categories:** DOCS, INFRA
**Batch:** B1 — standalone; no other workstream touches the tailnet policy.
**Branch:** feat/m147-tailnet-ssh-ci-access
**Test Baseline:** unit=3225 integration=457
**Depends on:** none
**Provenance:** agent-generated (incident response, GitHub Actions run 30464910532)
**Canonical architecture:** `playbooks/founding/02_preflight/tailnet-policy.hujson` — the tailnet policy is the source of truth for who may open a Secure Shell (SSH) session to a worker.

---

## Overview

**Goal (testable):** An ephemeral GitHub Actions node tagged `tag:ci` can open an SSH session and a Secure File Transfer Protocol (SFTP) transfer to a bare-metal worker tagged `tag:worker` as the non-root deploy user, and the tailnet policy refuses to save if that stops being true.

**Problem:** Every push to `main` since Jul 28, 2026 leaves the bare-metal dev worker running a stale `agentsfleet-runner` binary. The `deploy-worker-dev` job dies at "Provision runner env file from vault" with `tailnet policy does not permit you to SSH to this node` and exit 255. The dev release verdict is red, and the same failure is latent on the production path in `release.yml`.

**Solution summary:** Tailscale SSH was enabled on the workers (`EditPrefs: MaskedPrefs{RunSSH=true}`, Jul 28 01:03:20Z), which moved the access decision from the host's `sshd` and `~/.ssh/authorized_keys` to the tailnet policy's `ssh` block. That block only granted `autogroup:member`, a *user* identity, so the tagged Continuous Integration (CI) node matched nothing. This spec gives the workers their own tag (`tag:worker`), leaving `tag:ci` to mean only the ephemeral GitHub Actions node, and adds the `tag:ci` → `tag:worker` accept rule that CI needs. An `sshTests` assertion makes the guarantee self-enforcing, and the bootstrap playbooks stop re-advertising the old tag. Operator-visible outcome: worker deploys resume, and a future policy edit that would break them is rejected when it is saved rather than discovered as a red deploy.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(infra): CI reaches the workers under its own tailnet tag
- **Intent (one sentence):** Restore bare-metal worker deploys by granting the tagged CI node the SSH access that Tailscale SSH now arbitrates, and make the grant self-verifying so it cannot silently regress.
- **Handshake** — Restated: the workers and the CI runner currently share one tag, so no rule can express "runners may enter workers but workers may not enter each other"; splitting the tags is what makes the correct grant expressible, and the grant itself is what unblocks CI.
  `ASSUMPTIONS I'M MAKING:`
  1. The live tailnet policy matches the repo copy committed in `ea8f5d6ac`; the paste is diffed against the admin console before saving.
  2. The deploy user on both workers is the non-root `debian` (confirmed on `zombie-dev-worker-ant` via `id -un`) and holds passwordless `sudo`.
  3. Keeping the vault deploy key in the pipeline is acceptable for this workstream; retiring it is deliberately out of scope.
  4. Applying the policy and retagging the two worker nodes are human console actions — the repository's Tailscale OAuth client is scoped to `auth_keys` only (`policy_file` and `devices` both return HTTP 403).

## Implementing agent — read these first

1. `playbooks/founding/02_preflight/tailnet-policy.hujson` — the policy this spec rewrites; the header comment carries the two-tag model and why a member grant cannot cover CI.
2. `playbooks/founding/06_runner_bootstrap_dev/04_provision_runner_env.sh` — the script whose `scp` is the first thing the policy denies; the failure surfaces as a bare exit 255.
3. `.github/workflows/deploy-dev.yml` (`deploy-worker-dev`, the `tailscale/github-action@v4` step) — where `tags: tag:ci` is minted per run; it stays unchanged by design.
4. https://tailscale.com/kb/1193/tailscale-ssh — Tailscale claims port 22 on the tailnet address only; a rule whose `src` is a tag cannot use `check` mode.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `playbooks/founding/02_preflight/tailnet-policy.hujson` | EDIT | Becomes a two-tag policy with the CI grant and the `sshTests` regression guard. |
| `playbooks/founding/06_runner_bootstrap_dev/001_playbook.md` | EDIT | Bootstrap advertises `tag:worker`; prerequisite and `--ssh` notes state the port-22 consequence. |
| `playbooks/founding/07_runner_bootstrap_prod/001_playbook.md` | EDIT | Same change on the production bootstrap path. |
| `playbooks/founding/06_runner_bootstrap_dev/01_ssh_access.sh` | EDIT | Sources the shared helper so a denial in the access gate names its cause. |
| `playbooks/founding/06_runner_bootstrap_dev/04_provision_runner_env.sh` | EDIT | Routes its `scp` and `chmod` through the diagnosing wrapper. |
| `playbooks/lib/common.sh` | EDIT | Gains `playbooks_explain_ssh_failure` and `playbooks_ssh_run`. |
| `playbooks/founding/02_preflight/02_credentials.sh` | EDIT | Gains `check_worker_onboarded` so a worker with no tailnet identity is named a placeholder instead of passing as ready. |
| `playbooks/founding/02_preflight/credentials_test.sh` | EDIT | Three cases covering onboarded, placeholder-non-fatal, and shared-hostname attribution. |
| `src/runner/engine/CgroupScope.zig` | EDIT | Gains `enableDelegatedControllers` — writes `+cpu +memory +pids` to the delegated base so execution scopes have limit files to write. |
| `src/runner/main.zig` | EDIT | Calls it at startup behind `controllersRequired`, failing closed so a host that cannot build the cage leaves the fleet instead of refusing every lease. |
| `playbooks/lib/common_test.sh` | CREATE | Regression tests for the two new helpers. |
| `playbooks/founding/02_preflight/tailnet_policy_test.sh` | CREATE | Structural assertions on the canonical policy — the repo-side half of the `sshTests` guarantee, runnable without tailnet credentials. |
| `make/quality.mk` | EDIT | Adds both new test files to the `check-playbooks` regression list. |
| `docs/v2/active/M147_001_P0_DOCS_INFRA_TAILNET_SSH_CI_ACCESS.md` | CREATE | This spec. |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **UFS** (the denial substring `tailnet policy does not permit` and the tag identifiers are matched in one helper, not repeated per call site); **NDC** (no dead code — the wrapper is called from both scripts it is written for); **NLR** (touch-it-fix-it — the two bootstrap playbooks are corrected together rather than leaving the production one stale); **ORP** (orphan sweep — no file is deleted, so the sweep is a no-op assertion).
- `~/Projects/dotfiles/dispatch/write_shell.md` — applies to `playbooks/lib/common.sh`, `common_test.sh`, and both edited bootstrap scripts: quoted expansions, array arguments, no untrusted `eval`, repository shell compatibility.
- `~/Projects/dotfiles/dispatch/write_zig.md` — applies to `src/runner/engine/CgroupScope.zig` and `src/runner/main.zig` (§6): PUB surface justified against an in-tree consumer, `errdefer`/`defer` on every opened handle, the 350-line file cap, and the mandatory cross-compile of both Linux targets.
- `~/Projects/dotfiles/dispatch/write_documentation.md` — applies to the two bootstrap playbooks and the policy header prose.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `CgroupScope.zig` and `main.zig` (§6) | `make lint-zig` clean; cross-compiled for both `x86_64-linux` and `aarch64-linux`; `make test-unit-agentsfleet-runner` green. |
| PUB / Struct-Shape | yes — one new `pub fn enableDelegatedControllers` | Justified against an external consumer: `src/runner/main.zig` calls it at startup. No new type, so the file-as-struct shape of `CgroupScope.zig` is unchanged. |
| File & Function Length (≤350/≤50/≤70) | yes — `playbooks/lib/common.sh` grows, `CgroupScope.zig` sits at 340/350 | Both shell helpers stay well under the function cap. `CgroupScope.zig` ends at 340 lines: the path helper was inlined rather than kept, specifically to leave margin. Flagged in Session Notes — the next addition to that file should split it. Asserted by rubric row S8. |
| UFS (repeated/semantic literals) | yes | The denial substring and the remediation text live once, in `playbooks_explain_ssh_failure`; call sites pass only the captured output. |
| UI Substitution / DESIGN TOKEN | no — no `*.tsx`/`*.css` | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — shell diagnostics on stderr, no Zig logging surface, no schema change | N/A |
| SPEC TEMPLATE GATE | yes — this file | `bash audits/spec-template.sh --staged` clean before commit. |

## Prior-Art / Reference Implementations

- **Reference:** `playbooks/operations/teardown/database/02_teardown.sh` — the established `SCRIPT_DIR` + `source "${SCRIPT_DIR}/../../../lib/common.sh"` pattern with `playbooks_`-prefixed helpers; the new helpers and their wiring mirror it exactly rather than inventing a second shared-library convention.
- **Reference:** `playbooks/founding/06_runner_bootstrap_dev/provision_runner_env_test.sh` — the `PATH`-stubbing, `ok`/`bad` counter test style that `playbooks/lib/common_test.sh` follows.

## Sections (implementation slices)

### §1 — Two tags, so the grant is expressible

While the ephemeral CI node and the bare-metal workers share `tag:ci`, the only rule that can be written is `tag:ci` → `tag:ci`, which also grants dev worker → production worker shell as a passwordless-`sudo` user. Splitting the tags is therefore not hardening on top of the fix — it is what makes the correct fix expressible at all. **Implementation default:** move the *workers* to `tag:worker` and leave the workflows minting `tag:ci` untouched, because that direction requires no `.github/workflows/**` edit.

- **Dimension 1.1** — The policy declares both `tag:ci` and `tag:worker` with `autogroup:admin` as tag owner, so an admin can retag a machine from the console → Test `test_policy_declares_both_tag_owners`
- **Dimension 1.2** — Both bootstrap playbooks advertise `tag:worker`, so a future re-bootstrap cannot silently restore the broken tag → Test `test_bootstrap_playbooks_advertise_worker_tag`

### §2 — The grant CI actually needs

An accept rule from `tag:ci` to `tag:worker` for non-root users. `check` mode is impossible here: a tagged source has no user behind it to re-authenticate in a browser. `root` is deliberately excluded because every deploy step escalates through passwordless `sudo`.

- **Dimension 2.1** — The `ssh` block carries an `accept` rule with `src` `tag:ci` and `dst` `tag:worker` → Test `test_policy_grants_ci_tag_to_worker_tag`
- **Dimension 2.2** — Human access to the workers survives the retag window, because the member rule lists both tags until both nodes carry `tag:worker` → Test `test_member_rule_covers_both_tags_during_retag`

### §3 — The guarantee enforces itself

A policy edit that breaks CI must fail at save time, not hours later as a red deploy. Tailscale evaluates `sshTests` server-side when the policy is saved. **Implementation default:** `dst` is `tag:worker` rather than a host list, because a host list asserts the current tag *assignment* — `zombie-dev-worker-ant` is not reachable by `tag:ci` until it is retagged, so the assertion would fail and reject the very save that ends the outage — while the tag asserts the *rule*, resolves regardless of which nodes wear it, and covers every future worker with no list to keep in sync.

- **Dimension 3.1** — An `sshTests` entry asserts `tag:ci` may reach `tag:worker` as the deploy user and not as `root`, with `dst` holding only tags → Test `test_policy_asserts_ci_access_in_sshtests`

### §4 — A denial names its own cause

The outage cost a journal dig on the worker because `scp` reported only exit 255. The layer that refused is knowable from the transcript.

- **Dimension 4.1** — A tailnet-policy denial prints the missing tag rule and the policy file path → Test `test_should_name_the_missing_tag_rule_on_a_policy_denial`
- **Dimension 4.2** — An absent-host-key failure points at `--ssh` → Test `test_should_point_at_the_ssh_flag_when_host_keys_are_absent`
- **Dimension 4.3** — An unrecognised failure adds no misleading diagnosis → Test `test_should_stay_silent_on_an_unrecognised_failure`
- **Dimension 4.4** — The wrapper passes stdout through on success → Test `test_should_pass_stdout_through_when_the_command_succeeds`
- **Dimension 4.5** — The wrapper preserves the original exit status so `set -e` callers still die → Test `test_should_preserve_exit_status_and_explain_a_policy_denial`

### §5 — A worker with no machine stops reading as ready

`zombie-prod-worker-bird` was created on Mar 14, 2026 by duplicating the `zombie-prod-worker-ant` vault item. The copy carried the provider `hostname`, `deploy-user`, and admin-console credentials over verbatim; only `ssh-private-key` and `runner-token` were regenerated, and `tailscale-hostname` was never added. Its key was installed into `authorized_keys` on ant's box — SSH with bird's key reaches machine-id `38fde3f7bb6e48ed969d2ffc00de192a`, which is ant's. No second machine was ever bought. The credential gate has been passing bird as ready ever since, because a full credential set is indistinguishable from a real worker when nothing checks for a tailnet identity.

**Implementation default:** report, never fail. A machine that was never provisioned is not a credential fault, and failing the gate would block deploys over a host nobody is waiting on. The distinguishing signal is the absence of `tailscale-hostname` — CI reaches workers by their tailnet name and never by the provider hostname, so an item without one cannot be deployed to by definition.

- **Dimension 5.1** — A worker item carrying a `tailscale-hostname` is reported as onboarded → Test `test_should_report_a_worker_on_the_tailnet_as_onboarded`
- **Dimension 5.2** — A worker item with no `tailscale-hostname` is named a placeholder and does not fail the gate → Test `test_should_report_a_worker_without_tailnet_identity_as_a_non_fatal_placeholder`
- **Dimension 5.3** — A placeholder sharing a sibling's provider `hostname` says whose box it actually points at → Test `test_should_name_the_sibling_when_a_placeholder_shares_its_host`

### §6 — The runner enables its own delegated controllers

Retagging the workers let the `Verify delegated runner cgroup` step run for the first time — it was added Jul 28 (`c59c133d2`) while CI was already broken, so it had never executed. It failed with `controller 'cpu' is not enabled`, and the check was right.

`systemd`'s `Delegate=cpu memory pids` only makes controllers *available* in the unit cgroup (`cgroup.controllers`); writing `cgroup.subtree_control` is the delegatee's job, and nothing did it — `grep -rn subtree_control src/` found no writer. The consequence was total and silent: `CgroupScope.create()` made the scope directory, then failed writing `memory.max` because the file did not exist, so the daemon refused every lease `sandbox_unavailable` (UZ-RUN-007) while orphan scope directories accumulated. The live dev worker had **97 orphan `exec-*` cgroups** and was rejecting a lease every 5–25 seconds.

The unit was already built for this — `ReadWritePaths=/sys/fs/cgroup/system.slice/agentsfleet-runner.service` grants exactly the write access needed, and `DelegateSubgroup=runner` keeps the daemon out of the base cgroup so the write is legal under cgroup v2's no-internal-processes rule. Only the code was missing.

**Implementation default:** fail closed at startup rather than per lease. A host that cannot build the cage will refuse every lease anyway; exiting means it removes itself from the fleet, the control plane re-leases elsewhere, `systemd`'s restart makes it loud, and the deploy health check catches it — instead of the host black-holing work while reporting healthy. `dev_none` is exempt because it has no cage to build.

**On what is and is not unit-testable here — stated plainly, because getting this wrong is what caused the bug.** Enabling controllers needs a real cgroup-v2 mount with a delegated subtree; no unit test can assert it without one. The existing enforcement lane appeared to cover this and did not: `scripts/cgroup-delegate.sh` *performed the setup itself* — its comment even claims it is "matching the service-owned cgroup subtree used in production", an assumption that was false — and every unprivileged lane `SkipZigTest`s. So the suite proved "given a delegated subtree, limits are enforced" while nobody asserted "the runner establishes its own subtree". The Dimensions below therefore claim only what they prove, and the Linux behaviour is bound to an automated gate that runs on every deploy rather than to a test that would skip.

- **Dimension 6.1** — Controller enablement is attempted only on a Linux host running a tier that builds a cage; `dev_none` and `macos_seatbelt` are exempt, so neither is turned into a startup failure over a subtree that cannot exist → Test `delegated controllers are required only for a Linux tier that builds a cage`
- **Dimension 6.2** — On a real delegated host the runner enables its own controllers, proven end-to-end by the `Verify delegated runner cgroup` step of `deploy-worker-dev`, which reads `cgroup.subtree_control` back after the deploy. Automated, runs on every deploy, and cannot skip — this is precisely the assertion the old harness never made. Rubric row R8.

## Interfaces

```
playbooks_explain_ssh_failure <transcript>
  stdin:  none
  args:   $1 — combined stdout+stderr of a failed ssh/scp call
  stdout: none
  stderr: remediation lines for a recognised cause; nothing otherwise
  status: always 0 (diagnosis is advisory, never fatal)

playbooks_ssh_run <description> <command> [args...]
  args:   $1 — human-readable description used in the failure line
          $2… — the command to run, passed through verbatim
  stdout: the command's combined output, on success only
  stderr: "  ✗ <description> failed (exit N)", the transcript, then the diagnosis
  status: 0 on success; the command's original exit status on failure

tailnet policy ssh grant (playbooks/founding/02_preflight/tailnet-policy.hujson)
  { "action": "accept", "src": ["tag:ci"], "dst": ["tag:worker"],
    "users": ["autogroup:nonroot"] }
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Policy denies the tagged source | `ssh` block has no rule whose `src` matches `tag:ci` | `scp`/`ssh` exit 255; the wrapper prints the transcript then names the missing `tag:ci` → `tag:worker` rule and the policy file path. |
| Node advertises no SSH host keys | `tailscale up` ran without `--ssh` | Client reports `Host key verification failed`; the wrapper points at re-running `tailscale up ... --ssh`. |
| Host `sshd` rejects the key | Public-IP path with a stale `authorized_keys` | Client reports `Permission denied`; the wrapper states that Tailscale SSH bypasses `authorized_keys` on the tailnet address, so this concerns the public IP only. |
| Retag locks out human access | A node moves to `tag:worker` before the policy grants members access to it | Prevented by ordering: the member rule lists `tag:worker` **and** `tag:ci` while the retag is in flight; the host's `sshd` on the public IP is the break-glass path either way. |
| Unrecognised transport failure | Anything outside the three known causes | The wrapper prints the transcript and adds no diagnosis, so a wrong cause is never asserted. |
| Bootstrap re-advertises the old tag | A future re-run of the playbook's `tailscale up` with `--advertise-tags=tag:ci` | Prevented by §1.2: both playbooks advertise `tag:worker`, asserted by a grep rubric row. |

## Invariants

1. CI's SSH path to the workers is asserted in the policy, not in review — the `sshTests` block is evaluated server-side at save time, so a policy that breaks CI cannot be saved.
2. The wrapper never converts a failure into a success — `playbooks_ssh_run` returns the original exit status, proven by `test_should_preserve_exit_status_and_explain_a_policy_denial` under a `set -e` caller.
3. The diagnosis never asserts a cause it did not observe — the `case` matches literal transcript substrings and stays silent otherwise, proven by `test_should_stay_silent_on_an_unrecognised_failure`.
4. No bootstrap path re-advertises `tag:ci` on a worker — asserted by rubric row R3 as a zero-match grep across `playbooks/`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | this workstream alters tailnet access policy and playbook diagnostics only; no analytics event is added, renamed, or removed | not applicable | the diagnosis prints only literal remediation text — never the transcript's credentials, key material, or vault references | `test_should_stay_silent_on_an_unrecognised_failure` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_policy_declares_both_tag_owners` | `tagOwners` in the policy file names `tag:ci` and `tag:worker`, each owned by `autogroup:admin`. |
| 1.2 | unit | `test_bootstrap_playbooks_advertise_worker_tag` | Grepping `playbooks/` for `--advertise-tags=tag:ci` returns zero matches; both playbooks carry `--advertise-tags=tag:worker`. |
| 2.1 | unit | `test_policy_grants_ci_tag_to_worker_tag` | The `ssh` block contains an entry whose `action` is `accept`, `src` is `tag:ci`, and `dst` is `tag:worker`. |
| 2.2 | unit | `test_member_rule_covers_both_tags_during_retag` | The `autogroup:member` accept rule lists `tag:worker` and `tag:ci`, so the retag cannot strand human access. |
| 3.1 | unit | `test_policy_asserts_ci_access_in_sshtests` | `sshTests` contains an entry with `src` `tag:ci`, `dst` holding `tag:worker` and no hostnames, a non-empty `accept`, and `root` under `deny`. |
| 4.1 | unit | `test_should_name_the_missing_tag_rule_on_a_policy_denial` | Input `tailscale: tailnet policy does not permit you to SSH to this node` → stderr contains `tag:ci`, `tag:worker`, and `tailnet-policy.hujson`. |
| 4.2 | unit | `test_should_point_at_the_ssh_flag_when_host_keys_are_absent` | Input `Host key verification failed.` → stderr contains `--ssh`. |
| 4.3 | unit | `test_should_stay_silent_on_an_unrecognised_failure` | Input `some unrelated transport error` → stderr is empty. |
| 4.4 | unit | `test_should_pass_stdout_through_when_the_command_succeeds` | A command printing `remote-ok` and exiting 0 → wrapper exits 0 and reproduces `remote-ok` on stdout. |
| 4.5 | unit | `test_should_preserve_exit_status_and_explain_a_policy_denial` | A command emitting the denial on stderr and exiting 255 → wrapper exits 255, output carries the description and the `tag:worker` remediation. |
| 5.1 | unit | `test_should_report_a_worker_on_the_tailnet_as_onboarded` | A worker item whose `tailscale-hostname` resolves → gate output contains `onboarded: zombie-dev-worker-ant`, exit 0. |
| 5.2 | unit | `test_should_report_a_worker_without_tailnet_identity_as_a_non_fatal_placeholder` | `tailscale-hostname` unreadable → output contains `PLACEHOLDER: zombie-dev-worker-ant` **and** the gate still exits 0. |
| 5.3 | unit | `test_should_name_the_sibling_when_a_placeholder_shares_its_host` | A placeholder whose `hostname` equals its sibling's → output states the host belongs to `zombie-prod-worker-ant`. |
| regression | unit | `provision_runner_env_test.sh` (both cases) | The wrapper insertion leaves the deferred-start and restart-and-verify paths of `04_provision_runner_env.sh` behaving identically. |
| regression | unit | `credentials_test.sh` (eight pre-existing cases) | Adding `check_worker_onboarded` to the dev and prod paths leaves every existing credential assertion passing. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The policy grants the CI tag SSH access to the worker tag (§2) | `grep -A3 '"src":    \["tag:ci"\]' playbooks/founding/02_preflight/tailnet-policy.hujson` | output contains `"dst":    ["tag:worker"]` | P0 | |
| R2 | The policy asserts that grant in `sshTests` (§3) | `grep -c '"sshTests"' playbooks/founding/02_preflight/tailnet-policy.hujson` | `1` | P0 | |
| R3 | No bootstrap path advertises the old tag on a worker (§1) | `grep -rn --include='*.md' -- '--advertise-tags=tag:ci' playbooks/ \| wc -l \| tr -d ' '` | `0` | P0 | |
| R4 | A policy denial names its cause instead of exiting 255 silently (§4) | `bash playbooks/lib/common_test.sh` | `5 passed, 0 failed` | P0 | |
| R5 | The live dev worker deploy is green again | `gh run rerun --job "$(gh run view 30464910532 --json jobs --jq '.jobs[] \| select(.name=="deploy-worker-dev") \| .databaseId')" && gh run watch 30464910532` | `deploy-worker-dev` concludes `success` | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R7 | A worker with no machine no longer reads as ready (§5) | `bash playbooks/founding/02_preflight/credentials_test.sh` | `11 passed, 0 failed` | P0 | |
| R8 | The runner builds its own resource cage on a real host (§6) | **first** get a binary built from this branch onto the worker (`gh workflow run deploy-dev.yml --ref feat/m147-tailnet-ssh-ci-access`, or the post-merge `main` run) — **rerunning the old `deploy-worker-dev` job does NOT work: it redeploys run 30464910532's artifact, built from `main`, which predates this fix.** Then: `REQUIRE_RUNNER_CGROUP_DELEGATION=1 VAULT_DEV=ZMB_CD_DEV ./playbooks/founding/06_runner_bootstrap_dev/03_deploy_readiness.sh` | `✓ runner cgroup delegation: /system.slice/agentsfleet-runner.service (cpu memory pids)` | P0 | |
| R9 | Zig gates clean (§6) | `make lint-zig && make test-unit-agentsfleet-runner && zig build --build-file build_runner.zig -Dtarget=x86_64-linux && zig build --build-file build_runner.zig -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S1 | Playbook gate passes end to end | `make check-playbooks` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `--advertise-tags=tag:ci` (worker bootstrap) | `grep -rn --include='*.md' -- '--advertise-tags=tag:ci' playbooks/` | 0 matches |

## Out of Scope

- **Retiring the vault deploy key from the pipeline.** Under Tailscale SSH the `-i "$ANT_KEY"` flag is already ignored on the tailnet address, so `WORKER_ANT_SSH_KEY` is inert weight in both workflows. Removing it touches `.github/workflows/**` and both workers' `authorized_keys`, and needs a confirmed break-glass path first. Follow-up spec.
- **Putting the tailnet policy under GitOps.** `tailscale/gitops-acl-action@v1` would test the policy on a Pull Request (PR) and apply it on merge, making the repo copy authoritative instead of a transcription of what somebody pasted. Needs a new OAuth client with `policy_file` scope. Follow-up spec.
- **Tightening the allow-all network grant.** `grants` still carries `{"src": ["*"], "dst": ["*"], "ip": ["*"]}`, so the tag split constrains SSH but not packet-level reachability. Separate hardening decision.
- **Dropping the transitional `tag:ci` entry from the member `ssh` rule.** It is removable once both workers carry `tag:worker`; left in place so this spec needs exactly one policy save.
- **Provisioning a machine for `zombie-prod-worker-bird`.** Buying a server is playbook 07 step 0.0, explicitly human-owned, so bird's onboarding cannot land here. §5 stops it from being reported as a ready worker in the meantime; the vault item stays for whenever a second production box is bought. Bringing it up then needs no policy edit — `dst: ["tag:worker"]` covers it the moment it advertises the tag — but does need `hostname` repointed at the new box, `tailscale-hostname` added, and the entry added to `PROD_WORKER_HOSTS`.
- **Removing bird's orphaned key from ant's `authorized_keys`.** Bird's distinct `ssh-private-key` authenticates to ant's box today. Harmless while both are ours, but it is a credential with no owning machine. Deliberately untouched: pruning `authorized_keys` on a production host is not something to bundle into an outage fix.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A push to `main` lands, and the bare-metal worker is running the binary from that commit: `deploy-worker-dev` is green and the Discord verdict says `worker: success` instead of `worker: failure`.
2. **Preserved user behaviour** — Kishore keeps keyless `tailscale ssh` into both workers from the laptop; the workflows keep minting `tag:ci` exactly as they do today; the public-IP `sshd` break-glass path with the vault key keeps working.
3. **Optimal-way check** — The unconstrained-optimal shape is a pull-based worker that self-updates from the control plane and needs no inbound access at all. That deletes the deploy key and the SSH grant together, and is a milestone rather than an outage fix. The gap is acceptable now because the worker still has to be reachable for its first install regardless.
4. **Rebuild-vs-iterate** — Iterate. The retag plus one grant restores determinism immediately; a rebuild of the deploy transport would trade a same-day fix for weeks of drift.
5. **What we build** — Two tags in the policy, one accept rule, one `sshTests` assertion, the corrected `--advertise-tags` in both bootstrap playbooks, and a shared helper that names the refusing layer.
6. **What we do NOT build** — Deploy-key retirement (needs a break-glass audit), GitOps policy sync (needs a new OAuth client), network-grant tightening (separate risk decision), pull-based deploys (milestone-sized).
7. **Fit with existing features** — Compounds with the runner bootstrap playbooks and the `DEV_WORKER_READY` gate. It must not destabilize human tailnet access to the workers, which is why the retag window keeps both tags in the member rule.
8. **Surface order** — N/A — no user surface. The change is a tailnet policy and two operator playbooks.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — The operator hitting this failure now reads the cause in the job log: the missing `tag:ci` → `tag:worker` rule and the path to `tailnet-policy.hujson`. That replaced a journal dig on the worker host.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Four Sections that split cleanly by artifact: the tag model (§1), the grant (§2), the self-enforcement (§3), and the diagnostics (§4). §1 and §2 must land together — the grant references a tag that §1 introduces — while §3 and §4 are independently verifiable without touching live infrastructure.
- **Alternatives considered:**
  - *Smaller patch — a single `tag:ci` → `tag:ci` rule.* One console paste, green in a minute, no retag. Rejected because both workers and every ephemeral runner share `tag:ci`, so the same rule grants dev worker → production worker shell as a passwordless-`sudo` user. The tag split costs two console clicks more and removes that path entirely.
  - *Smaller patch — `tailscale set --ssh=false` on the workers.* One command, restores the exact configuration that was green through Jul 27. Rejected because it reverses a deliberate decision made on Jul 28 and leaves worker access governed by `authorized_keys` with no central revocation.
  - *Larger refactor — pull-based worker self-update.* The real ceiling; deletes the deploy key and the SSH grant together. Rejected for now as milestone-sized, and named in Out of Scope rather than mud-patched around.
- **Patch-vs-refactor verdict:** this is a **patch** because the transport is sound and exactly one policy statement is missing; the refactor that would obsolete the whole path is named as follow-up work rather than smuggled into an outage fix.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — no analytics or funnel playbook update required: this workstream changes tailnet access policy and playbook diagnostics only, and adds, renames, or removes no product or operator event.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
  - **greptile P2 — "Production worker missing from SSH test" (`tailnet-policy.hujson:119`)** — valid, and fixed structurally rather than by extending the host list. Indy asked whether `dst` could be a glob (`zombie-dev*`); Tailscale supports no glob or prefix matching on host fields, but `dst` does accept a tag ("can be a user's email address, a group, a tag, or a host that maps to an IP address"). Switching `dst` to `["tag:worker"]` covers every worker present and future with no list to keep in sync, so the finding cannot recur. That question also surfaced a latent bug in the first cut: with hostnames, `sshTests` asserts the current tag *assignment*, and `zombie-dev-worker-ant` is not reachable by `tag:ci` until it is retagged — so the assertion would have failed and Tailscale would have **rejected the policy save**, blocking the outage fix. `test_policy_asserts_ci_access_in_sshtests` now asserts `dst` holds only tags, so a hostname cannot creep back in. Context on `bird`, kept in the prod playbook: it holds vault entries (`02_preflight/02_credentials.sh`) but is absent from `PROD_WORKER_HOSTS` and from the tailnet, so bringing it up means adding it to that variable — it needs no policy edit.
  - **CI triage** — six jobs (`lint-zig`, `memleak`, `test-coverage-zig`, `test-integration-kernel`, `test-unit-agentsfleet-lib`, `test-unit-agentsfleet-runner`) failed on the first run with `unable to discover remote git server capabilities: ProtocolError` fetching `git+https://codeberg.org/ziglang/translate-c` via the `pg` dependency. Not caused by this diff, which contains no Zig and no `build.zig.zon`: `test.yml` on `main` was green at 15:14Z and codeberg was unreachable by 17:50Z. Confirmed recovered (HTTP 200) and reran the failed jobs.
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
