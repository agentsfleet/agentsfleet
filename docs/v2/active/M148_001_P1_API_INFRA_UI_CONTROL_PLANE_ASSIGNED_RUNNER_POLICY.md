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

# M148_001: The isolation an operator assigns is the isolation the runner applies

**Prototype:** v2.0.0
**Milestone:** M148
**Workstream:** 001
**Date:** Jul 30, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing: the dashboard presents a security property that does not drive runner behaviour, and a host that cannot deliver its claimed isolation keeps accepting work.
**Categories:** API, INFRA, UI
**Batch:** B1 — sequenced before any microVM tier; the assignment path must be authoritative before tiers denote different isolation strengths.
**Branch:** feat/m148-assigned-runner-policy — worktree `../agentsfleet-m148-assigned-runner-policy`
**Test Baseline:** unit=3266 integration=501
**Depends on:** M147_001 — delivers `enableDelegatedControllers`, the cgroup capability §3's probe reads back.
**Provenance:** agent-generated (design consult with Indy, Jul 30, 2026, arising from the M147_001 cgroup investigation)
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Isolation tiers

---

## Overview

**Goal (testable):** A runner applies the sandbox tier, network policy, registry allowlist, and worker count assigned to it in the control plane; refuses to lease when it cannot achieve the assigned policy; and reads only `AGENTSFLEET_API_URL` and `AGENTSFLEET_RUNNER_TOKEN` from its environment.

**Problem:** An operator picks a sandbox tier in **Add Runner** and the dashboard shows it on the runner row. The host reads an entirely different value from `/etc/default/agentsfleet-runner`. Both exist, neither wins, and nothing reconciles them — so the tier an operator sees is not the tier that runs. Worse, a host whose kernel cannot deliver the isolation it claims keeps accepting leases: the M147_001 investigation found the dev worker advertising `landlock_full` while refusing every lease for two days, with nothing surfacing the contradiction.

**Solution summary:** Invert the direction of configuration. Policy — sandbox tier, network policy, registry allowlist, worker count — becomes an attribute the control plane *assigns* to a runner row and delivers with the runner's identity, instead of a value each host declares locally. The runner probes what its kernel can actually enforce, reports that with its first heartbeat, and the control plane reconciles assigned against achievable: a runner that cannot meet its assignment is marked degraded and is sent no work, with the reason visible on the runner row. The host environment collapses from twelve variables to two. Operator-visible outcome: the tier on the runner row is the tier being enforced, and a host that cannot enforce it says so instead of quietly failing.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api,runner,app): the assigned isolation is the applied isolation
- **Intent (one sentence):** Make the sandbox policy an operator picks in the dashboard the policy the runner actually enforces, and make a host that cannot enforce it visibly refuse work rather than silently accept it.
- **Handshake (filled at PLAN, Jul 30, 2026):** Today the Add Runner tier selection is decorative — the host obeys its own env file and nothing compares the two. I will make the control plane the single author of runner policy (sandbox tier, network policy, registry allowlist, worker count), deliver it with the runner's identity on enrollment and on every heartbeat, have the runner probe what its kernel can actually enforce and report that upward, and have the control plane reconcile the two: an assignment the host cannot meet marks the runner degraded with the missing mechanism named, the control plane issues it no leases, and the runner refuses to take any. The host environment collapses to `AGENTSFLEET_API_URL` + `AGENTSFLEET_RUNNER_TOKEN` (+ optional `RUNNER_STORAGE_HOME`), and every error path fails closed to the safest posture, mirroring `parseSandboxTier`.
- `ASSUMPTIONS I'M MAKING:` (1) Wire paths follow the frozen `me`-plane convention — `POST /v1/runners/me/heartbeats`, `GET /v1/runners/me` — not the illustrative `/{id}/` paths this spec first sketched; the Interfaces block below is amended to match `protocol.zig`. (2) `degraded` is a computed reconciliation verdict stored beside `admin_state`, never a new `AdminState` value — cordon/drain/revoke stay pure operator intent. (3) The probe re-runs on each heartbeat tick (cheap kernel reads off the hot path); the wire report is sent when first available or when it changes. (4) Degraded gates only *new* lease issuance; in-flight leases finish under their issue-time policy. (5) `RUNNER_HOST_ID` is removable because the token already resolves the row and its server-side `host_id`; the daemon logs the row's host_id after its first policy fetch instead. (6) Worker-count changes bind at the next loop tick, never by killing live workers.

## Implementing agent — read these first

1. `src/lib/contract/protocol.zig` (`SandboxTier`, around line 116) — the doc comment states today's model outright: the tier is self-reported telemetry, placement keys off operator-assigned trust, and "a runner can lie about its tier". This workstream changes the first half and deliberately does not change the last.
2. `src/runner/daemon/config.zig` — every environment variable the runner reads, and the parse-and-fail-closed pattern (`parseSandboxTier`) the assigned-policy path should mirror.
3. `src/agentsfleetd/http/handlers/runner/register.zig` — records the self-reported tier today; becomes the assignment surface.
4. `ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx` — where the operator already selects a tier that currently changes nothing on the host.
5. `docs/v2/active/M147_001_P0_DOCS_INFRA_TAILNET_SSH_CI_ACCESS.md` §6 — the failure this reconciliation loop is designed to catch on the first heartbeat rather than after two days. (Path corrected at CHORE(open): the M147_001 spec still sits in `active/` even though its code is merged.)

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/lib/contract/protocol.zig` | EDIT | Adds the assigned-policy payload and the capability report; `SandboxTier`'s doc comment stops describing telemetry. |
| `schema/{next}_runner_assigned_policy.sql` | CREATE | Assigned policy columns, reported capability columns, and the degraded state on `fleet.runners`. |
| `schema/embed.zig` | EDIT | Registers the migration in the array. |
| `src/agentsfleetd/http/handlers/runner/register.zig` | EDIT | Assigns policy to the row instead of recording a claim. |
| `src/agentsfleetd/http/handlers/runner/self.zig` | EDIT | Returns the assigned policy the runner must apply. |
| `src/agentsfleetd/http/handlers/runner/heartbeat.zig` | EDIT | Carries policy down, accepts the capability report up, and computes the degraded verdict. |
| `src/agentsfleetd/http/handlers/fleet/runners_list.zig` | EDIT | Surfaces assigned, achievable, and degraded reason. |
| `src/runner/daemon/config.zig` | EDIT | Environment surface collapses to bootstrap only; policy fields come from the control plane. |
| `src/runner/daemon/loop.zig` | EDIT | Applies assigned policy and reports capability on the first heartbeat. |
| `src/runner/engine/capability_probe.zig` | CREATE | Probes Landlock, seccomp, cgroup controllers, and bubblewrap; returns what this kernel can actually enforce. |
| `src/runner/main.zig` | EDIT | Runs the probe at startup and refuses to lease on an unachievable assignment. |
| `public/openapi/components/schemas.yaml` | EDIT | Assigned policy and capability report shapes. |
| `ui/packages/app/lib/api/runners.ts` | EDIT | Types for assigned versus achievable. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx` | EDIT | All four policy fields become assignments, with copy that says so (Dimension 4.4). |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.tsx` | EDIT | Shows the degraded state and the mismatch reason. |
| `playbooks/founding/06_runner_bootstrap_dev/04_provision_runner_env.sh` | EDIT | Writes two variables instead of four. |
| `deploy/baremetal/agentsfleet-runner.service` | EDIT | The unit comment stops documenting removed variables. |
| `docs/architecture/runner_fleet.md` | EDIT | Records the inverted direction and the reconciliation loop. |
| `src/agentsfleetd/http/handlers/runner/sql.zig` | EDIT | New columns cross every runner-row statement: register insert, self select, heartbeat bump, lease-issuance predicate. |
| `src/agentsfleetd/http/handlers/runner/lease.zig` | EDIT | A degraded runner is issued no lease. |
| `src/agentsfleetd/http/handlers/runner/heartbeat_reconcile.zig` | CREATE | Pure assigned-vs-achievable reconciliation with the operator-facing reason vocabulary. |
| `src/agentsfleetd/http/handlers/fleet/sql.zig` | EDIT | The fleet read selects assigned, achievable, and the degraded verdict. |
| `public/openapi/paths/**` (runner + fleet-runner paths) | EDIT | Path docs pick up the new request/response shapes. |
| `deploy/baremetal/deploy.sh` | EDIT | The required-env check drops `RUNNER_HOST_ID`. |
| `playbooks/founding/06_runner_bootstrap_dev/001_playbook.md` | EDIT | Env-file provisioning prose drops the removed variables. |
| `playbooks/founding/07_runner_bootstrap_prod/001_playbook.md` | EDIT | Env-file provisioning prose drops the removed variables. |
| `playbooks/operations/runner_onboarding/001_playbook.md` | EDIT | Env-file provisioning prose drops the removed variables. |
| `playbooks/founding/06_runner_bootstrap_dev/provision_runner_env_test.sh` | EDIT | Asserts the reduced env set (§5.3). |
| `src/agentsfleetd/http/handlers/fleet/runner_patch.zig` | EDIT | Accepts an `assigned_policy` update alongside the existing actions. |
| `src/lib/contract/protocol_test.zig` | EDIT | Tier vocabulary tests drop `macos_seatbelt` (§6). |
| `ui/packages/app/lib/api/runners.test.ts` | EDIT | Tier list test drops `macos_seatbelt` (§6). |
| `src/runner/child_supervisor.zig` | EDIT | Stale Seatbelt comment; the egress fail-closed branch now pairs with a degraded row instead of a silent loop. |
| `src/runner/sandbox_args_edge_test.zig` | EDIT | Stale Seatbelt comment (§6). |
| `src/runner/main.zig` (`controllersRequired` tests) | EDIT | Seatbelt test cases reworked (§6) — file already listed above for the probe. |
| `public/openapi.json` | EDIT | Regenerated from the yaml sources. |
| `src/runner/daemon/AppliedPolicy.zig` | CREATE | The one holder of the assigned policy: heartbeat-fed, mutex-guarded, fail-closed on malformed input; workers snapshot it per lease. |
| `src/runner/daemon/control_plane_client.zig` | EDIT | Heartbeat returns the whole parse (policy strings would dangle otherwise) with the policy as raw JSON for lenient decoding. |
| `src/runner/daemon/worker_pool.zig` | EDIT | Workers carry the policy holder and their index (soft-shrink). |
| `src/runner/daemon/policy_apply.zig` | CREATE | Apply-time gates split from the loop: release dev_none refusal, one-shot cgroup enablement, grow-needs-restart notice. |
| `src/runner/daemon/lease_run.zig` | EDIT | Storage-home rename. |
| `src/runner/bundle_extract.zig` | EDIT | Storage-home rename (bundle cache root). |

Test files created or updated alongside the edited modules join this table as they land; R6 grades against the final table. (Rows above the divider were extended at CHORE(open) — see Discovery.)

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (the probe must be called from production startup, not only tests); **NLR** (every removed environment variable is cleaned out of playbooks, the unit file, and docs in the same diff, not left dangling); **NLG** (no "legacy env fallback" framing pre-2.0.0 — the variables are removed, not deprecated); **ORP** (orphan sweep for each removed variable name); **UFS** (policy field names shared verbatim across Zig, OpenAPI, and TypeScript).
- `~/Projects/dotfiles/dispatch/write_zig.md` — the runner and control-plane changes: `pg` drain discipline in the heartbeat handler, tagged-union results, `errdefer` on the probe's opened handles, both Linux cross-compiles.
- `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` — the enrollment and heartbeat response shapes.
- `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` — the migration: no static strings in the schema, app-side named constants for the state values.
- `~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md` — the dashboard changes: design-system primitives, token utilities.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — runner and control-plane Zig | `make lint-zig`, both Linux cross-compiles, `make test-unit-agentsfleet-runner` and `make test-unit-agentsfleetd`. |
| PUB / Struct-Shape | yes — new probe module | `capability_probe.zig` is a single-primary-type module, so file-as-struct layout; every new `pub` justified against a named in-tree consumer. |
| File & Function Length (≤350/≤50/≤70) | yes | `config.zig` shrinks as the environment surface collapses; the heartbeat handler gains the reconciliation branch — split it out rather than growing past the cap. |
| UI Substitution / DESIGN TOKEN | yes — dashboard | Degraded state uses design-system Badge/Alert primitives and theme tokens, no arbitrary utilities. |
| SCHEMA GUARD | yes — new migration | Single-concern migration under 100 lines, `schema/embed.zig` plus the migration array updated together; no `DEFAULT`/`CHECK` string literals. |
| ERROR REGISTRY | yes | A named `UZ-…` code for the unachievable-assignment refusal. |
| LOGGING | yes | Structured events for probe result, assignment applied, and degraded verdict. |

## Prior-Art / Reference Implementations

- **Reference:** `src/runner/daemon/config.zig` `parseSandboxTier` — the established fail-closed parse (unknown value collapses to the safest tier, and the release gate then refuses it). The assigned-policy decoder mirrors this exactly rather than inventing a second posture.
- **Reference:** M147_001 §6 `controllersRequired` — the pattern of a pure, unit-testable predicate taking the environment as a parameter instead of reading `builtin` directly. The probe's per-capability checks follow it so the matrix is testable off-Linux.
- **Reference:** `src/agentsfleetd/http/handlers/runner/register.zig` — the nearest existing handler for shape, error mapping, and row-write discipline.

## Sections (implementation slices)

### §1 — Policy travels with identity

The runner's token already resolves its row. Everything the host currently declares locally can therefore come down that same path. **Implementation default:** deliver policy on both the enrollment response and every heartbeat, rather than enrollment alone, so an operator changing the tier takes effect without a host visit.

- **Dimension 1.1** ✅ DONE — The enrollment response carries assigned sandbox tier, network policy, registry allowlist, and worker count → Test `test_enrollment_returns_assigned_policy`
- **Dimension 1.2** ✅ DONE — Every heartbeat response carries the current assigned policy, so a dashboard change reaches the host without redeploying it → Test `test_heartbeat_carries_current_assigned_policy`
- **Dimension 1.3** ✅ DONE — Assigned policy is stored on the runner row, not derived per request → Test `test_assigned_policy_persists_on_the_runner_row`

### §2 — The runner obeys the assignment, not its environment

Today `RUNNER_SANDBOX_TIER` decides behaviour and the dashboard decides display. After this slice the assignment decides both. **Implementation default:** remove the environment variables outright rather than leaving them as overrides — a fallback path is exactly how the two sources of truth diverged, and pre-2.0.0 carries no compatibility obligation.

- **Dimension 2.1** ✅ DONE — The runner applies the assigned tier; the removed variable has no effect because it is no longer read → Test `test_runner_applies_assigned_tier_not_environment`
- **Dimension 2.2** ✅ DONE — Applied policy is logged at startup in terms of what was assigned and by whom → Test `test_startup_logs_the_applied_assignment`
- **Dimension 2.3** ✅ DONE — A malformed or unknown assigned policy fails closed to the safest posture and refuses to lease, never to a permissive default → Test `test_malformed_assignment_fails_closed`

### §3 — Probe what the kernel can actually enforce, and reconcile

The reason M147_001's cgroup gap survived two days is that nothing compared claim against reality. **Implementation default:** probe at startup and report with the first heartbeat, rather than probing per lease — the capabilities are host properties that do not change between leases, and a per-lease probe would put syscalls on the hot path.

- **Dimension 3.1** ✅ DONE — The probe reports Landlock availability, seccomp installability, cgroup controllers present in `subtree_control`, bubblewrap presence, and whether kernel-enforced egress is available in this build (`egress_enforcement` — false until the 2.0.1 `EgressScope` wiring lands, so an assigned `allow_list_egress` degrades visibly instead of silently refusing every lease) → Test `test_capability_probe_reports_each_enforcement_mechanism`
- **Dimension 3.2** ✅ DONE — The probe result is sent with the first heartbeat → Test `test_first_heartbeat_carries_the_capability_report`
- **Dimension 3.3** ✅ DONE — Assigned stronger than achievable marks the runner degraded and it is sent no leases → Test `test_unachievable_assignment_marks_runner_degraded`
- **Dimension 3.4** ✅ DONE — A degraded runner records a reason naming the missing mechanism → Test `test_degraded_runner_names_the_missing_mechanism`
- **Dimension 3.5** ✅ DONE — A runner that later satisfies its assignment clears the degraded state on the next heartbeat → Test `test_degraded_clears_when_capability_returns`

### §4 — The dashboard shows assigned against achievable

An operator must be able to see that the two agree, and see why when they do not.

- **Dimension 4.1** — The runner row shows the assigned tier and, when they differ, the achievable one → Test `test_runner_row_shows_assigned_and_achievable`
- **Dimension 4.2** — A degraded runner is visually distinct and states the missing mechanism → Test `test_degraded_runner_row_states_the_reason`
- **Dimension 4.3** ✅ DONE — Add Runner copy describes the selection as an assignment the host must satisfy, not a description of the host → Test `test_add_runner_copy_describes_an_assignment`
- **Dimension 4.4** ✅ DONE — Add Runner exposes all four policy fields (isolation, network policy, registry allowlist, workers) with safe defaults; network defaults to `allow_all` — the explicit interim posture — because `allow_list_egress` degrades every runner until 2.0.1's egress wiring lands → Test `test_add_runner_exposes_all_policy_fields`

### §5 — The environment surface collapses to two

Twelve variables today; four of them are timeouts and four are policy. **Implementation default:** the workspace-base variable stays as an environment variable — it names where the disk is, a genuine host-local fact the control plane cannot know — and is renamed `RUNNER_WORKSPACE_BASE` → `RUNNER_STORAGE_HOME` (Indy, Jul 30, 2026). The four `RUNNER_CP_*_MS` become code defaults with no environment surface.

- **Dimension 5.1** ✅ DONE — The runner reads only `AGENTSFLEET_API_URL`, `AGENTSFLEET_RUNNER_TOKEN`, and `RUNNER_STORAGE_HOME` → Test `test_runner_reads_only_the_bootstrap_environment`
- **Dimension 5.2** — No removed variable name survives anywhere in the repository → Test `test_no_removed_runner_env_names_remain`
- **Dimension 5.3** ✅ DONE — `04_provision_runner_env.sh` writes the reduced set and the provisioned file still brings a runner up → Test `provision_runner_env_test.sh` (extended)

### §6 — The tier vocabulary shrinks to tiers that exist

`macos_seatbelt` has been declared-but-fail-closed since M80_004 deferred it ("seatbelt is deprecated long back"); no enforcement code exists, and a tier that cannot be applied must not be assignable. Removed, not deprecated (NLG — pre-2.0.0). Directed by Indy in-session, Jul 30, 2026.

- **Dimension 6.1** ✅ DONE — `SandboxTier` and the dashboard tier list no longer carry `macos_seatbelt`; existing tier tests updated → Test `test_sandbox_tier_vocabulary_excludes_seatbelt`
- **Dimension 6.2** ✅ DONE — No `macos_seatbelt` reference survives outside historical specs and applied migration 017's comment → Test `test_no_seatbelt_references_remain` (the R7 grep)
- **Dimension 6.3** ✅ DONE — A stray stored `macos_seatbelt` row parses fail-closed to the safest posture and refuses to lease — no data migration; Dimension 2.3's decoder covers it → covered by `test_malformed_assignment_fails_closed`

## Interfaces

```
GET /v1/runners/me              → 200
  { id, host_id, status, assigned_policy: {
      sandbox_tier, network_policy, registry_allowlist[], worker_count },
    achievable: { landlock, seccomp, cgroup_controllers[], bubblewrap, egress_enforcement },
    degraded: bool, degraded_reason: string|null }

POST /v1/runners/me/heartbeats
  request  { capability_report?: { landlock, seccomp, cgroup_controllers[], bubblewrap, egress_enforcement } }
  response { status, assigned_policy: {…}, degraded: bool, degraded_reason: string|null }

POST /v1/runners                (platform admin)
  request  { host_id, assigned_policy: { sandbox_tier, network_policy,
             registry_allowlist[], worker_count }, labels[] }

PATCH /v1/fleets/runners/{id}   (platform admin)
  request  { action } | { assigned_policy: {…} }   — existing action mutations
           unchanged; a policy update is added so a dashboard retier has a wire
           surface (test 1.2 exercises it)

runner environment, complete:
  AGENTSFLEET_API_URL         required
  AGENTSFLEET_RUNNER_TOKEN    required
  RUNNER_STORAGE_HOME         optional, host-local path (renamed from RUNNER_WORKSPACE_BASE)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Assignment unachievable | Assigned `landlock_full`, host kernel lacks Landlock | Runner refuses to lease, reports the gap; row shows degraded with the missing mechanism. |
| Network policy unachievable | Assigned `allow_list_egress`; the build's `EgressScope` wiring is unbuilt until 2.0.1 | Row degraded, reason names egress enforcement — in the database and on the dashboard — replacing today's silent per-lease refusal loop (`child_supervisor.zig:133`). |
| Policy fetch fails at startup | Control plane unreachable | Runner does not start leasing; it must never fall back to a permissive local default. Retries with backoff. |
| Policy changes mid-flight | Operator retiers a runner with leases in progress | In-flight leases finish under the policy they started with; the new policy binds the next lease. |
| Malformed assigned policy | Bad row, partial migration | Parse fails closed to the safest posture and the runner refuses to lease, matching `parseSandboxTier`. |
| Capability report absent | Old runner against a new control plane | Row is degraded with reason `no capability report`; no leases until one arrives. |
| Runner lies in its report | Compromised host | **Not handled — out of scope.** Reporting is unauthenticated self-assertion; see Out of Scope. |
| Host loses a capability while running | Operator disables a controller under a live daemon | Caught on the next heartbeat's probe refresh; runner degrades and stops taking new leases. |

## Invariants

1. A runner never applies a weaker policy than the one assigned — the applied value is read from the assignment and nothing else, enforced by there being no environment read to fall back to (§5 removes the variable, and §5.2's grep asserts it stays removed).
2. A runner with an unmet assignment leases nothing — enforced in code by the refuse-to-lease branch, proven by `test_unachievable_assignment_marks_runner_degraded`.
3. Policy failure is never permissive — every parse and fetch error path resolves to the safest posture, mirroring `parseSandboxTier`; proven by `test_malformed_assignment_fails_closed`.
4. Assigned and achievable are stored separately and never overwritten by each other — a schema-level separation, so no code path can let a self-report silently become the assignment.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `runner_capability_reported` | ops | a runner sends its probe result | runner id, each mechanism's availability | no host credentials, no token material, no key material | `test_first_heartbeat_carries_the_capability_report` |
| `runner_degraded` | ops | assigned policy exceeds achievable capability | runner id, assigned tier, missing mechanism | as above | `test_unachievable_assignment_marks_runner_degraded` |
| `runner_policy_applied` | ops | a runner applies an assignment at startup or after a change | runner id, assigned tier, policy version | as above | `test_startup_logs_the_applied_assignment` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_enrollment_returns_assigned_policy` | Enrolling a runner whose row assigns `landlock_full` returns that tier in `assigned_policy`. |
| 1.2 | integration | `test_heartbeat_carries_current_assigned_policy` | Changing the row's tier between two heartbeats changes the second response without a restart. |
| 1.3 | integration | `test_assigned_policy_persists_on_the_runner_row` | Assigned policy survives a control-plane restart. |
| 2.1 | integration | `test_runner_applies_assigned_tier_not_environment` | With a conflicting environment value present in the process environment, the runner applies the assigned tier. |
| 2.2 | unit | `test_startup_logs_the_applied_assignment` | Startup emits `runner_policy_applied` naming the assigned tier. |
| 2.3 | unit | `test_malformed_assignment_fails_closed` | An unparseable assigned policy → safest posture and a refuse-to-lease verdict, never a permissive default. |
| 3.1 | unit | `test_capability_probe_reports_each_enforcement_mechanism` | Given stubbed availability for Landlock, seccomp, cgroups, bubblewrap, and egress enforcement, the report mirrors each independently. |
| 3.2 | integration | `test_first_heartbeat_carries_the_capability_report` | The first heartbeat body contains a capability report; later ones need not repeat it unchanged. |
| 3.3 | integration | `test_unachievable_assignment_marks_runner_degraded` | Assigned `landlock_full` + a report lacking Landlock → row degraded, lease request returns none. |
| 3.4 | unit | `test_degraded_runner_names_the_missing_mechanism` | The degraded reason contains the specific absent mechanism, not a generic string. |
| 3.5 | integration | `test_degraded_clears_when_capability_returns` | A later report satisfying the assignment clears degraded on that heartbeat. |
| 4.1 | e2e | `test_runner_row_shows_assigned_and_achievable` | The runners list renders both values when they differ. |
| 4.2 | e2e | `test_degraded_runner_row_states_the_reason` | A degraded runner is visually distinct and names the missing mechanism. |
| 4.3 | unit | `test_add_runner_copy_describes_an_assignment` | Dialog copy describes an assignment the host must satisfy. |
| 4.4 | unit | `test_add_runner_exposes_all_policy_fields` | The dialog renders isolation, network policy, registry allowlist, and workers with the documented defaults (network → `allow_all`). |
| 5.1 | unit | `test_runner_reads_only_the_bootstrap_environment` | Config load touches exactly the three permitted names. |
| 5.2 | unit | `test_no_removed_runner_env_names_remain` | Grepping the repository for each removed name returns zero matches outside historical specs. |
| 5.3 | unit | `provision_runner_env_test.sh` (extended) | The provisioned env file contains the reduced set and still starts a runner. |
| 6.1 | unit | `test_sandbox_tier_vocabulary_excludes_seatbelt` | `SandboxTier` (Zig) and `SANDBOX_TIERS` (TypeScript) no longer contain `macos_seatbelt`. |
| 6.2 | unit | `test_no_seatbelt_references_remain` | The R7 grep returns zero matches. |
| regression | integration | existing runner lease suite | Lease, renew, report, and drain behaviour is unchanged for a runner whose assignment matches its capability. |
| regression | unit | `parseSandboxTier` suite | The existing fail-closed parse behaviour is preserved where it is reused by the assignment decoder. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The assigned tier is the applied tier (§2) | `make test-integration` | exit 0, `test_runner_applies_assigned_tier_not_environment` passes | P0 | |
| R2 | An unmet assignment leases nothing (§3) | `make test-integration` | exit 0, `test_unachievable_assignment_marks_runner_degraded` passes | P0 | |
| R3 | Policy failure is never permissive (§2, §3) | `make test-unit-agentsfleet-runner` | exit 0, `test_malformed_assignment_fails_closed` passes | P0 | |
| R4 | The environment surface is three names (§5) | `make test-unit-agentsfleet-runner` | exit 0, `test_runner_reads_only_the_bootstrap_environment` passes | P0 | |
| R5 | No removed variable survives (§5) | `git grep -nE 'RUNNER_(SANDBOX_TIER\|NETWORK_POLICY\|REGISTRY_ALLOWLIST\|WORKER_COUNT\|HOST_ID\|CP_[A-Z_]+_MS)' -- . ':!docs/v2/'` | 0 matches | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R7 | `macos_seatbelt` is gone (§6) | `git grep -rnw 'macos_seatbelt' -- . ':!docs/v2/' ':!schema/017_fleet_runners.sql'` | 0 matches | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | e2e walks the operator path | `make test-e2e` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted; `capability_probe.zig` is added.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `RUNNER_SANDBOX_TIER` | `git grep -n 'RUNNER_SANDBOX_TIER' -- . ':!docs/v2/'` | 0 matches |
| `RUNNER_NETWORK_POLICY` | `git grep -n 'RUNNER_NETWORK_POLICY' -- . ':!docs/v2/'` | 0 matches |
| `RUNNER_REGISTRY_ALLOWLIST` | `git grep -n 'RUNNER_REGISTRY_ALLOWLIST' -- . ':!docs/v2/'` | 0 matches |
| `RUNNER_WORKER_COUNT` | `git grep -n 'RUNNER_WORKER_COUNT' -- . ':!docs/v2/'` | 0 matches |
| `RUNNER_HOST_ID` | `git grep -n 'RUNNER_HOST_ID' -- . ':!docs/v2/'` | 0 matches |
| `RUNNER_CP_*_MS` | `git grep -nE 'RUNNER_CP_[A-Z_]+_MS' -- . ':!docs/v2/'` | 0 matches |
| `macos_seatbelt` | `git grep -rnw 'macos_seatbelt' -- . ':!docs/v2/' ':!schema/017_fleet_runners.sql'` | 0 matches |
| `RUNNER_WORKSPACE_BASE` (renamed) | `git grep -n 'RUNNER_WORKSPACE_BASE' -- . ':!docs/v2/'` | 0 matches |

## Out of Scope

- **Attestation — a runner that lies about its capability.** This workstream makes policy authoritative and catches *drift and misconfiguration*; it does not make the capability report *trustworthy*. A compromised host can accept `landlock_full`, apply nothing, and report success. Closing that needs measured boot, a hardware-rooted quote, or signed enforcement evidence — the identity workstream `src/lib/contract/protocol.zig` already defers. Nothing here should be described as making the tier verifiable.
- **A microVM / Firecracker tier.** Sequenced after this: once tiers denote genuinely different isolation strengths, the assignment path must already be authoritative. Adding the tier and inverting the configuration direction in one change would mean routing sensitive work on an unverified self-report during the transition.
- **Per-tenant secret delivery.** `SecretDelivery = inline` puts tenant secrets in the runner's address space; the `scoped`/`proxy` modes are already reserved in the protocol. Separate workstream.
- **Tightening the tailnet network grants.** The `grants` block is still `src:* dst:* ip:*`, so an escaped process reaches every node at the Internet Protocol (IP) layer. Named in M147_001; still open; unrelated to policy delivery.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator sets a runner to `landlock_full` in the dashboard, and within one heartbeat the runner row shows it applied. When the host cannot do it, the row says degraded and names the missing mechanism instead of showing a green badge over a runner that refuses every job.
2. **Preserved user behaviour** — Enrolling a runner is unchanged: mint a token, drop it on the host, start the daemon. Existing runners keep leasing, renewing, reporting, and draining exactly as they do now. The runners list keeps its current shape and gains fields.
3. **Optimal-way check** — The unconstrained-optimal shape is a runner whose isolation is cryptographically attested rather than merely assigned. The gap is that assignment is trusted, and it is acceptable now because it removes the two-sources-of-truth failure and the silent-degradation failure, which are the ones actually biting.
4. **Rebuild-vs-iterate** — Iterate. The lease, renew, and report loop is sound; only the direction of configuration is wrong. A rebuild would trade a contained change for churn across the whole runner.
5. **What we build** — Assigned policy on the runner row, delivery on enrollment and heartbeat, a startup capability probe, a reconciliation verdict with a degraded state, dashboard surfacing, and an environment surface of three names.
6. **What we do NOT build** — Attestation, a microVM tier, per-tenant secret delivery, tailnet grant tightening. Each is named in Out of Scope with its reason.
7. **Fit with existing features** — Compounds with the runner enrollment flow and the runners list. It must not destabilize the lease loop: policy is read at lease boundaries, never mid-execution.
8. **Surface order** — Both, and the dashboard leads: the operator-visible failure is that the dashboard lies. The runner change is what makes the dashboard honest, so they ship together.
9. **Dashboard restraint** — Show the degraded state only when a real capability report contradicts a real assignment. No isolation-strength scoring, no "hardened" badge, and no attestation language until attestation exists.
10. **Confused-user next step** — The runner row names the missing mechanism (for example "cgroup controllers not delegated"), which maps to a step in the runner bootstrap playbook. No ticket required.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Five Sections split by direction of travel — policy down (§1), the runner obeying it (§2), capability up and reconciled (§3), the operator seeing it (§4), and the old input surface removed (§5). §1 and §2 must land together or the runner reads a field nobody sends; §3 is independently valuable and is the piece that catches the M147_001 class of bug; §5 is the cleanup that makes the inversion irreversible.
- **Alternatives considered:**
  - *Keep the environment variables as an override.* Rejected: an override path is exactly how the two sources of truth diverged, and pre-2.0.0 carries no compatibility obligation. A fallback would preserve the bug this workstream exists to remove.
  - *Validate the self-reported tier instead of assigning it.* Rejected: it keeps the host as the source of truth and only adds a check, so an operator's dashboard selection would still not drive behaviour.
  - *Wait and do this together with Firecracker.* Rejected: the assignment path must be authoritative before tiers mean different isolation strengths, and bundling them makes the security-relevant change harder to review.
- **Patch-vs-refactor verdict:** this is a **refactor**, deliberately. Match to problem size: the defect is structural — configuration flows the wrong way — and a patch that merely reconciles the two values on read would leave both sources in place.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
  - > Indy (2026-07-30): "Ideally what i select in the Runner must be the one that is used by the runner. and get rid of these gazillion envs?" — context: the origin of this workstream, raised while reviewing why `RUNNER_SANDBOX_TIER` had no effect on runner behaviour during M147_001.
  - > Indy (2026-07-30): "I prefer option 1 - keep it optional with RUNNER_WORKER_BASE to be renamed to RUNNER_HOME? … Should this move as a configurable option as started by the other worktree?" — context: env surface. Resolution (Orly's call, stands unless Indy overrides): the name stays `RUNNER_WORKSPACE_BASE` — it holds disposable per-lease scratch, not the runner's install home (`/opt/agentsfleet`), and "HOME" would invite pointing it at the install dir. It also stays host-local, never control-plane-assigned: the control plane cannot see a host's disks, and a wrong assigned path would brick leasing.
  - > Indy (2026-07-30): "All four fields in dialog" — context: Add Runner exposes isolation, network policy, registry allowlist, and workers (Dimension 4.4); network defaults to `allow_all` until 2.0.1's egress wiring.
  - > Indy (2026-07-30): "…an unset or unrecognized RUNNER_NETWORK_POLICY resolves to the fail-closed default allow_list_egress … deliberately refused at src/runner/child_supervisor.zig:133-139 … the unit comment still claims 'allow_all is the current default' — stale since the M100 flip … as an issue, i assume this will go into the db as well." — context: confirmed against the code; the capability report gains `egress_enforcement`, and an assigned `allow_list_egress` reconciles to a degraded row whose reason lands in the database and on the dashboard (§3, Failure Modes). The stale unit comment is cleaned in §5.
  - > Indy (2026-07-30): "In the isolation we must remove macos_seatbelt, since its not valid" — context: §6 added; consistent with the M80_004 / M84_003 deferrals ("seatbelt is deprecated long back"). Removal, not deprecation (NLG).
  - > Indy (2026-07-30): "Okay keep the directory as RUNNER_STORAGE_HOME?" — context: supersedes the keep-the-name resolution above; the optional env is renamed `RUNNER_WORKSPACE_BASE` → `RUNNER_STORAGE_HOME`. Host-local semantics unchanged; the old name joins the Dead Code Sweep.
  - > Orly (Jul 30, 2026, CHORE(open)): spec instance amended to repo reality before EXECUTE — Interfaces corrected to the frozen `me`-plane paths (`protocol.zig` forbids a runner_id in any path); read-list item 5 repointed at `docs/v2/active/` where M147_001 actually sits; Files Changed extended with the files the R5 removal grep already hits (`deploy.sh`, three playbooks, `provision_runner_env_test.sh`) and the SQL/lease/fleet surfaces the new columns must cross. Also flagged, untouched: M147_001's spec is still `IN_PROGRESS` in `active/` though `enableDelegatedControllers` is merged to main and the branch pruned.
- **Metrics review** — three operator events declared above; no analytics or funnel playbook update required, as no end-user funnel changes.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
  - > Indy (2026-07-30): "Keep going, i want to manage via dahsboard, so go. Dont unbrick with allow_all, yes the allow_list_egress can be moved a different workstreem" — context: (a) the `EgressScope` kernel enforcement for `allow_list_egress` is Indy-acked as a separate follow-up workstream, not part of this one; (b) the stuck dev worker is NOT to be unbricked via a `RUNNER_NETWORK_POLICY=allow_all` env line — the dashboard assignment this workstream ships is the fix path.
