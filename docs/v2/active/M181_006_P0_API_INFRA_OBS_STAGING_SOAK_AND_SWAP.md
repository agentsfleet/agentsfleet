<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M181_006: Staging soak, rollback rehearsal, and the one-move production swap

**Prototype:** v2.0.0
**Milestone:** M181
**Workstream:** 006
**Date:** Sep 01, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — the family's payoff; every sibling exists so this one is boring
**Categories:** API | INFRA | OBS
**Batch:** B8 — family closer, strictly serial: every dimension needs the merged whole on staging
**Branch:** `feat/m181-006-soak-and-swap`
**Test Baseline:** unit=7045 integration=361 — from the declared `verify.unit` (`make test-unit-all`: rustd 2324 + app 2410 + cli 1624 + design-system 512 + website 175) and `verify.integration` (`make test-integration-rustd`), both exit 0 at `c000da206`
**Depends on:** M181_002 **merged** (the full route surface); M181_003 **merged** (the coverage gate — the parity roster's contract is generated, not hand-kept); M181_004 **merged** (the export — continuity is unprovable without it); M181_005 **merged** (the collectors — the path continuity is graded through)
**Provenance:** LLM-drafted (Claude Opus 5, Sep 01, 2026) — §3 and §4's swap half of M181_002, split out on Indy's parallelization call; section prose carried over, not re-derived
**Canonical architecture:** `docs/architecture/observability.md` §The three signal paths + `docs/architecture/runner_fleet.md` §Multi-replica + `docs/architecture/scaling.md`

---

## Overview

> **PREMISE CORRECTION (Sep 02, 2026) — the rollback story below is void, and
> it is load-bearing.** This spec is written around swapping production from a
> Zig daemon to a Rust one, keeping the Zig binary warm as the one-move
> rollback. Both halves are already false:
>
> - **The swap has happened at the artifact level.** `Dockerfile:39` copies
>   `dist/agentsfleetd-rs-linux-${TARGETARCH}`, built by
>   `cargo build --profile dist --bin agentsfleetd` (`release.yml:110,308`).
>   Production serves the Rust binary today, so the Goal's "serve the Rust
>   binary after a staging soak" is already true.
> - **No Zig daemon is built anywhere.** Every `zig build` in every workflow
>   passes `--build-file build_runner.zig`, whose only executable is
>   `agentsfleet-runner` (`build_runner.zig:102`); no Zig daemon artifact
>   reaches `dist/`. §Discovery's own correction of the parent family — "the
>   release workflow and the staging deploy workflow each build the Zig binary
>   today" — is itself wrong, so the rollback story this spec picked rests on
>   the false half of the two it was choosing between.
>
> What that invalidates, specifically: Invariant 2's single documented rollback
> mechanism and Invariant 3's `test_rollback_artifact_builds` are
> unsatisfiable, because there is no artifact to keep buildable; and Files
> Changed's "publish the Zig one as the rollback artifact" would mean creating
> a build that does not exist, inside a tree M187_001 §5 deletes.
>
> **Disposition — APPLIED at CHORE(open), Sep 04, 2026.** The acceleration was
> deliberate. This spec is now written as a soak of the Rust daemon with NO Zig
> rollback: rollback is the previous Rust image digest, which the registry
> retains and which `release.yml:622` already deploys immutably
> (`flyctl deploy --image ghcr.io/agentsfleet/agentsfleetd@${digest}`) rather
> than by tag. The runbook had already been corrected ahead of the spec —
> `playbooks/operations/cutover/001_playbook.md:57-59` reads "drain, serve the
> previous image digest… the registry retains the digest" — so this rewrite
> brings the spec into line with the procedure, not the other way round.
> M187_001's Invariant 1 and its §5 gate read this spec's soak, so they inherit
> the same correction; the consequence for M187_001 §5.2 is named in Invariant 3.
>
> > Indy (2026-09-02): "Its not accidental, Indy accelerated it, so just record
> > it and move on" — context: asked whether the missing Zig daemon build was
> > an accident that left production without a rollback artifact, or the
> > cutover having already landed. It is the latter, by intent.
>
> **SECOND CORRECTION (Sep 04, 2026) — the collector app was never created, and
> it is why the development deploy is red.** M181_005 shipped
> `deploy/fly/otelcol-{dev,prod}/` and both workflow steps, and closed with its
> own R1 ungraded ("⏳ blocked… graded from the change window, which is
> Indy's"). Nothing ever created the Fly apps those steps address:
> `playbooks/founding/03_priming_infra/001_playbook.md:41-46` creates four apps
> — `agentsfleetd-{dev,prod}` and `cloudflared-{dev,prod}` — and neither
> `otelcol-dev` nor `otelcol-prod`. `scripts/ensure_fly_app.sh` runs `deploy`,
> `scale count`, `machine list` and `image show`, never `apps create`. And the
> step ahead of it, `deploy-dev-fly.yml:74-80`, runs
> `flyctl secrets set --app otelcol-dev`, which fails first on an app that does
> not exist.
>
> This is load-bearing for THIS spec, not a neighbour's bug: every dimension
> below runs against the development environment, and §2.4's signal continuity
> is graded through the collector hop. So the fix is §0 here rather than a
> follow-up — the soak cannot start behind a red deploy.
>

**Goal (testable):** the three production `agentsfleetd` machines are PROVEN to serve the Rust binary — a soak in which the black-box parity suite, the runner parity lane, the dry lane, and the latency and memory budgets all pass against the Rust daemon, a rehearsed one-move rollback to the previous image digest, and metric families continuous across that rollback boundary. The swap itself already happened by acceleration; what this spec adds is the evidence it outran.

**Problem:** six milestones of parity evidence are per-surface. Cutover needs whole-system proof — all routes at once, sustained load, memory over hours, dashboards continuous — plus an exit that is boring: same schema, same stores, binary swap back.

**Solution summary:** stand the collector app up so the development deploy goes green (§0); run the soak against the budgets M181_001's lanes already refuse to run without; rehearse the digest rollback on development before touching production; close the probe runner's coverage gap so the runbook grades M175–M181 rather than M175–M179; and record the evidence in the runbook sections M181_002 left tagged for a milestone that closed without filling them.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): cutover — soak evidence, rollback rehearsal, and the production swap
- **Intent (one sentence):** production traffic moves to the Rust daemon behind whole-system proof, with rollback reduced to serving a binary that still speaks the same schema.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `playbooks/operations/cutover/001_playbook.md` + `probes.sh` — the runbook and its probe runner; every rubric row here is probe-tagged or manifest-declared.
2. `make/test-parity.mk` + `scripts/parity_lane.sh` — RECORD and COMPARE modes; COMPARE is the state-handoff oracle.
3. `docs/architecture/runner_fleet.md` §Multi-replica — the 3-machine shape, which gauges stay approximate across replicas, and why counters stay exact.
4. The declared-divergence register M181_001 seeded — a parity differ reads a declared difference as declared and an undeclared one as a regression.
5. M181_002's Discovery — the family's decision record; nothing here re-opens it.
6. `scripts/ensure_fly_app.sh` + `.github/workflows/deploy-dev-fly.yml` §deploy-fly — §0's surface. Read the step ORDER before the script: the secrets step addresses the app first and is what actually fails.
7. `docs/v2/done/M181_005_P0_INFRA_OBS_COLLECTOR_HOP.md` §Acceptance Rubric — R1 closed ungraded and is graded from §0's run, not re-derived.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `deploy/**` | EDIT | drain-swap steps for the 3-machine shape; the serving-binary selection knob |
| `playbooks/operations/cutover/001_playbook.md` | EDIT | swap rows, verification probes, abort criteria, the ONE rollback story, evidence sections |
| `playbooks/operations/cutover/probes.sh` | EDIT | probes for this spec's rubric rows |
| `make/test-parity.mk` | EDIT | the soak's route corpus, now that every route serves and the contract is generated |
| `docs/architecture/runner_fleet.md` | EDIT | production-shape note — serving binary and rollback posture |
| `.github/workflows/deploy-dev-fly.yml` · `.github/workflows/release.yml` | EDIT | **§0, needs Indy's approval before it lands** — order the collector stand-up so an absent app is created rather than fatal: today `flyctl secrets set --app "$OTLP_COLLECTOR_APP"` runs first and fails on an app that does not exist |
| `scripts/ensure_fly_app.sh` · `scripts/ensure_fly_app_test.py` | EDIT | §0 — create-if-absent, so the script's contract covers a first deploy instead of assuming someone ran `fly apps create` by hand; the self-test covers the new branch |
| `playbooks/founding/03_priming_infra/001_playbook.md` | EDIT | §0 — the two `fly apps create otelcol-{dev,prod}` lines that were never added, and the step summary that still says "four Fly.io apps" |
| `playbooks/operations/cutover/coverage.tsv` | EDIT | §3 — declare M180_001 and M181_001–005 and cover or exclude their 26 rubric rows, so Invariant 7 grades M175–M181 as it claims rather than M175–M179 |
| `rustd/crates/{afd_crypto,afd_core,afd_auth,afd_identity,afd_redis}/tests/**` | EDIT | §4.1 — eight parity tests read the Zig tree from disk; expectations frozen inline and proven green BEFORE the deletion |
| `src/agentsfleetd/**` | DELETE | §4.2 — the Zig daemon. `Dockerfile:39` ships the Rust binary and no workflow builds this tree |
| `build.zig` | EDIT | §4.3 — six references die with the tree, including `S_SRC_MAIN_ZIG` and the `test-auth` target |
| `src/build/auth_tests.zig` | DELETE | §4.3 — the `test-auth` gate's support file; its only scope was the deleted daemon's auth directory |
| `rustd/crates/**` · `cli/**` · `docs/**` · `audits/**` · `dispatch/**` · `playbooks/**` | EDIT | §4.2 — the reference sweep: 92 files name the deleted tree, most citing it as canon in prose |
| `rustd/crates/{afd_wire,afd_api_operator,afd_api_tenant,afd_library,afd_runner}/**` | EDIT | §5 — the bound each request field carries moves onto its request type; the sentence a break earns stays at the handler |
| `src/runner/**` · `build_runner.zig` | **UNTOUCHED — asserted, not assumed** | §4.5 grades that the deletion did not take the runner with it |
| `rustd/crates/agentsfleetd/**` | EDIT | only what the soak proves it needs — a startup or shutdown ordering fix the drain-swap surfaces, a budget-driven change the latency or memory dimension forces. No feature work: a soak that changes the daemon it is measuring has measured nothing |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — TIM (budgets are named numbers, never widened to pass), ECL (a provider outage mid-soak is not a parity defect), UFS (knob and budget names), ORP (no orphaned runbook rows).
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the runbook and architecture edits are published prose.
- `dispatch/write_shell.md` — probe additions: quoted expansions, no untrusted `eval`.
- `dispatch/verify.md` — every done-claim here is a rubric row; no package-scoped substitutes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| CI/CD edit approval | yes | `deploy/**` and any workflow edits need Indy's explicit approval — sought at PLAN |
| LENGTH / UFS | yes | probes and make edits under caps; budgets as named constants |
| MILESTONE-ID | yes | none in source; runbook is docs (exempt) |
| LOGGING | no | no daemon source changes |
| SCHEMA GUARD | no | no schema change — that is the rollback story |

## Prior-Art / Reference Implementations

- **Reference:** `.github/workflows/deploy-dev*.yml` + `deploy/` — the staged deploy, verify and acceptance shape the swap reuses.
- **Reference:** the M175–M181 rubrics — every per-surface oracle re-runs here as a pre-swap checklist; this spec adds only whole-system proofs.

## Sections (implementation slices)

### §0 — The collector app exists, and the deploy stops depending on someone having created it by hand

First, because nothing else in this spec can run behind a red deploy. The
development deploy fails at **"Ensure the OTLP collector is running"** for a
reason that is one missing line rather than a defect in anything shipped:
`otelcol-dev` and `otelcol-prod` were never created. The priming playbook
creates four Fly apps and says so in its own step summary; the collector apps
M181_005 introduced are not among them, and no script creates them either.

**Two fixes, and the second is the one that lasts.** Creating the apps unblocks
today. Making `ensure_fly_app.sh` create-if-absent is what stops the next fresh
environment hitting the same wall — the script's whole contract is refusing to
report a success it cannot prove, and "the app does not exist yet" is a state it
should resolve rather than die on. Ordering matters too: the secrets step runs
BEFORE the ensure step and fails first, so fixing only the script would leave
the failure exactly where it is.

**Both halves touch Fly and CI and are gated on Indy's explicit approval**
(Indy, Sep 03, 2026: wait for approval before Fly.io updates and before the CI
deploy-gate fix). The playbook edit is docs and proceeds; the workflow, script
and any `fly apps create` wait.

- **Dimension 0.1** — **DONE** — the priming playbook creates the collector apps alongside the other four, and its step summary counts them. The test derives the expected set from `deploy/fly/*/` rather than a list, so the next app added without a playbook line fails here → Tests `test_every_fly_app_is_created_by_the_priming_playbook` · `test_the_playbook_counts_the_apps_it_creates`
- **Dimension 0.2** — **DONE** — `ensure_fly_app.sh` creates an app that does not exist rather than failing, still refuses when creation itself fails, and gains a `--create-only` mode for the ordering constraint in 0.3 → Tests `test_an_absent_app_is_created_before_the_deploy` · `test_a_failed_creation_refuses_rather_than_falling_through` · `test_create_only_creates_without_deploying` · `test_create_only_is_idempotent` · `test_an_existing_app_is_not_recreated` · `test_create_only_rejects_a_wrong_argument_count`
- **Dimension 0.3** — **BLOCKED at the harness, not at the design** — the collector stand-up survives a first run: no step addresses the app before it can exist. The `--create-only` call 0.2 shipped is what the workflow must invoke ahead of `flyctl secrets set`. Each deploy workflow needs two edits — the `--create-only` call ahead of `flyctl secrets set`, and `FLY_ORG` set to that environment's organisation, since the org default was removed. Both are refused by the auto-mode CI/CD classifier, which does not read Indy's in-session approval; the exact diff is in the PR body for a human to apply → Test `test_collector_standup_is_order_safe`
- **Dimension 0.4** — the development deploy reaches green, and M181_005's R1 — closed ungraded — is graded from that run → Test `test_collector_carries_every_signal`

### §1 — Staging soak with budgets

The whole-system proof on staging: the black-box HTTP parity suite, the runner parity lane and the dry lane against the Rust daemon; sustained mixed load through the benchmark lane; chaos probes for the invariant tables — webhook replay, lease fencing under kill, stream reconnect; and the two budgets M181_001 embedded as constants the lane refuses to run without.

**The Zig integration corpus cannot grade the Rust daemon.** Three independent reasons, each checkable in one command: the lane was deleted with the Zig gating; the tests are in-process, importing Zig modules directly, so there is no HTTP boundary to repoint; and nothing in them names a daemon — the only environment knobs they read are datastore pointers. A green run against a Rust-served environment would report a pass rate for the implementation being retired — worse than no number, because it reads like evidence. M181_001's black-box lane is what replaces it.

**There is no Zig-side baseline, because there is no Zig daemon build.** An
earlier draft of this section said the Zig binary "is still built by the release
workflow and by the staging deploy workflow". That is false and was checkable in
one command: every `zig build` in every workflow passes
`--build-file build_runner.zig` (`release.yml:163,260`,
`deploy-dev-build.yml:36`), whose only executable is `agentsfleet-runner`. The
comparison this paragraph described cannot be run and is not attempted; the
parity lane's COMPARE mode is instead pointed at two Rust image digests, which
is the comparison that matches the rollback we actually have.

**The rollback window has an END, and naming it is still this spec's job** —
what changed is what sits inside it. The window holds the PREVIOUS RUST IMAGE
DIGEST, not a second implementation, so it costs the repository nothing to keep
and needs no expiry of its own. `src/agentsfleetd` is not the rollback and has
not been since the acceleration; its deletion is M187_001 §5's, gated on that
spec's end-to-end acceptance rather than on anything here.

- **Dimension 1.1** — the black-box parity suite, the runner parity lane and the dry lane are green against the Rust daemon on staging → Test `test_soak_suites_green`
- **Dimension 1.2** — per-route-class latency is within the budget the lane embeds → Test `test_latency_budget`
- **Dimension 1.3** — resident memory stays within the named ceiling across the soak window under sustained load, with a flat growth trend → Test `test_memory_ceiling_soak`
- **Dimension 1.4** — chaos probes hold mid-soak: replay suppressed, leases fenced, streams reconnect → Test `test_soak_chaos_invariants`
- **Dimension 1.5** — cross-DIGEST state handoff in both directions: the current image serves and writes production-shaped state, the previous digest then boots on the same stores and resumes serving correctly, and the reverse. Rollback safety is demonstrated, not inferred from "same schema". Graded by the parity lane in COMPARE mode across each direction — `make test-parity BASE_URL=<previous-digest> COMPARE_URL=<current>` — rather than by a bespoke lane whose only caller would be one rubric row. The cross-IMPLEMENTATION version of this dimension is deleted, not deferred: it named a Zig daemon that is not built → Test `state_handoff_is_bidirectional`

### §2 — Rollback rehearsal and the swap

Rollback is rehearsed BEFORE cutover: staging swaps back to the previous image digest using the runbook and verifies clean service — there is no Zig binary to swap to, and the paragraph after this one says why. Production cutover is all-at-once across the three machines with load-balancer drain — mixed-fleet operation is structurally tolerated, since every cross-replica invariant is atomic in the datastores, but it doubles the drift surface, so it is the contingency rather than the plan.

**The rollback story is settled, and neither of the family's two candidates won.** The parent family carried two incompatible statements — that the Zig binary remains built, shipped and warm as the rollback, and that rollback is a hand-dispatched redeploy of a frozen revision no longer built by CI. The first is false: no workflow builds a Zig daemon. The second is false in a different way: rollback is not hand-dispatched, because `release.yml:622` deploys by immutable digest, so every previously shipped image is already addressable. The runbook states the surviving story once (`001_playbook.md:57-59`) and this spec no longer proposes a deploy knob that chooses between two binaries — there is one binary and a list of its digests.

The runbook carries the declared-divergence register, and the rollback path carries **no migration invocation**: rollback serves an older binary against a ledger it already understands, and a migration there is at best a no-op and at worst the one command that can refuse mid-incident. The probe runner asserts the absence rather than trusting the prose.

- **Dimension 2.1** — **DONE — closed on Indy's call, Sep 04, 2026, ungraded (see Discovery)** — rollback rehearsal on staging: swap back, verify, recorded in the runbook's evidence section → Test `test_rollback_rehearsal`
- **Dimension 2.2** — **DONE — closed on Indy's call, Sep 04, 2026, ungraded (see Discovery)** — an older binary pointed at a newer ledger refuses rather than reaping, and the rollback path invokes no migration → Test `test_rollback_carries_no_migrate_and_refuses`
- **Dimension 2.3** — **DONE — closed on Indy's call, Sep 04, 2026, ungraded (see Discovery)** — every runbook probe is a copy-paste command that passes on staging post-swap, and every rubric row of the merged milestones is probe-tagged or manifest-declared → Test `test_runbook_probes`
- **Dimension 2.4** — **DONE — closed on Indy's call, Sep 04, 2026, ungraded (see Discovery)** — metric, span and log families are continuous across the swap: no renamed series, no dropped family, dashboards unbroken → Test `test_signal_continuity`

The production swap itself is operator-executed from the runbook — the agent prepares and rehearses; Indy pulls the trigger, and the swap additionally requires Indy's explicit go recorded in Discovery.

### §3 — The probe runner grades M175–M181, which is what it already claims

Invariant 7 says cutover cannot proceed with any M175–M181 rubric row ungraded.
The runner cannot enforce that today: `coverage.tsv` declares M175_001 through
M179_001 and stops. It passes green on 57 rows while three milestones and this
spec's four siblings are invisible to it — a vacuous pass of exactly the kind
Dimension 5.3 of M187_001 exists to catch, one milestone early.

Measured, not estimated: declaring the missing milestones against a scratch copy
of the manifest turns 0 failures into **26**, spread `M180_001` 5, `M181_001` 7,
`M181_002` 4, `M181_003` 5, `M181_004` 3, `M181_005` 2. Each is then probed or
excluded with a reason — the manifest prints on every run, so a skip cannot
become invisible by ageing.

- **Dimension 3.1** — **DONE** — every merged milestone through M181 is declared, and each of its rubric rows is probed or carries an exclusion reason. 57 rows → **119**, with seventeen new probes running each row's own Verify line verbatim → Test `test_probe_runner_row_coverage` (existing, over the widened roster)
- **Dimension 3.2** — the runbook's three sections tagged `M181_002` name the milestone that actually owns them; M181_002 closed having explicitly moved that work here → Test `test_runbook_has_no_orphan_owner_tag`
- **Dimension 3.3** — the runbook's evidence tables are filled from real runs, not left as empty rows → Test `test_runbook_probes`


### §4 — The Zig daemon is sunset

Moved here from M187_001 on Indy's call (Sep 04, 2026). That spec gated the
deletion on end-to-end fleet acceptance because the tree was the rollback; the
tree stopped being the rollback at the acceleration, so the gate was protecting
a property that no longer existed. What remains is a reference implementation,
and Indy keeps a clone outside the repository for that. M187_001 keeps the live
fleet verification, which is what it is actually for.

**The blast radius is 92 files, not one directory.** Measured with
`git grep -l "src/agentsfleetd"` under M187_001's own R8 exclusions: 27 Rust
files, 15 CLI files, 12 docs, 4 gates, 3 skill trees, `build.zig`, and the rest
across playbooks and root. M187_001's Files Changed named the directory and the
workflows and nothing else, which is why this is a section rather than a line.

**Eight of those are load-bearing, and they decide the order.** Rust parity
tests read the Zig source FROM DISK at test time — `read_to_string(...).unwrap()`
against `crypto_primitives.zig`, `error_registry.zig`, `queue/constants.zig`
and five more. They panic on a missing file, so deleting the tree first turns
`make test-unit-all` red for a reason unrelated to whatever else the diff did.
They are converted BEFORE the deletion, and proven green while the tree still
stands, or the conversion is not proven faithful — only compiling.

Those tests are not deleted with the tree. They pin crypto primitives, error
codes, scope catalogues and Redis keys; the Zig file was where the expectation
came from, not what made it worth asserting. Once Zig is gone Rust is canon, so
the expectation is frozen inline and the assertion survives. Deleting them
would trade a dangling dependency for a coverage hole.

- **Dimension 4.1** — **DONE** — every Rust test that read the Zig tree from disk asserts the same values from a frozen expectation, and is green with the tree still present → Tests the eight converted files, run before §4.2 lands
- **Dimension 4.2** — **IN_PROGRESS** — `src/agentsfleetd/**` is removed and no reference survives outside `docs/v2/done/` and `docs/v1/` → Test `no path under src/agentsfleetd is referenced after the deletion`
- **Dimension 4.3** — **DONE** — `build.zig` loses its daemon targets and still builds what it should: six references die with the tree, including `S_SRC_MAIN_ZIG` and the `test-auth` gate that reaches the daemon's auth through `src/build/auth_tests.zig`. A support file left addressing a deleted directory is removed with its caller rather than left orphaned → Test `the default build file declares no daemon target`
- **Dimension 4.4** — **IN_PROGRESS** — every gate, make target and playbook whose scope was the daemon either narrows or is removed, and none is left scanning nothing and reporting green. A gate covering daemon AND runner narrows; only an empty scope is deleted → Test `no gate reports a vacuous pass over a deleted tree`
- **Dimension 4.5** — **DONE** — the runner is untouched and still ships: `build_runner.zig` and `src/runner/**` unmodified, `compile-runner-amd64` and `compile-runner-arm64` still declared and still in the release job's `needs`, deploy stages still consuming `agentsfleet-runner-linux-amd64`. Zig does not leave this repository — the runner stays Zig by Indy's Sep 02 call, so a sweep that greps for `.zig` and expects zero is wrong by construction → Tests `the runner build still produces its artifact` · `the release workflow still builds and ships the runner`


### §5 — Request bounds are declared on the type, not re-spelled per handler

Not planned here. The soak could not start until three Rust-port defects were
repaired, and the credential-rule fix in `afd_library` was the first place a
bound was moved onto its request type rather than re-checked beside it. The
same shape was then carried across the request surface, on Indy's call, as ONE
commit — the alternative being a rule that holds in the crate the defect was
found in and nowhere else.

The rollout GROWS the code and that was agreed before it began: roughly −6 lines
of `if`, +1 per field, +12 for the sentence mapping ≈ +8 net per request struct.
`garde` reports a stringly `(path, message)`, while these handlers answer a
specific public sentence per field and sometimes a different error CODE, so each
conversion keeps a `detail_for` mapping and the wire text stays byte-identical.
Nothing a caller reads changes; what changes is that a cap has one home.

What is deliberately NOT converted is a finding, not a gap: newtype smart
constructors (`afd_tenant/apikey/name.rs`, `afd_vault/secret.rs`), parsers
(cursors, cron expressions, tokens, `Repository::parse`), URL path parameters
and raw `Bytes` bodies with no struct to hang a bound on, internal capacity
guards (`afd_observability`), and one handler-local struct with a single
sentence (`afd_api_ingress` approval webhook) whose conversion would add a
dependency to buy no shared bound. A newtype constructor IS the idiomatic Rust
pattern; converting it would make it worse.

- **Dimension 5.1** — **DONE** — every request type whose fields carried a hand-rolled length or emptiness check declares that bound with `garde` instead: eleven types across `afd_wire`, driven from `afd_api_operator`, `afd_api_tenant`, `afd_library` and `afd_runner` → Tests `patch_validation_covers_identity_and_reason_bounds` · `test_the_model_bound_holds_on_both_verbs_that_take_one` · `test_a_document_is_refused_when_empty_or_past_the_cap_and_kept_at_it`
- **Dimension 5.2** — **DONE** — the refusal SENTENCE each field earns is unchanged, mapped from the path `garde` reports rather than from garde's own message, including the two fields that share one cap and answer different copy → Tests `test_a_create_naming_no_credential_is_refused_before_the_store` · `should_admit_the_ceiling_and_refuse_one_byte_past_it`
- **Dimension 5.3** — **DONE** — a cap has exactly one declaration, and the cases that assert it read it from there. `FLEET_MARKDOWN_MAX_BYTES` carries the reason its number and its sentence disagree, so neither is reconciled by a later reader without a decision about the client between them → Tests `test_a_document_is_refused_when_empty_or_past_the_cap_and_kept_at_it` (at-cap and one-past arms) · `test_the_model_bound_holds_on_both_verbs_that_take_one` (inclusive-edge arm)
- **Dimension 5.4** — **DONE** — the credential rule reads the PARSED document rather than a raw byte window, so a YAML comment documenting a credential shape is not onboarded as a leak, and a `client_secret` pasted below the closing fence still is → Tests the `afd_library` credential-scanner cases over both halves of the document


## Interfaces

```
Rollback target                   the previous image digest — ONE mechanism, named in the runbook.
                                  `flyctl deploy --image ghcr.io/agentsfleet/agentsfleetd@<digest>`
                                  There is no serving-binary knob: there is one binary and a list of its digests.
Runbook                           playbooks/operations/cutover/001_playbook.md — drain order, probes,
                                  abort criteria, one-move rollback, divergence register
make test-parity                  RECORD (BASE_URL) and COMPARE (BASE_URL + COMPARE_URL) modes
make bench-cutover · make dry-app-rustd   the budget and dry lanes M181_001 shipped
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Collector app absent | the Fly app the deploy addresses was never created | the deploy fails at `flyctl secrets set --app otelcol-dev` before `ensure_fly_app.sh` runs; §0 creates the app and the playbook grows the line that was missing |
| Soak suite red | a parity defect surviving M175–M180 | the defect routes back to its owning milestone's surface; the soak is not declared green with a known red lane |
| Latency budget miss | a route class over tolerance | cutover blocked; profile, fix, re-soak; the budget is never widened to pass |
| Memory growth in soak | an unbounded buffer or a leaked task | cutover blocked; the trace is attached; fix and re-soak |
| Mid-swap abort | a probe fails on the first machine | abort criteria trigger rollback of the touched machine; a mixed fleet is tolerated structurally while recovering |
| Post-cutover regression | a defect visible only under production traffic | one-move rollback: deploy the previous image digest; the schema is untouched by design |
| Older binary meets a newer ledger | a rollback or stale image whose migration set predates the database's | the daemon REFUSES, naming the version it does not know, and changes nothing; a rollback that trips this crossed a migration boundary and is not one-move |
| State-handoff regression | the previous image digest cannot read or resume state the current one wrote, or the reverse | the handoff lane goes red and the rollback is not one-move; serialization fixed before the window is declared closed |
| Dashboard discontinuity | a renamed or dropped series | blocked at §2; series names are parity surface, fixed before the swap |

## Invariants

1. Rollback requires no schema or data migration — the family rule that no `schema/` change lands in M175–M181, plus `test_rollback_rehearsal`; the daemon enforces it by refusing a ledger it does not know. The invariant is only cheap while the family rule holds — the first post-cutover migration makes rollback across that boundary a schema decision.
2. The rollback is reachable by exactly ONE documented mechanism, named in the runbook — deploy the previous image digest — and `test_rollback_rehearsal` proves it by doing it. Carrying two descriptions is the failure this invariant prevents, and the family carried two.
3. The rollback target is an image digest the registry retains, never a build — so "is the rollback still buildable" is not a question this spec can be asked. The previous invariant asserted a Zig daemon artifact stayed buildable; no such artifact is produced by any workflow, so the assertion was unsatisfiable rather than merely unmet, and it is deleted here rather than carried. **M187_001 §5.2 must be amended in step:** it promises to delete "M181_006's buildable-rollback invariant" in the same diff as the tree, and after this rewrite there is nothing there to delete.
4. Budgets are named constants compared mechanically, never prose judgments — `test_latency_budget`, `test_memory_ceiling_soak`.
5. Every declared divergence is in the register before cutover, and the parity oracles read it — a declared divergence never surfaces as a regression and an undeclared one always does.
6. Every runbook step carries an executable probe — `test_runbook_probes`.
7. Cutover cannot proceed with any M175–M181 rubric row ungraded or red — the probe runner's row-coverage assert: covered by a tagged probe, or named in the printed exclusion manifest; anything else is a red run.
8. No Rust test reads the Zig tree from disk after §4.1, and none loses an assertion getting there — a frozen expectation replaces a file read, never a deletion. §4.1 is green BEFORE §4.2 lands, so a red suite after the deletion is a real regression rather than a missing file.
9. Zig does not leave this repository. The runner stays, so `src/runner/**` and `build_runner.zig` are graded as survivors — a sweep that greps `.zig` and expects zero is wrong by construction.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| existing metric families, continuity asserted across the swap | ops | unchanged | unchanged | unchanged | `test_signal_continuity` |
| span and log families, continuity asserted across the swap | ops | unchanged | unchanged | no payload bytes, no credentials | `test_signal_continuity` |
| `deploy.serving_binary` (one label on existing deploy telemetry) | ops | deploy or swap | binary name, environment | none needed | `test_rollback_rehearsal` |

No product-analytics changes.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 0.1 | unit | `test_every_fly_app_is_created_by_the_priming_playbook` · `test_the_playbook_counts_the_apps_it_creates` | every `deploy/fly/<app>/` directory has a matching `fly apps create` line, and the prose count equals the number of create commands |
| 0.2 | unit | `an absent app is created before deploy` · `a failed creation refuses rather than falling through` | a fake `flyctl` reporting no such app leads to `apps create` then `deploy`; a creation that exits non-zero aborts |
| 0.3 | unit | `test_collector_standup_is_order_safe` | no workflow step addresses `$OTLP_COLLECTOR_APP` before the step that can create it |
| 0.4 | e2e (dev) | `test_collector_carries_every_signal` | metrics, traces and logs all arrive at Grafana Cloud through the collector; a partial pipeline fails |
| 1.1 | e2e (staging) | `test_soak_suites_green` | parity suite + runner lane + dry lane green against the Rust daemon |
| 1.2 | e2e (staging) | `test_latency_budget` | per-route-class p95 within the lane's embedded constants |
| 1.3 | e2e (staging) | `test_memory_ceiling_soak` | RSS under the named ceiling, flat growth trend over the window |
| 1.4 | e2e (staging) | `test_soak_chaos_invariants` | replay suppressed, leases fenced, streams reconnect, mid-soak |
| 1.5 | e2e (dev) | `state_handoff_is_bidirectional` | COMPARE mode green between previous and current image digests, in each direction, over shared stores |
| 2.1 | e2e (staging) | `test_rollback_rehearsal` | swap back, clean service, evidence recorded |
| 2.2 | integration | `test_rollback_carries_no_migrate_and_refuses` | old binary refuses a newer ledger; rollback path contains no migrate step |
| 2.3 | e2e (staging) | `test_runbook_probes` | every probe passes post-swap; row coverage complete |
| 2.4 | e2e (dev + production) | `test_signal_continuity` | no renamed series, no dropped family, across the rollback boundary |
| 3.1 | unit | `test_probe_runner_row_coverage` | every M175–M181 rubric row is probed or excluded with a reason |
| 3.2 | unit | `test_runbook_has_no_orphan_owner_tag` | no runbook section is tagged to a milestone that closed without filling it |
| 3.3 | e2e (dev) | `test_runbook_probes` | every probe passes post-rollback; evidence rows filled |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R0a | The collector apps exist and the dev deploy is green (§0) | `flyctl apps list \| grep otelcol` then the `deploy-dev` run | both apps resolve; the deploy's collector step passes | P0 | |
| R0b | The probe runner grades every merged milestone (§3) | `bash playbooks/operations/cutover/probes.sh --coverage` | exit 0 with M180_001 and M181_001–005 declared | P0 | |
| R1 | Whole-system soak green (§1) | `make test-parity BASE_URL=<rust-staging>` + `make dry-app-rustd` | exit 0 each | P0 | |
| R2 | Budgets hold (§1) | `make bench-cutover` | exit 0 | P0 | |
| R3 | Handoff bidirectional (§1) | `make test-parity BASE_URL=<previous-digest> COMPARE_URL=<current>` in each direction | exit 0 both runs | P0 | |
| R4 | Rollback rehearsed and probes green (§2) | `bash playbooks/operations/cutover/probes.sh` on staging, post-swap and post-rollback | exit 0 both runs | P0 | closed on Indy's call Sep 04, 2026 — not run; see Discovery |
| R5 | The daemon tree is gone (§4.2) | `test ! -d src/agentsfleetd` | exit 0 | P0 | |
| R6 | Nothing references it (§4.2) | `git grep -l "src/agentsfleetd" -- ':!docs/v2/done' ':!docs/v1'` | no output | P0 | |
| R7 | The runner still builds after it (§4.5) | `zig build --build-file build_runner.zig -Doptimize=ReleaseSafe && test -x zig-out/bin/agentsfleet-runner` | exit 0 | P0 | |
| R8 | The runner still ships (§4.5) | `grep -c compile-runner- .github/workflows/release.yml` | at least 3 | P0 | |
| R9 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration lane green | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Lint green | `make lint-all` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** S1–S5 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.unit`, `verify.integration`, `verify.lint`, `verify.version`); S6 is the template's hygiene gate; R-rows name the lanes M181_001 shipped and this spec drives.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE. The production swap additionally requires Indy's explicit go in Discovery.

## Dead Code Sweep

**This spec IS the sweep.** `src/agentsfleetd/**` is deleted in §4, with the 92
references that name it and the `build.zig` targets rooted at it.

The gate moved because the reason for it dissolved. M187_001 held the deletion
behind end-to-end fleet acceptance on the stated ground that the tree was the
rollback; it stopped being the rollback at the acceleration, and the previous
image digest is what Invariant 2 names. A gate protecting a property that no
longer exists is ceremony, and the tree would have outlived the family on the
strength of it.

> Indy (2026-09-04): "I think i prefer to process the sunset in this and keep
> 187_001 for a live fleet verification? Since there is no point keeping it. I
> will clone a copy and keep in another folder for reference." — context: asked
> whether this milestone sunsets the daemon; it did not, and M187_001's gate no
> longer had a live justification. The reference clone answers the one remaining
> reason to keep the tree in-repo, and M187_001 keeps the verification it is
> named for.

**Not swept, deliberately:** `src/runner/**`, `src/build/main.zig` and
`build_runner.zig` compile `agentsfleet-runner`, which `release.yml` builds,
requires and deploys, and which stays Zig. `src/build/auth_tests.zig` IS swept —
it supports the `test-auth` gate whose only scope is the deleted daemon's auth
directory, making it a caller-less support file rather than runner
infrastructure.

## Out of Scope

- Zig retirement — owned by M187_001. This spec no longer "opens a rollback window" in the sense the family meant: the rollback is a digest, not a second tree.
- Amending M187_001 §5.2, whose promise to delete this spec's buildable-rollback invariant no longer has a referent. The correction is NAMED in Invariant 3 so it cannot be lost, but editing a pending sibling's text is that spec's CHORE(open) work, not this one's.
- Behaviour changes, new dashboards, canary infrastructure beyond the selection knob.
- Everything the four sibling specs own: the route surface (002), the coverage gate (003), the export (004), the collectors (005).

## Product Clarity (authoring record)

1. **Successful user moment** — production runs the Rust daemon for a full day: fleets wake, leases complete, dashboards continuous, and nobody outside the team can tell anything changed. The rollback runbook stays unused.
2. **Preserved user behaviour** — everything; that is the entire spec.
3. **Optimal-way check** — an all-at-once swap with a rehearsed rollback beats a rolling mixed fleet: the invariants tolerate mixing, but a single boundary keeps triage unambiguous, and the canary path is named as the contingency in the runbook.
4. **Rebuild-vs-iterate** — N/A: ships proof and process, not new architecture.
5. **What we build** — the soak evidence, the rehearsal, the runbook's swap rows and probes, the swap itself.
6. **What we do NOT build** — anything a sibling owns; anything the runbook cannot probe.
7. **Fit with existing features** — rides the existing deploy and verify workflow shape; must not destabilize the runner's Zig release path, which is untouched by the cutover and stays Zig permanently (Indy, Sep 02, 2026).
8. **Surface order** — N/A — operational; no new user surface.
9. **Dashboard restraint** — nothing new to show; continuity is the deliverable, and a new panel at cutover would be indistinguishable from a regression.
10. **Confused-user next step** — an operator mid-incident opens the runbook; every step has a probe and an abort criterion, and the divergence register tells them what genuinely differs between the binaries.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** two slices — prove, then swap behind a rehearsal — the irreversible-adjacent step preceded by the thing that would have caught it.
- **Alternatives considered:** keeping soak and swap inside M181_002 (rejected on Indy's parallelization call: every dimension here is serial behind ALL siblings, so holding the route-surface PR hostage to it parallelizes nothing); a rolling per-machine cutover as the plan (rejected: doubles the drift surface for little gain; kept as the contingency).
- **Patch-vs-refactor verdict:** this is a **patch** to the operational layer — pipelines, lanes, runbook; the refactor was M176–M180 and the proof machinery is M181_001's.

## Discovery (consult log)

> Indy (2026-09-01): "i wanna see what can be batched parallelized and break to smaller PRs?" … "Yes, 5 specs as drawn" — context: §3 and §4's swap half of M181_002, split into the family closer; prose carried over; the collector-first step went to M181_005.

> Indy (2026-09-03): "I think the otel is not installed hence the dev deploy fails? Does this milestone deletes the agentsfleetd zig clean up as well? and tell me the playbook to run that sets up the otel-collector in flyway" — context: the hypothesis was right and there was no playbook to name, which is the finding. `playbooks/founding/03_priming_infra/001_playbook.md:41-46` creates four Fly apps and neither collector; nothing else creates them; `flyctl secrets set --app otelcol-dev` (`deploy-dev-fly.yml:74-80`) fails first on the absent app. Answered on the deletion too: this spec deletes nothing — M187_001 §5 owns `src/agentsfleetd`, and `src/runner/**` stays Zig permanently.

> Indy (2026-09-03): chose "Open M181_006 and fold it in" — context: asked whether to fix the collector gap as a standalone PR off main, fold it into M181_006, or hand over the commands only. Folding was chosen, so the collector repair is §0 here rather than a sibling.

> Indy (2026-09-03): "Also when you do fly.io updates to deploy the otel conllector, and fix the deploy gate of CI, wait for Indy's approval prior to the same." — context: a standing approval gate for this spec's §0. The playbook edit is docs and proceeds; `fly apps create`, the `deploy-dev-fly.yml` / `release.yml` ordering fix, and `scripts/ensure_fly_app.sh` all wait for an explicit go. This supersedes auto-mode for those paths.

> Indy (2026-09-03): "ensure the main files uncommitted are moved over to your new worktree" … "if you start … worktree ensure the main unmodified changes are pulled over as well (the spec we deleted)" — context: `main` had no uncommitted or untracked files at CHORE(open) (`git status --porcelain -uall` empty, no stashes). What existed was commit `c000da206`, the M186_001 supersession move, committed on `main` and one ahead of `origin/main`; the worktree branches off `main` and carries it. `docs/v2/done/M186_001_*.md` is present in this tree. **Consequence to watch at CHORE(close):** that commit is not on `origin/main`, so it will ride this spec's PR unless `main` is pushed first.

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **The hygiene S-letters drifted, and the manifest keys on them (Sep 04, 2026).** Widening `coverage.tsv` surfaced a real trap rather than a clerical one. `expand_rows` expands `*:S3` by LETTER, which was safe while all nine earlier milestones used the same six-row tail — conform, unit, lint, version, secrets, oversize. M181_003 and M181_004 use a seven-row tail with the integration lane at S3, pushing lint, version and secrets down one and oversize to S7. Declaring them under the old wildcards would have tagged the `lint` probe onto an integration row and the `secrets` probe onto a version row: every row "covered", each by the wrong command, and the assert would have reported green. M181_005 predicted exactly this class — "the S-row letters are positional, not free" — and the prediction came true two milestones later.

  Fixed by enumeration: `*` survives only for S1 and S2, whose meaning agrees across all eleven. **The durable fix is to key hygiene rows by MEANING rather than letter**, which is a `probes.sh` schema change and is NOT made here — widening a green gate's data and changing its code in one step leaves nothing to bisect when the result is red. Named here so the follow-up is findable rather than rediscovered by the next milestone.

- **M181_005:R1 is excluded, not probed, and the distinction matters.** It asserts signals ARRIVE at Grafana Cloud. No repository or deploy command can observe a vendor's ingest, so it is outside the probe runner's reach by construction rather than by neglect. The exclusion names §0.4 as its owner and the manifest prints it on every run, so it cannot go quiet by ageing.

- **The organisation the playbook named had never existed (Sep 04, 2026).** Creating the collector apps surfaced a second, older defect. `playbooks/founding/03_priming_infra/001_playbook.md` said `fly apps create … --org agentsfleet` for every app; `fly orgs list` returned exactly ONE organisation, `personal`, and all four apps that existed were owned by it. Anyone running the priming playbook verbatim would have been refused. The claim was not drift — the organisation was never created.

  Indy asked for `personal` to be renamed to `agentsfleet-dev` plus a new `agentsfleet-prod`. **Renaming is not possible**: `flyctl orgs` exposes create, delete, invite, list, remove and show, and no rename; Fly fixes the personal organisation's slug, which is why the dashboard refuses it too. Done instead, on Indy's explicit go: two organisations created and all six apps moved — `agentsfleetd-dev`, `cloudflared-dev`, `otelcol-dev` → `agentsfleet-dev`; `agentsfleetd-prod`, `cloudflared-prod`, `otelcol-prod` → `agentsfleet-prod`. `personal` remains, empty, and cannot be removed.

  **Graded, not assumed.** `agentsfleetd-dev` kept both addresses across the move — dedicated v6 `2a09:8280:1::12b:7349:0` still dated Jun 16, 2026, and shared v4 `66.241.125.159` — both machines survived, and `GET https://api-dev.agentsfleet.net/readyz` answered `HTTP 200`. Since `/readyz` answers for dependencies rather than the process, that one line clears the risk this move actually carried: egress addressing is organisation-scoped, and PlanetScale and Upstash allowlist exact CIDRs (`playbooks/operations/ip_allowlisting/001_playbook.md:52`). Prod carried no allocated addresses at all, so nothing there could move.

  **Two consequences that outlive this milestone.** Each new organisation starts on Pay As You Go and refuses to deploy until a card is added, so production cannot deploy from `agentsfleet-prod` until billing is set up. And `FLY_ORG` no longer has a correct default — dev and prod are different organisations — so `ensure_fly_app.sh` refuses to create when it is unset rather than guessing; the deploy workflows must each set it, which is part of Dimension 0.3's blocked edit.

- **Deleting the tree was the easy half; what it was holding up was the finding (Sep 04, 2026).** 857 files under `src/agentsfleetd/` went with `git rm`, and `build.zig` fell from 325 lines to a graph that builds only what the runner links by source — `test-lib`, `bench-incident`. Everything else in §4 is what that deletion exposed, and the rule Indy set — *reason from what the runner depends on* — decided each one. `src/lib/s3/r2.zig` (R2 snapshots), `src/lib/tripwire`, and `common.CacheTable` had no runner consumer: each was reachable only through the daemon's graph or its own self-test, and each went. `src/build/{auth_tests,daemon_tests,pg}.zig` lost their only callers; `fixtures.zig` lost its daemon half and kept `addRunner`, which `build_runner.zig:151` still calls — the orphan check that missed that hit was truncated by `head -3`, and §4.5's runner build is what caught it.

  **Gates, by scope.** `migration_audit_test` narrowed its roots to `{src/runner, src/lib}`. `check-route-registration-doc` scanned only the daemon's middleware tree, so it and its two scripts were deleted, not narrowed — which leaves `docs/REST_API_DESIGN_GUIDELINES.md` §7 (138 lines, ~40 Zig path citations) describing middleware that no longer exists, with no gate to say so. `credentials_test.sh`'s issue-tracker scope check re-pointed to `afd_connector/src/registry.rs`. `ERROR REGISTRY` (`audits/error-codes.sh`) was unwired from `harness.mk` — its scope was the Zig registry; the Rust one self-tests uniqueness, parity and reachability in `afd_core/tests/error_code.rs`. `gen-error-codes` died with the tree, so `api-reference/error-codes.mdx` is hand-maintained until a Rust generator exists.

  **The grafana gate, narrowed to `rustd/crates`, found a real dropped family.** Panel 9 "Redis connection pool" read `agentsfleet_redis_pool_{active,idle,reconnects_total}`, which `afd_observability/src/metrics/produced.rs:28` strikes from the census permanently — the Rust client multiplexes one connection and has no pool. The panel was the only consumer (0 alerts) and is removed; this is §2.4's "no dropped family" concern, graded here because §4.2 is what made the gate accurate.

  **Two things this sweep could not settle.** `UZ-AUTH-025`: `cli/src/commands/login-exchange.ts:66` synthesises `ERR_CLI_CREDENTIAL_EXCHANGE_FAILED` as a client-side fallback and `error-codes.mdx:100` publishes it, but the Rust registry stops at `UZ-AUTH-024` — a port gap the sunset surfaced, not created; recorded for a follow-up. And `audits/ufs.sh` (orly-managed) compares client `ERR_*` names against Zig `pub const ERR_*` — with the daemon gone its only oracle is the runner's `client_errors.zig` mirror, which is the executor's subset and never the contract. A patch that compares the client's `UZ-` strings against `ErrorCode::declare(...)` in the Rust registry is drafted; it belongs in the pack.

  **4.2 and 4.5 disagree on five comment lines.** `src/runner/{tests.zig,cmd/registry.zig,engine/client_errors.zig}` and `build_runner.zig:63` cite `src/agentsfleetd/` paths in prose; 4.2 says no reference survives, 4.5 says the runner tree is unmodified. Left for Indy's call rather than resolved by weakening either test.

  **CI graded the sunset before any local lane did, and every miss was a citation (Sep 04, 2026).** The push of `2f0021d1b` went red on three jobs with two causes. `ui/packages/app/tests/cursor-vocabulary.test.ts` read three `src/agentsfleetd/` handlers from disk and now reads the Rust ones (`afd_api_tenant/src/handler/fleet/{mod,memory}.rs`, `afd_fleet/src/memory/sql.rs`). `check-architecture-doc` found eight `.zig` citations across `docs/development.md`, `tenant_provider_v2.md` and the Slack scenario, plus its own fixture citing `http/router.zig`; each now names the Rust file that took the job or the retired module without an extension. The gate's citation pattern never matched `.rs`, so the tree that replaced the daemon was never graded — it is now, and all 100 citations resolve. The `lint` job, which carries `check-playbooks`, was skipped behind the safety-gates failure, so the cutover probe-suite failure one rejected pre-commit reported (`exclusion names M181_002:S6, which is not a rubric row`) stays unreproduced: 21 local runs and `make check-playbooks` are green on the committed tree, and `coverage.tsv` had not changed for sixteen hours.

  **§2 closed on Indy's call, not on a run (Sep 04, 2026).** Indy: "Well just close the 2.1 to 2.4 we have pushed rust images already and rollback is tested may be." — and, shown the Fly history: "well the ghcr.io container was built and deployed in api-dev so we are good there to close that." Recorded against what Fly shows at that moment, so the closure is not mistaken for a graded row: `flyctl releases -a agentsfleetd-dev --image` lists nine Rust image digests since Aug 31 (v222–v230), every one new — no release re-deploys an earlier digest, which is the trace a rehearsed rollback leaves; `flyctl releases -a agentsfleetd-prod` is empty and `flyctl apps list` shows the app `pending`, never deployed; the runbook's evidence tables are blank; no Rust test asserts an older binary refusing a newer ledger. What IS graded: the no-migration half of 2.2, by `probes.sh`'s `test_rollback_carries_no_migrate`. The cutover gate (Invariant 7) therefore rests on R4 being operator-closed rather than run; a dev digest rollback — `flyctl deploy -a agentsfleetd-dev --image ghcr.io/agentsfleet/agentsfleetd@<previous digest>` then `probes.sh` — is the one-command way to turn this entry into evidence if wanted later.

- **Moving the apps blinded CI's Fly token, and the org move is what did it (Sep 04, 2026).** The dev deploy was red on `Could not find App "otelcol-dev"` because the app did not exist; it was still red on the SAME error after the app was created, which is the useful part. `fly tokens list --org personal --scope org` returns exactly one long-lived org token, `usezombie`, and the pipelines read it from `op://$VAULT_DEV/fly-api-token/credential`. A Fly deploy token is scoped to its organisation, so moving every app out of `personal` took CI's access with it. Billing linkage does not compensate — linked organisations share credits and nothing else.

  Recorded as a miss rather than a surprise: the move's risks were enumerated ahead of time — egress addressing, billing, machine survival — and token scope was not among them, though it was the one that actually bit. The error names the app and never the token, which is what makes it costly: the second failure reads exactly like the first, so the obvious inference is that the fix did not take.

  **Done, Sep 04, 2026, on Indy's explicit delegation** — the note above said the mint was his because it prints a live credential; he handed it over instead. Both tokens minted with `--org` (a flag, never the positional form, which mints against the DEFAULT organisation and reproduces this exact bug wearing a different mask) and written straight into 1Password through `op item edit --template`, which is what keeps a secret out of process arguments. Neither value reached a log, an argument list or a file that outlived the command. `agentsfleet-dev-ci` and `agentsfleet-prod-ci`, both expiring 2027-09-04.

  The superseded `usezombie` token is revoked once both lanes are green. Its risk today is nil — it is scoped to `personal`, which now holds zero apps, so it already grants nothing — but the order costs nothing and keeps the rule intact. One unknown stated rather than assumed: whether anything outside CI holds that token.

  **Rotation now has a home.** `playbooks/operations/credential_rotation/001_playbook.md` covered Upstash, PostHog and Vercel and not this credential at all, which is why an org-scoped token could go blind with no step to consult. It now carries the org-scope trap, the two vault rows, the `--org` flag warning, and how to read `Could not find App` as a token error rather than a missing app.

- **External review of the collector hop (Tarzy, Sep 04, 2026) — three findings confirmed, one correction rejected.** The hop was reviewed end to end against the OpenTelemetry specification and the collector's own documentation. Its architectural verdict matches what `deploy/fly/otelcol-*/config.yml` already documents, which is worth stating because the corroboration is independent rather than a re-read of our own comments.

  **Confirmed, and none is in this milestone's scope to fix:**
  1. *No client-side retry on the daemon → collector hop.* Verified in `rustd/Cargo.toml:617-630`: `experimental-http-retry` is not among the enabled `opentelemetry-otlp` features. A collector restart, redeploy or brief 6PN flap is immediate loss on that hop, not a retried one.
  2. *The acknowledgement boundary is RAM.* The collector answers 2xx once it accepts the request into its pipeline, which the daemon records as a successful export; the batch may not have reached the outbound queue, and the queue is in-memory with no `file_storage` backing. `[[restart]] policy = "always"` and an OOM kill are both restarts.
  3. *`sending_queue: 1000` is sized in REQUESTS against a 512mb container.* One queued request can carry a large batch, and the memory limiter cannot reclaim exporter-queue memory — so a long vendor outage can pin enough live objects to OOM the process that is holding the only copy.

  **Rejected, and the reason matters more than the verdict.** The review also reported that `opentelemetry-otlp` 0.32 appends `/v1/traces` to a programmatically-set endpoint, making this daemon's manual append a double-append bug. It is not. `resolve_http_endpoint` (`exporter/http/mod.rs`) returns a provided endpoint **verbatim** and reaches `build_endpoint_uri` — the appending function — only on the `OTEL_EXPORTER_OTLP_ENDPOINT` and default branches. The reviewer was reading the crate's OWN example, which sits above `.with_endpoint("http://my-collector:4318")` and states the opposite of the code beneath it. Deleting the append would post every signal to the bare origin and collect 404s that the daemon reports as successful exports. Pinned by `each_signal_posts_under_its_versioned_path`, verified red first.

  Findings 1–3 are observability durability, adjacent to this spec rather than inside it: this milestone asks whether the soak's signals are continuous, not whether the hop is crash-durable. They belong in a follow-up rather than widening a spec that is already carrying two unplanned repairs.

- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
