# Handoff — execute M66_001

**Date:** 2026-05-10
**Captain:** Kishore
**Author:** Claude Opus 4.7 (1M context)
**Status:** Spec authored, awaiting CHORE(open) and implementation

The spec is at `docs/v2/pending/M66_001_P1_API_CLI_DOCS_UI_BYOK_RETIREMENT_AND_TRACTION_RATES.md`. This handoff is the operator's manual to land it.

---

## Where things stand right now

### Open PRs (all need merge before M66_001 implementation begins)

| # | PR | Repo | Contents | Why it gates M66_001 |
|---|---|---|---|---|
| 1 | [usezombie/docs#47](https://github.com/usezombie/docs/pull/47) | docs | M65 docs polish: BYOK prose retirement, stealth-mode banner, rate-snippet rewire, 28-entry changelog brevity pass. Mergeable, all checks ✅. | M66_001 §3 (BYOK doc-site sweep) and §4 (website rate update) edit the same files. Land #47 first to avoid merge contention. |
| 2 | [usezombie/usezombie#311](https://github.com/usezombie/usezombie/pull/311) | usezombie | Website stealth-mode banner + canonical contact email on `Pricing.tsx`. 13/13 tests pass, lint clean. | M66_001 §4 + §5 build on this. Land #311 first. |
| 3 | [usezombie/usezombie#312](https://github.com/usezombie/usezombie/pull/312) | usezombie | M66_001 spec + `docs/v2` housekeeping + `audit-spec-template.sh` dotfiles symlink. | The spec itself plus the symlink. Land before CHORE(open) so the spec sits on `main` per kishore-spec-new convention. |

**Recommended merge order:** #311 → #47 → #312, then proceed to CHORE(open) for M66_001.

### Dotfiles state

- Branch `master` at `2a65eda` (pushed). `scripts/audit-spec-template.sh` now lives in dotfiles; `bin/sync-agents` `PROJECT_LINKS` includes the entry. Future projects pick it up via `~/bin/sync-agents`.
- Working tree has one pre-existing modification (`docs/gates/ui-substitution.md`) — not part of this work.

---

## Pre-CHORE(open) checklist

Run from `~/Projects/usezombie/` once #311, #47, #312 have merged:

```bash
# 1. Bring local main up to date
cd ~/Projects/usezombie
git checkout main
git pull --ff-only origin main
git status   # should be clean except for the pre-existing M50_001 spec mod

# 2. Verify the spec landed
ls -la docs/v2/pending/M66_001_P1_API_CLI_DOCS_UI_BYOK_RETIREMENT_AND_TRACTION_RATES.md

# 3. Verify the audit symlink resolves
readlink scripts/audit-spec-template.sh
# expect: /Users/kishore/Projects/dotfiles/scripts/audit-spec-template.sh
bash scripts/audit-spec-template.sh | tail -5
# expect: SPEC TEMPLATE GATE: clean

# 4. Sister repo check (docs site)
cd ~/Projects/docs
git checkout main
git pull --ff-only origin main
git status   # should be clean

# 5. Org profile
cd ~/Projects/.github
git checkout main && git pull --ff-only origin main && git status
```

---

## CHORE(open) — exact sequence

Per AGENTS.md, four steps land in one commit before any code is written:

```bash
cd ~/Projects/usezombie

# 1. Move the spec from pending/ to active/
git mv docs/v2/pending/M66_001_P1_API_CLI_DOCS_UI_BYOK_RETIREMENT_AND_TRACTION_RATES.md \
       docs/v2/active/M66_001_P1_API_CLI_DOCS_UI_BYOK_RETIREMENT_AND_TRACTION_RATES.md

# 2. Edit the spec to set Status: IN_PROGRESS and Branch: feat/m66-001-byok-retirement
#    (Use Edit tool — single field swap.)

# 3. Create the feature branch + worktree
git checkout -b feat/m66-001-byok-retirement main
git worktree add ../usezombie-m66-001-byok-retirement feat/m66-001-byok-retirement

# 4. Move into the worktree, commit the CHORE(open) move
cd ../usezombie-m66-001-byok-retirement
git add docs/v2/active/M66_001_P1_API_CLI_DOCS_UI_BYOK_RETIREMENT_AND_TRACTION_RATES.md
git rm docs/v2/pending/M66_001_P1_API_CLI_DOCS_UI_BYOK_RETIREMENT_AND_TRACTION_RATES.md  # if needed
git commit -m "chore(m66-001): open — Status IN_PROGRESS, Branch field, spec → active/"

# Now you may write code. Stay inside ../usezombie-m66-001-byok-retirement until VERIFY → PR.
```

---

## Implementation order across the six sections

The spec is one workstream with six sections; sequence them inside the worktree:

| Order | Section | Why this order |
|---|---|---|
| 1 | **§1 nanos unit migration** | Schema migration + Zig constant rename. Blocks every consumer. |
| 2 | **§2 M66 traction rates** | Drops in the new constant values. Builds on §1's nanos unit. Update Zig + TS + paired pin tests. |
| 3 | **§5 single SUPPORT_EMAIL** | Cheap and orthogonal — five new constants, one sweep. Lands before §3 so the website/app/docs work in §3+§4 references the constant from day one. |
| 4 | **§3 BYOK retirement** | Schema enum value, Zig enum, API wire format (clean break — no alias), TS, app component rename, CLI flag, architecture-doc rename. The single biggest section. |
| 5 | **§4 website pricing surface fix** | Pricing.tsx + FAQ.tsx + lib/rates.ts. Surfaces the §1+§2 rates with the platform-vs-self-managed gradient and "stealth-mode testing rate — will rise post-GA" framing. |
| 6 | **§6 documentation currency audit** | Walk every spec under `docs/v2/done/` and grep-confirm docs site, architecture docs, READMEs are aligned. Surface drift; either fix in this PR (mechanical) or file as follow-up specs (`docs/v2/pending/M66_NNN_DOCS_DRIFT_*.md`). |

Each section should land as one or more commits with a clear conventional-commit message naming the section number.

---

## Cross-repo coordination

The spec touches three repos. Branches in each:

| Repo | Branch | Contents | Order |
|---|---|---|---|
| `usezombie/usezombie` | `feat/m66-001-byok-retirement` (lead) | Schema + Zig + app + website + CLI + arch docs + paired pin tests + new SUPPORT_EMAIL constants. | 1 — leads. |
| `usezombie/docs` | `feat/m66-001-byok-retirement-docs` | `~/Projects/docs/snippets/rates.mdx` flip to nanos values; new `snippets/contact.mdx`; BYOK prose sweep across `concepts.mdx`, `index.mdx`, `quickstart.mdx`, `zombies/credentials.mdx`, etc.; new `<Update>` block in `changelog.mdx` announcing the M66 rate cut + term retirement. | 2 — opens after lead PR is in review. |
| `usezombie/.github` (org profile) | `chore/m66-001-readme-sweep` | `profile/README.md` BYOK references + email literal. Single-file change, can ship as a tiny PR. | 3 — independent; ship anytime. |

**Coordination rule:** the docs PR's `<Update>` block in `changelog.mdx` is the canonical announcement surface for the rate cut + term retirement. It lands after the usezombie PR's content is locked (no further rate changes mid-review).

---

## Critical gotchas

1. **The classifier blocks direct push to `main` on usezombie.** Spec authoring lives on main per the kishore-spec-new convention, but the auto-mode classifier requires a PR for any push that touches main directly. PR #312 was the workaround. Plan for the same on any future direct-to-main commits.
2. **`audit-spec-template.sh` is now a symlink.** Don't edit it in `~/Projects/usezombie/scripts/` — the change won't persist. Edit at `~/Projects/dotfiles/scripts/audit-spec-template.sh`, then `git -C ~/Projects/dotfiles add scripts/ && git commit && git push origin master`.
3. **Pre-v2.0 RULE NLG forbids legacy scaffolding.** This spec REMOVES the BYOK identifier; it does not preserve it as scaffolding. No `--byok` CLI alias, no `mode: "byok"` API accept, no `Mode.byok` deprecated variant. Clean break is the contract.
4. **Cents → nanos scaling is `× 10,000,000`** (1¢ = 10M nanos), not `× 1,000` (which would be cents → mills) or `× 1,000,000` (cents → micros). This is easy to get wrong on auto-pilot.
5. **`i64` BIGINT max = 9.22e18 nanos = ~$9.22B per tenant.** Realistic balances are 4–5 orders of magnitude below. Migration includes a pre-flight overflow assertion (`SELECT MAX(balance_cents) FROM core.tenant_billing` < 9e11) before scaling.
6. **The "Implementation default" framing.** Spec body uses `Implementation default: <choice> because <reason>` for non-obvious decisions (PG14 `ALTER TYPE RENAME VALUE` for the enum migration, `git mv` for file renames to preserve history, etc.). Defer to those defaults unless you have evidence to deviate; if you deviate, log in the spec's Discovery section.
7. **Documentation currency audit (§6) is real work.** ~50 specs under `docs/v2/done/` need a walkthrough. Don't skip it — Captain explicitly asked for it. Drift findings are either mechanical (fix in this PR) or filed as follow-up specs.

---

## Verification skeleton (fill in as you go)

This maps to the spec's Acceptance Criteria. Run before opening the PR:

```bash
cd ~/Projects/usezombie-m66-001-byok-retirement

# Tier 1 — every iteration
make lint
make test

# Tier 2 — once the diff touches HTTP/schema/DB/Redis paths
make test-integration

# Tier 3 — once before ship-ready, from clean state
make down && make up && make test-integration

# Lifecycle / leak / cross-compile
make memleak | tail -3
make check-pg-drain
zig build -Dtarget=x86_64-linux
zig build -Dtarget=aarch64-linux

# Hygiene
gitleaks detect | tail -3
make check-version

# Spec-specific sweeps (must all return 0)
grep -rn '\bBYOK\b' src/ ui/ zombiectl/ public/ docs/architecture/ \
  | grep -v -E '(historical|legacy lineage)' | wc -l
grep -rn 'hello@usezombie\.com' src/ ui/ zombiectl/ docs/ public/ | wc -l
grep -rn 'EVENT_BYOK_CENTS\|STAGE_CENTS\|EVENT_PLATFORM_CENTS\|STARTER_CREDIT_CENTS' \
  src/ ui/ zombiectl/ | wc -l   # should be 0 — old constants gone

# Schema enum sanity (against running dev DB)
psql "$DATABASE_URL" -c "SELECT enum_range(NULL::tenant_provider_mode)"
# expect: {platform,self_managed}
```

Paste the actual command output into the spec's `## Verification Evidence` table during VERIFY — not paraphrased, the real lines.

---

## CHORE(close) skill chain (mandatory order)

Per AGENTS.md, four skills gate the lifecycle from implementation-complete to PR-merged:

| # | When | Skill | What it does |
|---|---|---|---|
| 1 | After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage against the spec's Test Specification. Catches happy-path-only tests, missing negatives, fixture drift. |
| 2 | After tests pass, still before CHORE(close) | `/review` | Adversarial diff review against this spec, `docs/architecture/billing_and_provider_keys.md`, `docs/REST_API_DESIGN_GUIDELINES.md`, `docs/ZIG_RULES.md`, Failure Modes, Invariants. |
| 3 | After `gh pr create` opens the PR | `/review-pr` | Comments on the open PR against the now-immutable diff. Catches what the local `/review` missed. |
| 4 | After every push | `kishore-babysit-prs` | Polls Greptile per cadence, walks every review id, triages P0/P1 vs RULES.md, fixes+replies+reschedules. Stops on two consecutive empty polls. |

CHORE(close) outputs (per AGENTS.md):

- All Dimensions/Sections marked DONE in the spec body.
- Spec moved `docs/v2/active/` → `docs/v2/done/`.
- New `<Update>` block in `~/Projects/docs/changelog.mdx` (template + version-bump matrix in `~/Projects/dotfiles/skills/release-template.md` — re-source each release, never paraphrase).
- PR `## Session notes` recording decisions, assumptions, dead ends, deferrals, `/write-unit-test` + `/review` outcomes, `kishore-babysit-prs` final report.
- Orphan sweep clean (RULE ORP — every renamed/deleted symbol → 0 hits).
- Working tree clean before PR open/update.
- `make check-version` passes (VERSION bumped if user-visible release).

---

## Quick state map at handoff

```
~/Projects/usezombie/        main, ahead of origin/main by 6 commits
                              (5 pre-existing + the M66 spec; PR #312 carries them)
                              chore/m66-spec-and-dotfiles-cleanup pushed → PR #312
                              ?? docs/v2/HANDOFF_M65_DOCS_REPHRASE.md  (untracked)
                              ?? docs/v2/HANDOFF_M65_M66_SESSION.md   (untracked)
                              ?? docs/v2/HANDOFF_M66_001_EXECUTE.md   (this file, untracked)
                              ?? docs/v2/PROPOSAL_M66_PRICING_BYOK_EMAIL.md  (untracked)
                              modified: docs/v2/pending/M50_001_*.md (pre-existing)
~/Projects/docs/             chore/m65-byok-marketing-rephrase, in sync, clean → PR #47
~/Projects/dotfiles/         master at 2a65eda (pushed); audit-spec-template.sh moved + sync-agents manifest updated
~/Projects/.github/profile/  not touched this session
```

---

🤖 Authored by Claude Opus 4.7 (1M context). Hand off whenever.
