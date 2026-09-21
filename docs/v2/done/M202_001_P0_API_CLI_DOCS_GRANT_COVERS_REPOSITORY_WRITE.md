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

# M202_001: The integration grant authorises repository write, and no card stands between a Pull Request and its review

**Prototype:** v2.0.0
**Milestone:** M202
**Workstream:** 001
**Date:** Sep 19, 2026
**Status:** DONE
**Priority:** P0 — the `github-pr-reviewer` golden path cannot complete a single review today; every model turn raises its own approval card.
**Categories:** API, CLI, DOCS
**Batch:** B1 — standalone; no other workstream touches the gate crates.
**Branch:** `fix/m202-close-and-acceptance-lanes`
**Baseline revision:** `5eb6f388535b11cbb6df6e15cf61f13ece2aedd9` (`origin/main` at CHORE(open))
**Test Baseline:** unit 5,405 → 5,323 (−82, TypeScript packages); rustd integration 512 → 550 (+38). 0 failed at both revisions.
**Baseline evidence:** `make test-unit-all` and `make test-integration-rustd` at `5eb6f388` (worktree) and at HEAD, read from each lane's own verdict line. HEAD unit: app 2,874 · website 142 · cli 1,676 · design-system 631, plus 2,646 rustd workspace tests; `enforce-coverage` line=100.00%. The unit fall is the parser cutover — 183 suites went, 55 replaced them. An earlier integration figure of 1,024 → 1,100 counted each test twice.
**Depends on:** none
**Provenance:** agent-generated (pre-spec, live reproduction against api-dev on Sep 19, 2026; owner decisions captured in Discovery)
**Canonical architecture:** `docs/architecture/scenarios/github-pr-reviewer.md` §3

---

## Overview

**Goal (testable):** A fleet holding an approved `github` integration grant runs a `pull_request` event from wake to posted review — token minted, diff read, comments posted — raising zero approval cards, and the next event after `grant delete` is refused without minting.

**Problem:** An operator installs the Pull Request reviewer, opens a Pull Request, and gets an unbounded queue of approval cards instead of a review. Each model turn raises its own card, and every card after the first is blank — it names no action, because a continuation carries no message of its own. Reproduced live on Sep 19, 2026: one steer against `agentsfleet/linkwarden#1` produced three `repository_write` cards, two approvals, 27,393,300 nanos of spend, and zero review comments on the Pull Request.

**Solution summary:** The integration grant already names the fleet, and the fleet's own `TRIGGER.md` already names the repositories and the access level — so the grant is the authorisation, and a second per-event question adds no information a person can act on. The grant lands approved at install, the unconditional `repository_write` park is deleted, and the credential mint reads the grant instead of a gate row. `budget.daily_dollars` becomes the money brake it was always meant to be, and `agentsfleet grant delete` is the manual stop.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(gate): the integration grant authorises repository write
- **Intent (one sentence):** An operator who has connected GitHub and installed a reviewer fleet gets reviews on their Pull Requests without answering a question per model turn, and keeps one command that stops it.

## Implementing agent — read these first

1. `rustd/crates/afd_gate/src/gate/first.rs` — the unconditional write park and its reasoning; §"The write-kind park runs before the rules" states the boundary this spec relocates rather than removes.
2. `rustd/crates/afd_approval/src/request.rs` — how a grant is requested today and why the raise and the resolve must spell one word (RULE UFS).
3. `rustd/crates/afd_credential/src/credential/github.rs` — `ScopedRequest::for_binding` and `Granted::verify`; the token is already scoped by the declared repositories, and that check stays.
4. `docs/architecture/scenarios/github-pr-reviewer.md` — the end-to-end walk this spec makes true.
5. `docs/RUST_ERROR_STANDARD.md` — every fallible signature touched here obeys it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_approval/src/request/install.rs` NEW · `grant_sql.rs` NEW | — | The install verb (approved grant, no card) and the grant half of `sql.rs`, split out because `request.rs` and `sql.rs` both sat at 326 of the 350 cap; mirrors `afd_gate`'s own pair. |
| `afd_approval/src/{sql,grant,lib}.rs` · `afd_fleet_lifecycle/src/install/grants.rs` | EDIT | Statements move to `grant_sql`; the install calls `grant_at_install`, not `request`. |
| `rustd/crates/afd_approval/src/request.rs` | EDIT | Declares the install module; keeps raising the card for `Origin::Park`, the backstop. |
| `rustd/crates/afd_gate/src/gate/first.rs` | EDIT | Delete `park_write_kind` and `writes_to_a_repository`; the rules walk becomes the only first-encounter path. |
| `rustd/crates/afd_gate/src/gate/detail.rs` | EDIT | Retire `KIND_REPOSITORY_WRITE`, `RADIUS_REPOSITORY_WRITE`, `REPOSITORY_WRITE_SPEND_CEILING` and `Stated::write_kind`. |
| `rustd/crates/afd_gate/src/gate/spend.rs` + `spend/tests.rs` | DELETE | The per-gate spend ledger has no gate to draw against once the park is gone. |
| `afd_gate/src/gate/{park,sql}.rs` · `policy/repair.rs` | EDIT | Spend bindings go and both columns write NULL; the branch names the event; `gate_of` deleted — no production caller. |
| `afd_fleet/src/lease/{mint,deliver}.rs` · `error/*.rs` | EDIT | The second mint gate goes and `repair_branch` stops being async. Three refusals unreachable and gone: `UZ-REPAIR-010`, `UZ-REPAIR-011`, `REPAIR_SPEND_EXHAUSTED` — **public surface, needs a `~/Projects/docs` branch before release.** |
| `afd_fleet_runtime/src/config/raw/predicate.rs` | EDIT | `DAEMON_OWNED_GATE_KINDS` keeps `repository_write` RESERVED, not owned. |
| `afd_gate/tests/integration_gate_lifecycle.rs` + `support/gate_fixture.rs` · `afd_fleet/tests/integration_credential_mint{.rs,/cases.rs}` | EDIT | The lifecycle suite asserted the defect; replaced by §2's Dimensions. Three write-gate mint cases and their seeder leave; four mint cases stay. |
| `cli/test/grant.integration.test.ts` EDIT · `cli/test/fleetbundle-pr-reviewer.unit.test.ts` NEW | — | Dimensions 4.1 and 4.3. |
| `rustd/crates/afd_gate/src/gate/grants.rs` | EDIT | The write authority read becomes the grant read: approved and unrevoked, or refuse. |

| `cli/src/commands/grant.ts` | NO CHANGE | Already status-agnostic: it renders `status` and `approved_at` generically and carries no copy implying a pending question. |
| `docs/architecture/scenarios/github-pr-reviewer.md` · `connectors.md` | EDIT | §3 records one authorisation; trust anchor 6 names the grant. That section's count was already drifting — 4 claimed, 5 listed — and is now 6. |
| `tests/fixtures/fleetbundle/github-pr-reviewer/SKILL.md` | EDIT | Reads owner and number from the event; the fixture matches the shipped bundle. |

| `cli/src/**` · `cli/package.json` · `bun.lock` | EDIT | The command tree, parser and entry point move from commander to `effect/unstable/cli`; commander leaves every import. The auth guard moves into `guardedHandler` at all 54 handler sites, because `Command.provideEffectDiscard` gates one command's handler and the root is a group with none. |
| `afd_fleet_runtime/src/config/raw/predicate.rs` · `afd_approval/src/{sql.rs,request/tests.rs}` · `afd_fleet_runtime/tests/frontmatter_fields.rs` | EDIT | R4 taken literally: the retired kind leaves `DAEMON_OWNED_GATE_KINDS`, which held one entry afterwards and is now the scalar `DAEMON_OWNED_GATE_KIND`. A new test pins that the retired spelling parses like any kind a fleet authors. |
| `cli/test/**` — 14 oversize suites split into 31, plus 12 `helpers-*.ts` modules | EDIT · NEW · DELETE | S7 taken literally: nothing the diff touches exceeds 350 lines. A pure move — every suite's test count is unchanged (1,676 pass across 188 files, was 178). `fleet-steer.integration.test.ts` also served as a helper module for three siblings, which now import `helpers-fleet-steer.ts`. |
| `cli/test/acceptance/{grant-approval-live,library-onboard-live}.spec.ts` · `app/tests/e2e/acceptance/multi-fleet-grant-journey.spec.ts` | EDIT | The `acceptance-cli` and `acceptance-e2e` lanes, red on `origin/main`: five of six failures asserted the pending card this spec deleted. The sixth, a `JSON Parse error: Unexpected EOF`, reported neither exit code nor stderr and now reads through the lane's own `trailingJson`. |
| `.githooks/**` · `.oracle/orly.json` · `AGENTS.md` · `README.md` · `docker-compose.yml` · `playbooks/lib/runner/**` · `app/{lib/api,tests}/**` | EDIT | Cutover fallout: hard-coded CLI paths in `cursor-vocabulary.test.ts`, the Bun shebang and dropped node `engines`, and the harness rows referencing them. |

**Scope note (R6).** The grant work shipped as Pull Request #699; what remained
was this close, the two red acceptance lanes, and the parser migration the close
could not be verified without. The owner directed one Pull Request — Discovery.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC** (no dead code at write time: the retired constants and `spend.rs` leave with their readers, in the same commit), **NLR** (touch-it-fix-it on every file opened), **NLG** (no "legacy gate" framing for the retired path — it is deleted, not deprecated), **UFS** (the grant status spelling is read from `afd_wire::grant::status`, never re-spelled), **ORP** (orphan sweep at CHORE(close)).
- **`docs/RUST_ERROR_STANDARD.md`** — every fallible signature under `rustd/` follows it; a crate never hand-writes its error type.
- **`dispatch/write_rust.md`** — ownership, justified `unsafe`, preserved error variants, deterministic concurrency tests.
- **`dispatch/write_ts_adhere_bun.md`** — the `cli/` edit is a TypeScript surface.
- **`docs/LOGGING_STANDARD.md`** — the retired `write_mint_refused` diagnostic and its replacement.
- **No compatibility aliases.** The retired gate kind gets no alias, no fallback spelling and no deprecation window.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS GATE | yes — grant status strings and the retired kind constant | Read every status from `afd_wire::grant::status`; delete the retired constants rather than relocating them. |
| LENGTH GATE | yes — `first.rs` and `grants.rs` are edited | Both shrink; `spend.rs` is deleted. No file approaches the cap. |
| LOGGING GATE | yes — a diagnostic is retired and one is added | The new refusal names the grant state, per `docs/LOGGING_STANDARD.md`. |
| MILESTONE-ID GATE | yes — new tests | Every added test carries `M202_001`. |
| GREPTILE GATE | yes — end-of-turn read | NDC/NLR/NLG/UFS named above and obeyed by construction. |
| SCHEMA GUARD | no — no `schema/` file is touched | `core.integration_grants` is used as it stands; no `DROP`, no `ALTER`, no migration-array edit. |
| UI GATE / DESIGN TOKEN GATE | no — no `ui/packages/` file is touched | N/A. |
| ZIG GATE | no — no `*.zig` file is touched | N/A. |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_approval/src/grant.rs` + `sql::RESOLVE_GATE` — the standing-authorisation shape this spec extends. The grant table is already the memory of a human answer that outlives the card that asked for it; this spec makes it the only such memory for repository write.
- **Reference:** `rustd/crates/afd_credential/src/credential/github.rs` — `ScopedRequest::for_binding` already narrows a token to the declared repositories and `Granted::verify` already checks the response. Neither changes; the spec removes a second authorisation layer above them, not the scoping beneath.

## Sections (implementation slices)

### §1 — The grant lands approved

A fleet declaring a mintable credential gets an approved grant at install and no card. **Implementation default:** `approved` is written by the same statement that writes the row, because a row that lands pending and is updated a moment later has a window where a delivery is refused for a grant nobody was going to be asked about.

- **Dimension 1.1** DONE — an install declaring `credentials: [github]` writes one grant row with status approved and a non-null `approved_at` → Test `test_m202_001_install_lands_an_approved_grant`
- **Dimension 1.2** DONE — no `integration_grant` card reaches the inbox for that install → Test `test_m202_001_install_raises_no_card`
- **Dimension 1.3** DONE — a re-install of the same fleet and service moves no second row, per `uq_integration_grants_fleet_id_service` → Test `test_m202_001_reinstall_keeps_one_grant`

### §2 — The unconditional write park is deleted

`writes_to_a_repository` and `park_write_kind` leave, with the kind, the radius and the ceiling they carried. **Implementation default:** delete rather than gate behind a flag, because a retired authorisation path that still compiles is a second answer to the question the grant now owns (RULE NDC, RULE NLG).

- **Dimension 2.1** DONE — a write-access fleet's first-encounter event with an approved grant reaches `Verdict::Pass` and raises no card → Test `test_m202_001_write_fleet_and_its_continuations_pass_without_a_card`
- **Dimension 2.2** DONE — a continuation event of that run also passes, raising no card → Test `test_m202_001_write_fleet_and_its_continuations_pass_without_a_card`
- **Dimension 2.3** DONE — a fleet declaring `gates.rules` still parks on a matching rule; the rules walk is untouched → Test `test_m202_001_rule_gated_fleet_still_parks`
- **Dimension 2.4** DONE — an event parked before this change still resolves from its recorded gate, so no in-flight run is stranded → Test `test_m202_001_parked_event_still_resolves`

### §3 — The mint reads the grant

The write authority becomes one question: is there an approved, unrevoked grant for this fleet and service? **Implementation default:** refuse closed on any read failure, matching the posture the retired path held for an unreadable gate lookup.

- **Dimension 3.1** DONE — a mint with an approved grant succeeds and the token names only the declared repositories → Test `test_m202_001_mint_scopes_to_declared_repositories`
- **Dimension 3.2** DONE — a mint with a revoked grant is refused and no token is issued → Test `test_m202_001_revoked_grant_refuses_the_mint`
- **Dimension 3.3** DONE — a mint with no grant row is refused → Test `test_m202_001_absent_grant_refuses_the_mint`
- **Dimension 3.4** DONE — a grant read that fails refuses rather than admitting → Test `test_m202_001_unreadable_grant_refuses_closed`
- **Dimension 3.5** DONE — a fleet at its `daily_dollars` is halted, proving the remaining money brake survived the ceiling's retirement → Test `test_m202_001_budget_still_halts_the_fleet`

### §4 — The surfaces tell the truth

The Command-Line Interface (CLI), the architecture pages and the changelog stop describing an authorisation that no longer exists. **Implementation default:** the fixture bundle is updated in the same commit as the daemon, because a fixture describing a bundle nobody ships proves nothing.

- **Dimension 4.1** DONE — `grant list --json` reports the install-time grant as approved with its `approved_at` → Test `test_m202_001_grant_list_reports_approved_at_install`
- **Dimension 4.2** DONE — the scenario page's §3 and §6 name one authorisation and the retired card appears nowhere → Test `test_m202_001_scenario_page_names_one_authorisation`
- **Dimension 4.3** DONE — the `github-pr-reviewer` fixture reads owner and number from the event payload → Test `test_m202_001_fixture_skill_reads_the_event`

## Interfaces

```
UNCHANGED — the wire carries no new field and loses none.

core.integration_grants                     (no schema change)
  UNIQUE (fleet_id, service)
  status ∈ afd_wire::grant::status          pending | approved | revoked
  approved_at BIGINT, revoked_at BIGINT

Install  POST /v1/workspaces/{ws}/fleets
  → grant row: status = approved, approved_at = now
  → NO core.fleet_approval_gates row

Mint (internal, runner ↔ broker)
  authority := approved grant for (fleet_id, service) AND revoked_at IS NULL
  scope     := declared repositories ∩ App installation   (unchanged)
  refusal   := UZ-GRANT-* as today; no new registry code

RETIRED — gate_kind "repository_write" is never written again.
  Existing rows stay readable; the inbox renders them as history.

Repair branch identity  (owner decision, Sep 19, 2026 — see Discovery)
  was := agentsfleet-repair/base64url(gate id)   the gate §2 deletes
  now := agentsfleet-repair/base64url(event id)  v7, afd_admission::admit
  The grant cannot name it: one row per fleet lifetime collides every event
  onto one branch. Encoding, width and the egress lock are unchanged.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Grant absent | Fleet installed before this change, or a credential declared by a later PATCH | Mint refused, `write_mint_refused` names `grant_absent`; the operator reinstalls or the request path raises the grant. |
| Grant revoked | Operator ran `grant delete` | Mint refused; the next event is refused before any provider call and before any spend. |
| Grant read fails | Database unavailable mid-run | Refuse closed, never admit; the event stays leasable and re-decides on the next poll. |
| Repository not declared | Fleet's binding does not name the repository the event carries | Unchanged — the mint's existing scoping refuses, and `Granted::verify` refuses a response that widened. |
| Budget exhausted | Fleet spent its `daily_dollars` | Unchanged — the money gate halts the fleet; this is the brake the retired ceiling duplicated. |
| Existing parked event | A `repository_write` card was pending when this ships | The recorded gate still decides that event, because `RefState::Found` outranks policy; no in-flight run is stranded. |

## Invariants

1. **No token is minted without an approved, unrevoked grant for that fleet and service** — enforced in the broker's mint path, proved by Dimensions 3.2, 3.3 and 3.4.
2. **A minted token names only repositories the fleet declares** — enforced by `ScopedRequest::for_binding` and checked against the response by `Granted::verify`; unchanged by this spec and pinned by Dimension 3.1 as a regression.
3. **A revoked grant refuses the next mint without a provider call** — enforced by ordering the grant read before any network use, proved by Dimension 3.2.
4. **`gate_kind = "repository_write"` is never written again** — enforced by deleting the constant; the Dead Code Sweep greps prove no writer survives.
5. **Money is bounded by `budget.daily_dollars`** — enforced by the existing money gate, pinned as a regression so retiring the mint ceiling cannot quietly remove the remaining brake.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `grant_auto_approved` | ops | An install writes an approved grant | `fleet_id`, `service` | no token, no repository contents, no operator identity beyond the workspace | `test_m202_001_install_lands_an_approved_grant` |
| `write_mint_refused` | ops | A mint is refused for grant state | `fleet_id`, `service`, refusal reason | no token material | `test_m202_001_revoked_grant_refuses_the_mint` |
| `grant_requested` | ops | Retired for the install path; still fires where a grant is requested outside install | unchanged | unchanged | `test_m202_001_install_raises_no_card` |

No product analytics event changes: the approval inbox loses a row type an operator never chose to create, and no funnel is defined over it. Discovery records the analytics decision.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_m202_001_install_lands_an_approved_grant` | Install a fleet declaring `credentials: [github]` → exactly one grant row, status approved, `approved_at` non-null. |
| 1.2 | integration | `test_m202_001_install_raises_no_card` | Same install → zero rows in `core.fleet_approval_gates` for that fleet. |
| 1.3 | integration | `test_m202_001_reinstall_keeps_one_grant` | Install the same bundle twice → one grant row; the unique constraint holds and no second row appears. |
| 2.1 | integration | `test_m202_001_write_fleet_and_its_continuations_pass_without_a_card` | Write binding, no gate rules → first encounter is `Verdict::Pass`. Tier corrected from `unit`: `judge_first_encounter` takes `&self` on a `Gates` holding a live pool, so no unit tier exists. |
| 2.2 | integration | `test_m202_001_write_fleet_and_its_continuations_pass_without_a_card` | Three following turns also `Pass`, then `card_count() == 0`, unfiltered by kind: the claim is that NOTHING is raised. One test with 2.1 — the treadmill is the pair, and splitting it would let half pass. |
| 2.3 | integration | `test_m202_001_rule_gated_fleet_still_parks` | A config whose `gates.rules` match the event → parks, holds, and passes once answered. Tier corrected, same reason. |
| 3.1 | unit | `test_m202_001_mint_scopes_to_declared_repositories` | Binding names one repository → the scoped request carries that bare name and `contents`+`pull_requests` write, and nothing else. |
| 3.2 | integration | `test_m202_001_revoked_grant_refuses_the_mint` | Approve then revoke, then mint → refused, no provider call recorded, no token returned. |
| 3.3 | integration | `test_m202_001_absent_grant_refuses_the_mint` | No grant row → mint refused. |
| 3.4 | unit | `test_m202_001_unreadable_grant_refuses_closed` | Injected read failure → refusal, never admission. |
| 4.1 | e2e | `test_m202_001_grant_list_reports_approved_at_install` | Subprocess `agentsfleet grant list --fleet <id> --json` → one item, `status: "approved"`, non-null `approved_at`. |
| 4.2 | unit | `test_m202_001_scenario_page_names_one_authorisation` | `scenarios/github-pr-reviewer.md` names the grant as the authorisation and carries no `repository_write`. Written because the SPEC TEMPLATE GATE requires a tiered test per Dimension; the agent had argued for review-only coverage and the gate overruled it. |
| 4.3 | unit | `test_m202_001_fixture_skill_reads_the_event` | The fixture `SKILL.md` names `repository.full_name` and `pull_request.number`, carries no `/pulls/<digits>`, and still refuses to approve or merge. Asserted on the file: this fixture is prose a MODEL executes. |
| 2.4 | integration | `test_m202_001_parked_event_still_resolves` | A pre-existing pending `repository_write` row → the event still resolves from the recorded gate; regression for in-flight runs. |
| 3.5 | integration | `test_m202_001_budget_still_halts_the_fleet` | Fleet at its `daily_dollars` → halted; regression proving the remaining money brake survived the ceiling's retirement. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | An install raises no approval card (§1) | `cargo test -p afd_approval m202_001 -- --include-ignored` | exit 0 | P0 |  ✅ shipped in #699 |
| R2 | A write fleet runs every event and continuation cardless (§2) | `cargo test -p afd_gate m202_001` | exit 0 | P0 |  ✅ shipped in #699 |
| R3 | A revoked grant refuses the mint (§3) | `cargo test -p afd_credential m202_001 -- --include-ignored` | exit 0 | P0 |  ✅ shipped in #699 |
| R4 | The retired gate kind has no writer left (§2) | `git grep -rn -w 'repository_write' rustd/ cli/ \| grep -v '_test\|/tests/'` | 0 matches | P0 |  ✅ 0 matches |
| R5 | A live Pull Request is reviewed end to end (§4) | manual — see Decision ownership below | a review comment on `agentsfleet/linkwarden#1`, with its Uniform Resource Locator (URL) in Discovery | P0 |  ⬜ manual — unrun |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 |  ✅ table covers the diff |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 |  ✅ ALL GATES GREEN |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ all unit lanes passed |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | ✅ all lint checks passed |
| S4 | Integration suite green | `make test-integration-rustd` | exit 0 | P0 | ✅ 550 passed, 0 failed |
| S5 | Version sync | `make check-version` | exit 0 | P0 | ✅ 0.49.0 everywhere |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ no leaks found |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 |  ✅ no output |
| S8 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | ✅ 0 outside test prose |

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes, so recording those results does not require another code commit and suite run. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may also be **MOVED** — see below.

**Decision ownership.** R5 is tier `manual` and cannot be manufactured. Procedure: install the fleet into a workspace whose GitHub App covers `agentsfleet/linkwarden`, open or reopen a Pull Request there, and observe a posted review. Required person: the workspace owner, because only they can bind the App. Durable evidence: the review comment Uniform Resource Locator (URL), recorded in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `rustd/crates/afd_gate/src/gate/spend.rs` and `spend/tests.rs` | `test ! -f rustd/crates/afd_gate/src/gate/spend.rs && test ! -d rustd/crates/afd_gate/src/gate/spend` |

**2. Orphaned references — zero remaining imports/uses.**

`git grep -rn -w "<symbol>" rustd/` → 0 matches outside test prose. The list grew during implementation: retiring the ceiling pulled its supply chain, and `cargo` refused to compile until every reader went too.

| Deleted symbol | Was |
|----------------|-----|
| `REPOSITORY_WRITE_SPEND_CEILING`, `RADIUS_REPOSITORY_WRITE`, `KIND_REPOSITORY_WRITE`, `Stated::{write_kind,spend_ceiling}` | the card's kind, radius, ceiling and stamp |
| `park_write_kind`, `writes_to_a_repository` | the unconditional park and its predicate |
| `WriteApproval`, `reserve_write_approval`, `approved_write_gate` | the second authorisation layer |
| `SELECT_APPROVED_WRITE_GATE`, `LOCK_WRITE_GATE_FOR_MINT`, `SPEND_WRITE_GATE_FOR_MINT`, `Bound::spend_count`, `SPEND_OPENS_AT` | statements and bindings with no readers left; both spend columns now NULL |
| `repair::gate_of` | the branch→gate reverse; no production caller even before this |
| `write_unapproved`, `binding_drift`, `write_spend_exhausted` + `ErrorKind`s and `DETAIL_*` | three refusals now unreachable |

## Out of Scope

- **A continuation inheriting its parent's gate decision.** The same treadmill still reaches any fleet whose `gates.rules` require approval, because a continuation gets a fresh event identifier and `first.rs` never reads `resumes_event_id`. Deleting the write park removes the blocker for this scenario and not the underlying defect — follow-up spec.
- **A blank `proposed_action` on a continuation card.** `CONTINUATION_BODY` is `{}`, so any card raised for a resumed run names nothing. Rides the follow-up above.
- **`agentsfleet logs` showing tool calls.** The scenario page promises the run's `http_request` calls and responses; `logs` returns the same event rows as `events` — separate defect, separate spec.
- **`fleet update` renaming a fleet from bundle frontmatter** without warning — separate defect.
- **`UZ-BUNDLE-001` collapsing every malformed-bundle reason into one sentence** — public error text, needs a matching `~/Projects/docs` branch.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Priya opens a Pull Request, walks away, and comes back to find review comments on it. She was asked nothing.
2. **Preserved user behaviour** — `grant delete` still stops a fleet instantly. Gate rules a workspace authored still park events. An in-flight parked event still resolves from its recorded gate. Budget still halts a fleet that overspends.
3. **Optimal-way check** — this is the most direct shape: the authorisation a person already gave is the one consulted. The gap to unconstrained-optimal is that the declared repository list is trusted as written rather than snapshotted at approval; acceptable now because the App installation bounds which repositories a token can ever reach, and revoke is immediate.
4. **Rebuild-vs-iterate** — iterate. The grant table, its statuses and its revoke path already exist and are already the memory of a human answer; nothing about them needs rebuilding to carry this.
5. **What we build** — an approved-at-install grant, a deleted park, a mint that reads the grant, and the surfaces that describe it.
6. **What we do NOT build** — a repository-scoped grant table, a snapshot of the approved repository list, a per-approval mint ceiling, a deprecation window for the retired kind, or any compatibility alias.
7. **Fit with existing features** — compounds with connectors, the credit gate and the fleet library. The one feature it must not destabilize is the gate rules path, which is how a workspace asks to be consulted on purpose.
8. **Surface order** — Command-Line Interface first, the repository default. The dashboard inbox simply stops receiving a row type; no new control is added.
9. **Dashboard restraint** — no new dashboard control. The approval inbox keeps rendering historical `repository_write` rows as history and gains nothing.
10. **Confused-user next step** — a refused mint names the grant state, so `agentsfleet grant list --fleet <id>` is the self-serve move and its output says approved, revoked or absent.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections split by authority — where the grant is written (§1), where the old question is removed (§2), where the new question is asked (§3), and where the surfaces are told (§4). Each is independently provable and §2 cannot land before §3 without a window in which a write fleet is authorised by nothing.
- **Alternatives considered:** (a) a `repository_grants` table keyed by fleet and repository — rejected by the owner: the authorisation is the integration grant and a second table splits one answer across two rows; (b) asking once at bind time instead of auto-approving — proposed and rejected by the owner, recorded in Discovery; (c) keeping the 32-mint ceiling on a grant column — rejected, because it is a money brake and `budget.daily_dollars` is the money brake.
- **Patch-vs-refactor verdict:** this is a **patch** because the standing-authorisation machinery, its statuses, its revoke path and its token scoping all exist and are all correct. What changes is which of two authorisation layers is consulted, and the removal of the one that duplicated the other.

## Discovery (consult log)

- **Consults** — Architecture consult against `docs/architecture/scenarios/github-pr-reviewer.md` and `docs/architecture/connectors.md`: the scenario page's §6 lists the external repository test as the open item, and this spec is what closes it. Live reproduction against api-dev on Sep 19, 2026 established the defect: fleet `01a0b863-15e8-7502-adc4-daf05a56187b`, gates `01a0b867-7f1d` (named the steer message), `01a0b868-3a72` and `01a0b869-fee5` (both `proposed_action: ""`), events `…-113` `fleet_error`, `…-114` `processed`, `…-115` `received`; `agentsfleet/linkwarden#1` carried 0 reviews and 0 comments throughout.
- **Owner decisions (verbatim).**
  > Indy (2026-09-19): "But instead how about do the integration_grant on a repo level as well, once Priya has connected their github repo, and the PR #42 is triggered is part of the repo list, the fleet repsonsible for it must start? meaning an auto approved grant with option to deny, or the regular what revoke/approve pattern we have." — context: the shape of the fix.
  > Indy (2026-09-19): "I want an auto approve, and i dont want a repository_grants table, as this is an integration_grant ..." — context: overrides the agent's recommendation to ask once at bind time and to add a table.
  > Indy (2026-09-19): "budet is the brake, i dont this we must cap on 32. the cap is already there. and the if priya revokes is a gate too (manual)" — context: retires `REPOSITORY_WRITE_SPEND_CEILING`; `budget.daily_dollars` and `grant delete` are the brakes.
- **Owner decision during implementation (Sep 19, 2026)** — `repair::branch_for`
  names the branch after the APPROVED gate and `policy::egress::write` locks that
  string as the only ref a run may create, so §2 leaves the write lease refused at
  `Misconfigured::NoRepairBranch`. Offered four sources; Indy selected **the event**,
  rejecting the grant on evidence that its unique constraint collides every event.
- **Agent recommendation not taken, recorded for the record** — the agent recommended asking once at bind time rather than auto-approving, on the grounds that the declared repository list is editable under the same scope that wakes the fleet, so auto-approval trusts a list no person confirmed. The owner weighed it and chose auto-approve. The residual risk is bounded by the App installation, `budget.daily_dollars` and `grant delete`, and is stated in Product Clarity item 3.
- **Owner decisions at close (verbatim, Sep 20, 2026).**
  > "I think i told you do move the the spec M202 that you are claiming to simply move to done, since it done in your PR. I just need 1 pr" — the `Branch:` field was corrected to the branch holding the work rather than the work being moved.
  > "Make the code match the rubric literally" — R4, R6 and S7 each failed on criterion wording, not code. Offered a rubric refinement, a literal code change, or shipping red; the owner chose literal. R4 therefore deletes the reserved gate kind this spec had earlier kept, and S7 splits pre-existing oversize suites the LENGTH GATE itself exempts.
  > "Also i need the acceptance-cli, acceptance-e2e to pass" — both red on `origin/main` at the post-merge deploy of #701 (run 35496393337); fixing them is in scope here.
- **Deferrals** — none at authoring. Every item in Out of Scope is a named separate defect, not a deferral of this spec's own scope.
