# M32_001: Quickstart + Docs Rewrite for v2 MVP

**Prototype:** v2.0.0
**Milestone:** M32
**Workstream:** 001
**Date:** Apr 21, 2026
**Status:** PENDING
**Priority:** P1 — The docs are the product landing surface; stale pre-Clerk vocabulary blocks operator onboarding
**Batch:** B3 — sequential on B2 code specs (M11_005, M19_001, M13_001, M21_001, M27_001, M33_001). Docs author against target-state, land after code merges to main.
**Branch:** feat/m32-quickstart-v2 (in `~/Projects/docs`)
**Depends on:** M11_005 (tenant credits), M19_001 (zombie lifecycle UI + `zombiectl zombie install`), M13_001 (credential vault UI), M21_001 (BYOK), M27_001 (dashboard detail + list pages), M33_001 (flagship `samples/homelab` executable Homelab Zombie).

**Sequencing note — M31_001 lands first in the docs repo.** M31_001 and M32_001 both touch `docs.json` and `concepts.mdx`. They are NOT parallel in the docs repo. Order: M31_001's docs-side PR merges first, then M32_001 rebases onto the updated docs `main`. In the usezombie repo, M31 is a code branch (see M31_001 spec); only its sibling docs PR is the one that collides with M32.

---

## Overview

**Goal (testable):** A new operator lands on `docs.usezombie.com/quickstart` and completes the full signup → create zombie → trigger → verify-credits-deducted loop in under 10 minutes, using only the docs and `zombiectl`.

**Problem:** The current `docs.usezombie.com` content was written against the pre-Clerk "lead-collector demo" framing. It references access codes, invite redemption, per-workspace credits, and marketing-site homepage copy that no longer match what ships. A new operator following the quickstart lands on dead CLI paths (`zombiectl credits redeem`, invite-code flows) and never reaches a running zombie.

**Solution summary:** Rewrite the Mintlify docs at `$DOCS_REPO/` to match the v2 MVP: Clerk signup → $10 tenant balance → create a zombie → trigger it via curl → see credits deducted in the dashboard. New CLI reference page, new operator quickstart, billing page reflects tenant-scoped credits, stale-vocabulary sweep across every `.mdx`.

---

## Files Changed (blast radius)

Target repo: `$DOCS_REPO` (Mintlify, deployed at docs.usezombie.com). Set `DOCS_REPO` before running any eval command:

```bash
export DOCS_REPO="${DOCS_REPO:-$HOME/Projects/docs}"  # default for local dev; CI must export its own checkout path
```

All grep / `mint dev` / `wc` commands in this spec reference `$DOCS_REPO` (not a hardcoded path) so they work from any machine, CI runner, or agent worktree.

| File | Action | Why |
|------|--------|-----|
| `quickstart.mdx` | REWRITE | New 6-step flow matching M19 UI: sign up → dashboard → create zombie with stub skill → copy webhook URL → curl trigger → verify credits in dashboard. |
| `index.mdx` | REWRITE | Hero from "lead-collector demo" to "runtime for self-hosted infra agents". Align with `docs/brainstormed/docs/homelab-zombie-launch.md`. |
| `concepts.mdx` | UPDATE | Add section: "Tenants, workspaces, zombies, skills — how they relate." Clarify that credits are tenant-scoped (post-M11_005). |
| `cli/zombie-lifecycle.mdx` | CREATE | `zombiectl` command reference: `zombie create / run / logs / trigger`, `credential add`. |
| `operator/quickstart.mdx` | CREATE | Deployment quickstart for self-hosted operators. |
| `billing/*.mdx` | UPDATE | Tenant-scoped $10 free credits. Remove invite-code/redeem references. |
| all `*.mdx` | SWEEP | Remove stale `credits redeem`, `invite code`, `access code` references (killed by Clerk pivot). |
| `docs.json` | UPDATE | Nav wiring for new pages: remove 3 deleted integration pages, add 4 new zombie pages (flagship Homelab Zombie + 3 README-only zombies). |
| `docs/integrations/hiring-agent.mdx` | DELETE | Stale zombie page (sourced from `nostromo/hiring_agent_zombie.md`, now deleted — low forward value). |
| `docs/integrations/lead-collector.mdx` | DELETE | Stale zombie page (sourced from `nostromo/lead_collector_zombie.md`, now deleted). |
| `docs/integrations/ops.mdx` | DELETE | Stale zombie page (sourced from `nostromo/ops_zombie.md`, now deleted). |
| `docs/zombies/homelab.mdx` | CREATE | Flagship zombie page sourced from `docs/brainstormed/docs/homelab-zombie-launch.md` — the Homelab Zombie narrative (kubectl/docker diagnosis without holding the kubeconfig). The underlying executable skill (`samples/homelab/`) is authored by M33_001; this MDX page is the user-facing docs entry. |
| `docs/zombies/homebox-audit.mdx` | CREATE | README-only zombie page sourced from `samples/homebox-audit/README.md` (README-only move in §9; not an executable install target in this milestone). |
| `docs/zombies/migration-zombie.mdx` | CREATE | README-only zombie page sourced from `samples/migration-zombie/README.md` (README-only move in §9; not an executable install target in this milestone). |
| `docs/zombies/side-project-resurrector.mdx` | CREATE | README-only zombie page sourced from `samples/side-project-resurrector/README.md` (README-only move in §9; not an executable install target in this milestone). |
| `changelog.mdx` | UPDATE | Note zombie page churn (3 removed, 4 added) under an `Integrations` tag. |

---

## Applicable Rules

Standard set only — no domain-specific rules (the target is a Mintlify docs repo, not Zig/SQL).

---

## Sections (implementation slices)

### §1 — Stale-vocabulary sweep

**Status:** PENDING

Before writing new prose, find and delete/rewrite every pre-Clerk term across the docs repo.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | full docs tree | `grep -rni "credits redeem\|invite code\|access code" .` | zero matches after sweep | lint (grep) |
| 1.2 | PENDING | full docs tree | `grep -rni "lead collector" .` | zero matches outside historical changelog entries | lint (grep) |
| 1.3 | PENDING | billing pages | occurrences of workspace-scoped credit language | reframed to tenant-scoped | manual review |

### §2 — Quickstart rewrite

**Status:** PENDING

Rewrite `quickstart.mdx` to match M19 UI + M11_005 credits + CLI trigger.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `quickstart.mdx` | new operator walks the page top-to-bottom | 6 steps: sign up → dashboard → create zombie → copy webhook URL → curl trigger → verify credits | manual E2E |
| 2.2 | PENDING | `quickstart.mdx` | every CLI block | every `zombiectl` command exists and does what the page says | manual smoke |
| 2.3 | PENDING | `quickstart.mdx` | every screenshot | matches current M19 UI (or uses stub placeholder pending dashboard render) | manual review |

### §3 — Index + concepts refresh

**Status:** PENDING

Reframe the hero and clarify the tenant/workspace/zombie/skill model.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `index.mdx` | current lead-collector hero | new hero aligned with `docs/brainstormed/docs/homelab-zombie-launch.md` | manual review |
| 3.2 | PENDING | `concepts.mdx` | new section "Tenants, workspaces, zombies, skills" | covers the four nouns + credit scoping | manual review |
| 3.3 | PENDING | `concepts.mdx` | credits section | calls out $10 tenant balance, shared across workspaces | manual review |

### §4 — CLI reference

**Status:** PENDING

One new page covering the MVP `zombiectl` surface.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `cli/zombie-lifecycle.mdx` | `zombie create / run / logs / trigger` | each documented with `--help` output + one example | manual smoke |
| 4.2 | PENDING | `cli/zombie-lifecycle.mdx` | `credential add` | documented with encryption note + delete pointer | manual review |

### §5 — Operator guide

**Status:** PENDING

Standalone self-hosted quickstart.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | PENDING | `operator/quickstart.mdx` | operator with fresh host | deploy `zombied` via docker-compose (or equivalent), point CLI at it, run same quickstart | manual E2E |
| 5.2 | PENDING | `operator/quickstart.mdx` | credential provisioning | covers `op://` vault pattern + env var fallback | manual review |

### §6 — Billing accuracy

**Status:** PENDING

Rewrite billing pages around the single-tenant-wallet model.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 6.1 | PENDING | `billing/*.mdx` | credit semantics | $10 / 1000¢ at signup; shared across workspaces; debited per run | manual review |
| 6.2 | PENDING | `billing/*.mdx` | removed vocabulary | no references to redeem/invite/access codes | lint (grep) |

### §7 — Nav wiring

**Status:** PENDING

`docs.json` updates.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 7.1 | PENDING | `docs.json` | new CLI + operator pages | reachable from sidebar | `mint dev` manual |
| 7.2 | PENDING | `docs.json` | deleted pre-Clerk pages | no broken nav entries | `mint dev` link check |
| 7.3 | PENDING | `docs.json` | zombie nav parity — 3 old integration entries removed, 3 new zombie entries added | integration nav count drops by 3, zombies nav count rises by 3 | `mint dev` link check |

### §8 — Zombie page churn

**Status:** PENDING

Replace the three stale pre-pivot integration zombies (hiring-agent, lead-collector, ops) with four new zombie pages: the flagship **Homelab Zombie** (sourced from `docs/brainstormed/docs/homelab-zombie-launch.md`; the executable skill at `samples/homelab/` is authored by M33_001) plus three README-only zombies (homebox-audit, migration-zombie, side-project-resurrector) sourced from the moved `samples/<name>/README.md`.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 8.1 | PENDING | `docs/integrations/{hiring-agent,lead-collector,ops}.mdx` | stale-page deletion | all three files removed from `docs/` repo; no broken internal links remain | `grep -r "integrations/hiring-agent\|integrations/lead-collector\|integrations/ops" $DOCS_REPO/` returns 0 |
| 8.2 | PENDING | `docs/zombies/homelab.mdx`, `docs/zombies/homebox-audit.mdx`, `docs/zombies/migration-zombie.mdx`, `docs/zombies/side-project-resurrector.mdx` | new zombie addition | each page renders cleanly under `mint dev`. `homelab.mdx` content reflects `docs/brainstormed/docs/homelab-zombie-launch.md`; the three README-only pages reflect `samples/<name>/README.md` post-move. | manual review + `mint dev` |
| 8.3 | PENDING | `docs.json` + changelog | nav parity | `docs.json` drops 3 integration entries, adds 3 zombie entries; `changelog.mdx` notes the churn under an `Integrations` tag | `mint dev` broken-link clean |

---

## Interfaces

**Status:** PENDING — this is a docs-only workstream; no code interfaces.

The "interfaces" it documents are:
- `zombiectl zombie create / run / logs / trigger / credential add` (owned by M19/M13/existing CLI)
- `GET /v1/tenants/me/billing` (owned by M11_005 — returns `{plan_tier, plan_sku, balance_cents, updated_at}`)
- Dashboard pages at `app.usezombie.com/{dashboard,zombies,zombies/[id]}` (owned by M27_001)

Each referenced interface must exist before this workstream merges; see Depends on.

---

## Failure Modes

| Failure | Trigger | System behavior | User observes |
|---------|---------|-----------------|---------------|
| CLI command in docs missing from binary | operator copies a command that doesn't exist | CLI errors `unknown command` | manual E2E catches before merge |
| Screenshot references old UI | M19 re-flows post-screenshot | page still loads; screenshot just stale | addressed in §2.3 |
| `mint dev` link check fails | missing page / bad nav entry | Mintlify build errors at dev-time | `mint dev` gate at §7 |
| Operator follows page without `zombied` running | local-first flow without infra | CLI reports connection refused | operator quickstart §5 covers the setup |

---

## Implementation Constraints

| Constraint | How to verify |
|-----------|---------------|
| `mint dev` builds cleanly with zero broken links | `cd $DOCS_REPO && mint dev` |
| Zero grep matches for `credits redeem`, `invite code`, `access code` outside changelog history | `grep -rni "credits redeem\|invite code\|access code" $DOCS_REPO/ \| grep -v changelog.mdx` returns 0 |
| Quickstart end-to-end works in < 10 minutes | manual timed walkthrough |
| Every referenced `zombiectl` command exists | `zombiectl <cmd> --help` exits 0 for each |
| Every referenced API path exists in the OpenAPI spec on main | cross-check |

---

## Test Specification

### Manual E2E walkthroughs (the only "test" a docs rewrite has)

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| Hosted quickstart top-to-bottom | 2.1, 2.2 | fresh operator, no prior state | signup → dashboard → create zombie → curl trigger → credits show deduction; completes ≤10 min |
| Operator quickstart top-to-bottom | 5.1 | fresh host, nothing installed | deploy `zombied`, run same flow against it |
| Nav walk | 7.1, 7.2 | click every sidebar entry | no 404, no broken internal links |

### Negative tests

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|----------------|
| Stale vocabulary sweep | 1.1, 1.2 | `grep -rni "credits redeem\|invite code\|access code" .` | exits non-zero (no matches) |

### Regression tests

N/A — docs repo has no automated regression coverage beyond `mint dev` + grep.

### Leak tests

N/A — docs rewrite.

### Spec-claim tracing

| Claim | Test | Type |
|-------|------|------|
| "Completes in under 10 minutes" | timed manual walkthrough | E2E |
| "No references to removed pre-pivot vocabulary" | grep sweep | lint |
| "Docs build cleanly" | `mint dev` | build |

---

## Execution Plan

| Step | Action | Verify |
|------|--------|--------|
| 1 | Audit stale vocab with grep sweep; list every match. | `grep -rni "credits redeem\|invite code\|access code\|lead collector" $DOCS_REPO/` |
| 2 | Rewrite `quickstart.mdx` against M19 UI + curl trigger + credit check. | §2 dims |
| 3 | Rewrite `index.mdx` hero + blurb. | §3.1 |
| 4 | Update `concepts.mdx` with tenant/workspace/zombie/skill + tenant credits. | §3.2–3.3 |
| 5 | Add `cli/zombie-lifecycle.mdx`. | §4 dims |
| 6 | Add `operator/quickstart.mdx`. | §5 dims |
| 7 | Update billing pages. | §6 dims |
| 8 | Update `docs.json` nav. | §7 dims |
| 9 | `mint dev` end-to-end; fix broken links. | `mint dev` clean |
| 10 | Manual E2E walkthrough timed. | ≤10 min |

---

### §9 — Sample zombies: move from brainstormed/ to repo root samples/ (physical `mv` + doc-source retarget only)

**Status:** PENDING

Samples we want to ship to operators belong at the repo root under `samples/`, not under `docs/brainstormed/` (which is a scratchpad). Two distinct kinds of move land here:

**(A) Flagship executable zombie — Homelab Zombie.**
M32 §9 only performs the physical `git mv` of the two sub-skill directories (`kubectl-readonly`, `docker-readonly`) from `docs/brainstormed/samples/skills/` to `samples/skills/` (and subsequently referenced by `samples/homelab/skills/` — authored by M33_001). The flagship `samples/homelab/` directory itself (its `SKILL.md`, `TRIGGER.md`, `README.md`) is **authored by M33_001**, not by M32. M32 does not attempt `zombiectl zombie install --from samples/homelab` — that E2E is M33_001's acceptance.

**(B) README-only zombies — homebox-audit, migration-zombie, side-project-resurrector.**
These three directories move from `docs/brainstormed/samples/` to `samples/` as README-only content (no executable `SKILL.md` / `TRIGGER.md` authored in this milestone). They exist at `samples/<name>/README.md` purely so the four new MDX zombie pages have a canonical source path post-move. Making them installable is a follow-up milestone per zombie — not M32, not M33.

**M32 owns ONLY the physical move + doc-source retarget.** It does NOT author executable skills; it does NOT attempt any `zombiectl zombie install --from samples/<name>` E2E. The valid install target authored in this v2 cycle is `samples/homelab` (owned by M33_001). `samples/homebox-audit/`, `samples/migration-zombie/`, `samples/side-project-resurrector/` are README-only after M32 and are explicitly NOT valid install targets.

**Physical move (in the `usezombie` repo, not the docs repo):**

| From | To | Nature |
|------|----|--------|
| `/Users/kishore/Projects/usezombie/docs/brainstormed/samples/homebox-audit/` | `/Users/kishore/Projects/usezombie/samples/homebox-audit/` | README-only |
| `/Users/kishore/Projects/usezombie/docs/brainstormed/samples/migration-zombie/` | `/Users/kishore/Projects/usezombie/samples/migration-zombie/` | README-only |
| `/Users/kishore/Projects/usezombie/docs/brainstormed/samples/side-project-resurrector/` | `/Users/kishore/Projects/usezombie/samples/side-project-resurrector/` | README-only |
| `/Users/kishore/Projects/usezombie/docs/brainstormed/samples/skills/docker-readonly/` | `/Users/kishore/Projects/usezombie/samples/skills/docker-readonly/` | sub-skill (consumed by M33_001's flagship) |
| `/Users/kishore/Projects/usezombie/docs/brainstormed/samples/skills/kubectl-readonly/` | `/Users/kishore/Projects/usezombie/samples/skills/kubectl-readonly/` | sub-skill (consumed by M33_001's flagship) |

The three zombie directories each contain `README.md` only. The two skill directories are preserved wholesale; M33_001 converts their content into Clawhub-format `SKILL.md` at `samples/homelab/skills/<name>/SKILL.md`. The physical `samples/homelab/` root (flagship SKILL.md + TRIGGER.md + README.md) is **not created by M32** — it is created by M33_001.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 9.1 | PENDING | `/Users/kishore/Projects/usezombie/samples/` | `ls /Users/kishore/Projects/usezombie/samples/` after move | Lists exactly: `homebox-audit/`, `migration-zombie/`, `side-project-resurrector/`, `skills/docker-readonly/`, `skills/kubectl-readonly/` | shell |
| 9.2 | PENDING | `/Users/kishore/Projects/usezombie/docs/brainstormed/samples/` | `ls` after move | Empty, or directory removed entirely | shell |
| 9.3 | PENDING | `docs.json` (Mintlify) | nav entries for the four new zombie pages | `homelab.mdx` is listed alongside the three README-only zombies; nav attribution references `samples/homelab` (for flagship — authored by M33_001) and `samples/<name>/README.md` (for the three README-only zombies) | manual review |
| 9.4 | PENDING | `docs/zombies/homelab.mdx`, `docs/zombies/homebox-audit.mdx`, `docs/zombies/migration-zombie.mdx`, `docs/zombies/side-project-resurrector.mdx` | `Files Changed` row and inline source attribution | `homelab.mdx` cites `docs/brainstormed/docs/homelab-zombie-launch.md` (narrative) + `samples/homelab/` (executable, authored by M33_001); the three README-only pages cite `samples/<name>/README.md` | grep |
| 9.5 | PENDING | physical existence check | `ls /Users/kishore/Projects/usezombie/samples/homebox-audit/ /Users/kishore/Projects/usezombie/samples/migration-zombie/ /Users/kishore/Projects/usezombie/samples/side-project-resurrector/` | Each shows only `README.md` (no SKILL.md / TRIGGER.md in this milestone — those are per-zombie follow-ups) | shell (ls) |

**Acceptance for this section:**

- [ ] Files are moved on disk (no copies; `git mv` so history follows).
- [ ] `docs.json` nav items for all four zombie pages (flagship + 3 README-only) land with correct source attribution.
- [ ] The four `docs/zombies/*.mdx` pages cite the correct upstream path.
- [ ] `samples/homebox-audit/`, `samples/migration-zombie/`, `samples/side-project-resurrector/` each contain README.md only — they are README-only in this milestone and explicitly NOT install targets for `zombiectl zombie install`.
- [ ] M32 explicitly does NOT author `samples/homelab/SKILL.md` / `TRIGGER.md` / `README.md` — that is M33_001's scope.
- [ ] M32 explicitly does NOT attempt any `zombiectl zombie install --from samples/<name>` E2E — the only valid install target in v2 (authored by M33_001) is `samples/homelab`; M32 does not exercise it.

---

## Acceptance Criteria

- [ ] `mint dev` builds cleanly with zero broken links — verify: `cd $DOCS_REPO && mint dev`
- [ ] Quickstart flow tested manually end-to-end in ≤10 minutes — verify: timed walkthrough
- [ ] Zero grep matches for removed pre-pivot vocabulary — verify: Eval E2
- [ ] Every `zombiectl` command referenced exists — verify: Eval E3
- [ ] New pages reachable from sidebar — verify: manual `mint dev` click-through
- [ ] Tenant-scoped $10 credit model consistent across billing + concepts + quickstart — verify: manual review

---

## Eval Commands

```bash
# E1: Mintlify builds cleanly
cd $DOCS_REPO && mint dev 2>&1 | tail -20

# E2: Stale vocabulary sweep (must return 0 matches outside changelog history)
grep -rni "credits redeem\|invite code\|access code" $DOCS_REPO/ \
  | grep -v -E "changelog\.mdx|\.git/" \
  || echo "VOCAB clean"

grep -rni "lead collector\|lead-collector" $DOCS_REPO/ \
  | grep -v -E "changelog\.mdx|\.git/" \
  || echo "LC clean"

# E3: CLI reference accuracy — each referenced zombiectl command must exist
for cmd in "zombie create" "zombie run" "zombie logs" "zombie trigger" "credential add"; do
  zombiectl $cmd --help >/dev/null 2>&1 && echo "OK: $cmd" || echo "MISS: $cmd"
done

# E4: Broken-link check (Mintlify reports during dev)
cd $DOCS_REPO && mint broken-links 2>&1 | tail -10 || true

# E5: Nav wiring
grep -c '"page"' $DOCS_REPO/docs.json
```

---

## Dead Code Sweep

Mandatory — this workstream deletes stale content.

| Content to delete | Verify deleted |
|-------------------|----------------|
| `credits redeem` references | `grep -rni "credits redeem" $DOCS_REPO/` → 0 (excluding changelog) |
| `invite code` / `access code` references | `grep -rni "invite code\|access code" $DOCS_REPO/` → 0 |
| Old `quickstart.mdx` lead-collector flow | page rewritten; git history preserves prior |
| Any nav entry pointing at deleted page | `mint dev` reports zero broken links |

---

## Verification Evidence

**Status:** PENDING — filled in during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Mintlify build | `mint dev` | | |
| Vocab sweep | Eval E2 | | |
| CLI reference | Eval E3 | | |
| E2E timed walkthrough | manual | | |
| Tenant-credit consistency | manual review | | |

---

## Out of Scope

- API reference regeneration — automated elsewhere (owned by API repo + OpenAPI pipeline).
- Marketing site changes (`usezombie.com`) — different repo, separate milestone.
- SEO content / blog posts — not part of the MVP docs surface.
- Video walkthroughs / animated GIFs — text + screenshots only for MVP.
- Internationalization — English only.
- Dark mode styling tweaks — Mintlify default is fine.
