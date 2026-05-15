# M70_001 — Handoff (parked May 15, 2026)

**Status:** IN_PROGRESS. §1–§8 DONE; VERIFY phase + CHORE(close) + PR not yet run.

## Scope / status

Implementing M70_001 — audit scripts default to full-codebase scope. Pre-commit was blind to staged content when audits ran against `BASE...HEAD`. M70 flips defaults to `git ls-files`-driven walks; index includes staged content, blindspot closed.

- ✅ §1 audit-ufs.sh — `--diff` retired, perf 40s → 4s
- ✅ §2 audit-design-tokens.sh — default flipped, perf 3.5s → 0.8s
- ✅ §3 audit-msid-ui.sh (renamed mid-flow from audit-combined.sh) — header docs, stays diff-shaped
- ✅ §4 make/harness.mk — drop scope flags, point at audit-msid-ui.sh
- ✅ §5 gate-body docs — Scope (M70) sections in 6 files
- ✅ §6 AGENTS_INVARIANCE — Scenario 22 + 4.1c text fix
- ✅ §7 Bonus perf — audit-logging 22s → 4.8s, audit-deinit-pairs 17s → 3.2s
- ✅ §8 Bonus cleanup — A1 (9 cross-runtime orphans renamed/deleted) + B1 (18 raw literals → named-key refs)
- ⏳ VERIFY phase: `/write-unit-test`, `/review`, `make test`, Zig cross-compile not yet run
- ⏳ CHORE(close): spec move active→done, PR

## Working tree

### usezombie worktree `~/Projects/usezombie-m70-audit-scope/`

```
$ git status -sb
## feat/m70-audit-scripts-full-codebase
 M docs/v2/active/M70_001_P1_INFRA_AUDIT_SCRIPTS_FULL_CODEBASE_SCOPE.md    # §1–§8 marked DONE
?? HANDOFF.md                                                              # this file
```

**4 commits ahead of `origin/main`, not pushed:**
```
7671769b fix(errors): A1 cross-runtime parity + B1 named-key presets
e650cb6e feat(harness): M70_001 §4 — full-codebase scope + audit-msid-ui rename
d48c2cd2 chore(m70): open M70_001 — audit scripts full-codebase scope
4b9463ec docs(m70): add spec — audit scripts default to full-codebase scope    # already on main
```

### dotfiles `~/Projects/dotfiles/`, branch `master`

Clean, fully pushed to `origin/master`. Three M70 commits landed:
```
56e578e perf(harness): batch audit-logging + audit-deinit-pairs into single-awk passes
d0f3bf6 docs(invariance): update 4.1c to reflect M70 default scope
a55d677 feat(harness): audit scripts default to full-codebase scope (M70_001)
```
Plus Captain's commits between mine (also pushed): `5ca82ab` (private-block tracking), `a2fd057` (drop PUB clause), and the `audit-combined.sh → audit-msid-ui.sh` rename.

## Branch / PR (GitHub)

- Branch: `feat/m70-audit-scripts-full-codebase`
- PR: **not opened yet**
- Forge: `gh` (github.com/usezombie/usezombie)

## Running processes

None. No tmux sessions, no dev servers, no background watchers.

## Tests/checks

- ✅ `make harness-verify` — **ALL GATES GREEN · 10.02s** (Captain accepted 15s as acceptance budget)
- ✅ Pre-commit hooks (gitleaks + audit-agents-md + make lint + redocly) ran clean on both usezombie commits
- ✅ dotfiles pre-push: invariance signoff passed (105/105 YES at HEAD `d0f3bf6`)
- ⏳ `/write-unit-test` — not run; **CHORE(close) violation if skipped**
- ⏳ `/review` — not run; required before CHORE(close) commits
- ⏳ `make test` (tier 1) — not run; lint already ran via pre-commit
- ⏳ `make test-integration` — not run; diff doesn't touch HTTP/schema/DB paths, tier-1 likely sufficient
- ⏳ Zig cross-compile (`zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`) — not run; mandatory per AGENTS.md L177. Zig change is 2 lines (pub const additions in `src/errors/error_registry.zig`); should be trivially clean but must verify.

## Next steps (ordered)

1. `cd ~/Projects/usezombie-m70-audit-scope`
2. **Zig cross-compile** (mandatory): `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
3. **VERIFY tier 1**: `make test` (lint already passed in pre-commit)
4. Decide on tier 2: `make test-integration` is required iff diff touches HTTP/schema/DB/Redis. M70 doesn't (only JS error-codes consts + Zig pub consts + harness scripts). Tier-1 is likely sufficient — confirm.
5. Invoke `/write-unit-test` — audits diff coverage. Likely flag: only `ERR_CREDIT_EXHAUSTED` has a wire-format pin test; consider extending to cover the other 7 renames (or note the audit suffices as a check).
6. Invoke `/review` — adversarial vs spec, ZIG_RULES.md (the 2 Zig pub const additions), AGENTS.md Action-Triggered Guards. Address or document deferrals.
7. **CHORE(close)**:
   - Spec `Status: DONE`, all §1–§8 already DONE
   - `git mv docs/v2/active/M70_001_*.md docs/v2/done/`
   - Update Verification Evidence table with E1–E5 results from spec
   - Delete `HANDOFF.md` (this file — ephemeral; must not ship in PR)
   - Commit: `chore(m70): close M70_001 — all sections DONE`
8. **Changelog**: M70 is harness/infra — internal-only. Skip `~/Projects/docs/changelog.mdx` `<Update>` per CHORE(close) rule ("skip iff internal-only"). Confirm by reading the release-template voice criteria.
9. `git push -u origin feat/m70-audit-scripts-full-codebase`
10. `gh pr create` with `## Session notes` block: lifecycle, A1/B1 surface bonus, perf passes, harness budget 10.02s, latent string-dup-file bug preserved deliberately.
11. `kishore-babysit-prs` auto-runs after push.

## Risks / gotchas

- **Latent string-dup-file bug in audit-ufs.sh preserved deliberately.** Subshell `while record` doesn't propagate violations. Fixing surfaces ~3019 pre-existing dups across the codebase. Separate cleanup spec needed (M70_002 candidate). Header comment documents the 3019 estimate.
- **`audit-spec-template.sh` symlink drift on `main`** (`M scripts/audit-spec-template.sh`, rel→abs path). Pre-existing; explicitly out of scope per prior HANDOFF directive.
- **Stale `HANDOFF.md` at repo root on `main`** (from prior M69 session). Not in this worktree's diff; left for whoever owns that follow-up.
- **`bun install` was run** in usezombie worktree to hydrate oxlint for the pre-commit hook (1080 packages, `node_modules` gitignored, no diff impact). Next pickup may need to repeat if env is fresh.
- **Captain's mid-flow rename of audit-combined.sh → audit-msid-ui.sh + PUB clause drop** (dotfiles commits `5ca82ab`, `a2fd057`). My usezombie wiring follow-up landed in `e650cb6e`. Anything else referencing "audit-combined" (outside M70 docs) may need a sweep — `bin/sync-agents` has no mapping for it (checked).
- **Duplicate UZ-BILLING-001 in Zig registry.** Added `pub const ERR_BILLING_UNAVAILABLE = "UZ-BILLING-001"` alongside existing private `const ERR_BILLING_INVALID_SUBSCRIPTION_ID = "UZ-BILLING-001"`. Only one is pub, so not a UFS violation. The private one is dead per RULE NDC — clean-up candidate for a follow-up, not in M70 scope.
- **Harness budget 10.02s.** 0.02s over the spec's ≤10s aspirational target. Captain explicitly accepted 15s as the new floor ("4 harness budget 15s is fine"). Spec text could be updated in CHORE(close); currently the §7 description carries the note.
- **PUB GATE technically fires** on the 2 Zig pub const additions in `src/errors/error_registry.zig` per AGENTS.md ("any new ^pub line in new-bytes"). Shape verdict: registry-of-consts pattern; new consts match existing shape verdict; both serve genuine consumers (Zig server + JS CLI mirror). No inheritance. Document in `/review` output / PR Session Notes.
- **No `/review` yet** — required before CHORE(close) commits per AGENTS.md skill chain. If MCP-backed, log skip in PR Session Notes per AGENTS.md L210 if unavailable.

## Reading order for ramp-up

1. This file (`~/Projects/usezombie-m70-audit-scope/HANDOFF.md`)
2. The spec: `docs/v2/active/M70_001_P1_INFRA_AUDIT_SCRIPTS_FULL_CODEBASE_SCOPE.md` (§1–§8 marked DONE)
3. The 4 commits in this branch (`git log --oneline main..HEAD`)
4. The dotfiles M70 commits (`cd ~/Projects/dotfiles && git log --oneline -10`)
5. AGENTS.md Action-Triggered Guards table (for the PUB GATE call on the Zig change)
