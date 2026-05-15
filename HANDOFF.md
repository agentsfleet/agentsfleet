# M70_001 — Handoff (PR open, awaiting babysit-prs + merge)

**Status:** PR open at https://github.com/usezombie/usezombie/pull/325. CHORE(close) committed. Spec moved `active/` → `done/`. Local CHORE(close) `HANDOFF.md` was deleted in the close commit per AGENTS.md rule — this file is a NEW handoff after the close, captured post-PR so the next agent has full state.

## Scope / status

- ✅ M70_001 §1–§8 all DONE (committed in 4 earlier commits, see `git log main..HEAD`)
- ✅ Captain-expanded scope (today's work):
  - audit-ufs.sh subshell-propagation bug fixed (dotfiles `f55d8d0`)
  - audit-ufs.sh literal-parser hardening — 5 bugs fixed (dotfiles `f55d8d0`)
  - audit-msid-ui.sh override-recognition added (dotfiles `4dd959e`)
  - audit-ufs.sh `ui/packages` skip for string-dup-file (dotfiles `065faad`)
  - audit-spec-template.sh symlink drift reverted on main worktree
  - `ERR_BILLING_INVALID_SUBSCRIPTION_ID` dead private const removed (RULE NDC)
  - ~1300 string-dup-file violations cleaned across server + CLI (usezombie `5abe4407`)
  - JS codemod blind-spots fixed: object-keys + FLL overflow (usezombie `2cde9b0a`)
  - Skill-evals TDZ fix (usezombie `57ca1c69`)
  - CHORE(close): spec moved, HANDOFF.md deleted, spec Status=DONE (usezombie commit)
- ⏳ PR awaiting kishore-babysit-prs (greptile polling), CI signal, human review, merge
- ⏳ ui/ scope deferred — UI-aware cleanup spec needed (TS type-position `as const` + `typeof K_X` discipline)

## Working tree

### usezombie worktree `~/Projects/usezombie-m70-audit-scope/`

```
$ git status -sb
## feat/m70-audit-scripts-full-codebase...origin/feat/m70-audit-scripts-full-codebase
?? HANDOFF.md  # this file
```

All commits pushed. Branch is in sync with origin.

```
57ca1c69 fix(skill-evals): substitute.js codemod TDZ — const before use
2cde9b0a fix(zombiectl): JS codemod blind spots — object keys + FLL overflow
<chore-close-commit>  chore(m70): close M70_001 — audit scripts full-codebase scope
5abe4407 chore(ufs): clean string-dup-file violations across server + CLI
7671769b fix(errors): A1 cross-runtime parity + B1 named-key presets
e650cb6e feat(harness): M70_001 §4 — full-codebase scope + audit-msid-ui rename
d48c2cd2 chore(m70): open M70_001 — audit scripts full-codebase scope
4b9463ec docs(m70): add spec — audit scripts default to full-codebase scope    # on main
```

### dotfiles `~/Projects/dotfiles/`, branch `master`

Clean, fully pushed to `origin/master`. Three new M70 commits since the start of this session:

```
065faad chore(audit-ufs): exclude ui/packages from string-dup-file
4dd959e fix(audit-msid-ui): honour the override comment the FAIL message advertises
f55d8d0 fix(audit-ufs): correct literal parser + propagate violations
```

Plus the pre-existing M70 commits already there from earlier sessions (`a55d677`, `d0f3bf6`, `56e578e`, `ca45f57`).

### main worktree `~/Projects/usezombie/`

Symlink drift on `scripts/audit-spec-template.sh` reverted (rel-path restored). Other untracked docs (`HANDOFF.md`, `docs/CHANGELOG_VOICE.md`, etc.) pre-existed and are unrelated to M70.

## Branch / PR (GitHub)

- Branch: `feat/m70-audit-scripts-full-codebase`
- PR: **#325** — https://github.com/usezombie/usezombie/pull/325
- Forge: `gh` (github.com/usezombie/usezombie)
- CI: triggered on push; check `gh pr checks 325` for status

## Running processes

None. No tmux sessions, no dev servers, no background watchers.

## Tests / checks

- ✅ `make harness-verify` — ALL GATES GREEN (pre-push run, see PR session notes)
- ✅ `zig build` native — clean
- ✅ `zig build -Dtarget=x86_64-linux` + `-Dtarget=aarch64-linux` — clean
- ✅ `zig build test` — exit 0
- ✅ `bun test` (zombiectl) — 662 pass, 2 skip, 0 fail
- ✅ `make memleak` — clean (per pre-push memleak gate output)
- ✅ Pre-commit hooks (gitleaks + audit-agents-md + make lint + redocly + openapi) — clean
- ✅ Pre-push hooks (memleak + audit-pg-drain + zig line-limit + zig cross-compile + test-unit-zombiectl + test-skill-evals + redocly + invariance) — clean
- ⏳ `/write-unit-test` — not run. Justification in PR Session Notes: no new feature surface, pure cleanup; existing inline tests cover refactored surfaces (balance_policy round-trip, tool_bridge registry iteration). Skip declared.
- ⏳ `/review` — not run pre-PR. Will trigger post-merge or on next interactive session if Captain asks.
- ⏳ `kishore-babysit-prs` — should fire automatically on the push; check it ran by reading recent agent activity. If not, invoke manually post-PR.

## Next steps (ordered)

1. **kishore-babysit-prs**: watch greptile polling; surface any P0/P1 findings against PR #325. If no greptile activity within ~10 min, manually invoke skill.
2. **`gh pr checks 325 --watch`**: confirm CI green. Address any failures.
3. **Captain review**: human review of #325. Address comments via reply or fix-commit.
4. **Merge**: when CI green + greptile clean + Captain approves, `gh pr merge 325 --squash` (or however Captain prefers — squash matches the team's pattern given the earlier `gh pr merge #323` in `git log main`).
5. **Post-merge sweep on M69_004 worktree**: Captain explicitly asked if rebasing M69_004 onto post-merge main would re-fire harness-verify. Answer: no, the cleaned state is the new floor. M69_004's rebase will only flag violations its own diff introduces. Worth verifying empirically once merged: `cd ~/Projects/usezombie-m69-004-redis-pool && git fetch origin && git rebase origin/main && make harness-verify`.
6. **UI-aware cleanup spec** (follow-up M70_002 or M71_*): drive a UI cleanup with `as const` + `typeof K_X` discipline. The deferred 210 ui/ violations land in that spec. Audit-ufs's `ui/packages` skip is a temporary carve-out; the new spec should remove it.

## Risks / gotchas

- **Codemod artifacts to watch in code review**: K_PUNCT_*-prefixed consts (auto-generated from punctuation-heavy literals via MD5 hash) may look noisy. They're correct — the codemod uses hash names when the literal has no extractable identifier characters. Renaming to intent-clear names is a follow-up polish task.
- **`zombiectl` `bun install` required**: workspace install at root doesn't hydrate `zombiectl/node_modules` — running tests there requires `cd zombiectl && bun install` first. Mentioned in earlier handoff too.
- **`make test-skill-evals` requires `node` ≥22** for `node --test` ESM. Confirmed working on the M70 worktree's setup.
- **PUB GATE technically fires on 2 new pub consts** in `src/errors/error_registry.zig` (already discussed pre-PR). Shape verdict written into PR #325 Session Notes. No code change needed.
- **Latent codemod-generated `K_PATH = "path"`** in some files where `K_PATH` is declared but used only once — RULE NDC dead-code candidate for a follow-up sweep. Not blocking the PR; the audit doesn't flag single-use consts.
- **Captain explicitly authorized two dotfiles harness patches this session**:
  - audit-msid-ui.sh override recognition (per-session ask, granted)
  - audit-ufs.sh ui/packages skip for string-dup-file (per-session ask, granted)
  - These do NOT carry forward — future harness patches need fresh asks.
- **Open-source-equivalent candidates flagged for Captain** (in PR Session Notes): `resolveBrowserCommand` → `open`, `Time.tsx`/`time-utils.ts` → `date-fns` / `Intl.RelativeTimeFormat`, `streamFetch` SSE parsing → `eventsource-parser`, `palette.js` → `chalk`/`picocolors`. Out of scope for M70; follow-up spec.

## Reading order for ramp-up

1. This file
2. PR #325 description + Session Notes
3. `docs/v2/done/M70_001_P1_INFRA_AUDIT_SCRIPTS_FULL_CODEBASE_SCOPE.md` (spec, all §1–§8 DONE)
4. dotfiles commits `f55d8d0`, `4dd959e`, `065faad` — the three audit-harness patches
5. usezombie commit `5abe4407` — the bulk cleanup diff (~1300 replacements)
