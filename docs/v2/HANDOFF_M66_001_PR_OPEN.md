# Handoff — open the M66_001 lead PR + companion docs PR + skill chain tail

**Date:** May 11, 2026
**Captain:** Kishore
**Author:** Claude Opus 4.7 (1M context)
**Status:** Branch fully ready. Force-push landed at `c0f666c6`. Two PRs left to open + the post-`gh pr create` skill chain (`/review-pr` + `kishore-babysit-prs`).

---

## What's done

§1–§6 all landed and pushed. Spec moved to `docs/v2/done/`. Companion docs branch pushed to `usezombie/docs#feat/m66-001-byok-retirement-docs` (`11290fe`). CHORE(close) committed. Adversarial review (`/review` skill) caught four real items pre-PR; all four were fixed inline:

1. Branch was 6 commits behind `origin/main` and would have silently reverted M67 (oxlint migration). **Rebased.** History rewritten cleanly; force-pushed with `--force-with-lease` after Captain's explicit auth. Pre-push hook ran 1288 tests, 0 failed, memleak gate passed.
2. Spec-close edits (Status DONE, per-section DONE markers, Verification Evidence table, Session Notes) hadn't been staged before the `git mv` ran during CHORE(close) — landed in the wrong commit. **Committed as `d4a8a842`.**
3. `src/zombie/metering.zig` was carrying nanos values through variables named `cents` and a structured-log field `.cents = cents`. Loki/Grafana would have misread by 10⁷×. **Swept** to `nanos` / `.nanos` / `.nanos_attempted`. Pure rename, no semantic change. Bundled with #4 in `c0f666c6`.
4. Spec body asserted a special-case `UZ-PROVIDER-MODE-RENAMED` error code, a `Mode.parse()` Zig helper, and a `--byok` / `--self-managed` CLI flag pair that the shipped implementation never adopted (RULE NLG ruled them out — Session Notes #2 captured the decision). **Spec body reconciled** so it reads against the code that shipped: byok rejection flows through the generic `UZ-REQ-001` `ERR_INVALID_REQUEST` path, Mode rejection is HTTP-layer string compare (no helper), CLI surface is `tenant provider add --credential <name>`. Earlier contractual assertions are struck through with one-line "Superseded — …" notes so original intent survives in the archive. Bundled with #3 in `c0f666c6`.

**Verification at handoff:**

- Zig 29/29 · skill-evals green
- Website vitest 129/129 · App vitest 357/357 · zombiectl bun test 567/567
- `make lint` green under oxlint (post-M67)
- Cross-compile both `x86_64-linux` + `aarch64-linux` green
- Gitleaks clean (1715 commits scanned, no leaks)
- `audit-ufs.sh --diff`: 4 baseline violations, all by-design (3 PascalCase-enum vs JS SCREAMING_SNAKE — `CHARGE_TYPE` / `PROVIDER_MODE` / `SELF_MANAGED_SENTINELS`; 1 presentation-only `RATES_DISPLAY`)
- Pre-push hook (`1288 passed; 221 skipped; 0 failed; memleak gate passed`)
- **Integration suite (Tier 2) was 1508/0 at `b6959357` pre-rebase**; not re-run post-rebase because the only post-rebase delta is variable-rename in `metering.zig` (no semantic change) plus doc-only spec edits, and Captain rejected `make up` during the verification pass.

**Branch:** `feat/m66-001-byok-retirement` on `usezombie/usezombie`. Tip = `c0f666c6`.
**Worktree:** `~/Projects/usezombie-m66-001-byok-retirement/`.
**Spec:** `docs/v2/done/M66_001_P1_API_CLI_DOCS_UI_SELF_MANAGED_RETIREMENT_AND_TRACTION_RATES.md` (Status: DONE).
**Companion docs branch:** `usezombie/docs#feat/m66-001-byok-retirement-docs` (tip `11290fe`). Already pushed; PR not yet opened.

---

## What's left

### 1. `gh pr create` for the lead PR

Branch `feat/m66-001-byok-retirement` against `main` on `usezombie/usezombie`. PR title and body should track the changelog `<Update>` voice from the spec body + Session Notes. Suggested title:

```
feat(m66): BYOK→self_managed retirement + nanos billing unit + posture-dispatched stage gradient
```

Body must include:

- **Summary** — three-to-five bullets covering the spec's §1–§6 outcomes (nanos unit, traction rates, BYOK retirement, website pricing rewrite, SUPPORT_EMAIL constant, docs depin).
- **Breaking changes** — `mode: "byok"` → 400 generic fall-through, `balance_cents` → `balance_nanos` schema rename, internal constant renames (`STARTER_CREDIT_CENTS` → `STARTER_CREDIT_NANOS`, `STAGE_CENTS` split into `STAGE_PLATFORM_NANOS` + `STAGE_SELF_MANAGED_NANOS`, `EVENT_PLATFORM_CENTS` collapsed into `EVENT_NANOS = 0`), `_cents_per_mtok` model-rate columns → `_nanos_per_mtok` BIGINT. Forward-only — no migration, `make down && make up` for local dev.
- **Test plan** — checkbox list copied from the spec's Acceptance Criteria. Note the integration-suite footnote about pre-rebase verification.
- **Companion PR** — link to the docs PR once it's open (open the docs PR right after the lead PR, so the cross-link goes both ways).

Captain's earlier "skip .github/profile" decision stands: don't touch the org-profile README literal in this PR.

### 2. `gh pr create` for the companion docs PR

In `~/Projects/docs/` on branch `feat/m66-001-byok-retirement-docs` (already pushed). Open against `main`. Title:

```
docs(m66): nanos shape + posture-dispatched stage rates + SUPPORT_EMAIL snippet
```

Body should explain the snippet renames (`rates.mdx` now exports `STARTER_CREDIT` / `EVENT_RATE` / `STAGE_PLATFORM` / `STAGE_SELF_MANAGED`, new `contact.mdx` snippet), the historical-entry carve-out (`EVENT_RATE_M65` / `STAGE_RATE_M65` placeholders for the May 9 entry), the broken cross-reference fix (`billing_and_byok.md` → `billing_and_provider_keys.md`), and the new `<Update label="May 11, 2026">` block. Cross-link the lead PR.

### 3. `/review-pr` on the lead PR

Skill runs after `gh pr create` opens the PR. Comments via `gh pr review`. Address inline before requesting human review or merging.

### 4. `kishore-babysit-prs`

After every push the lead PR will get Greptile + bot reviews. Run `kishore-babysit-prs`; it polls per cadence, walks every review id, triages P0/P1 vs `docs/greptile-learnings/RULES.md`, fixes + replies + reschedules. Stops on two consecutive empty polls.

---

## Standing decisions still in force

- **No force-push without explicit ask.** Captain authorized the one rebase force-push for this milestone (memory `feedback_sync_main_before_pr.md`); future pushes are non-force `git push origin feat/m66-001-byok-retirement`.
- **Lead PR + paired docs PR only.** Skip `.github/profile` per Captain's earlier directive.
- **Pre-v2.0 RULE NLG.** Forward-only schema, no migration scripts, no legacy-aware error codes, no compat shims. Embedded in spec body Session Notes #2.
- **Cross-tier role names identical across Zig/TS/JS for every domain constant.** Presentation-only maps (`RATES_DISPLAY`) and PascalCase enum types (`Mode`, `ChargeType`) are exempt — documented as baseline UFS violations.
- **No effort estimates / no time budgets in any spec.** TEMPLATE.md "Prohibited" enforced by SPEC TEMPLATE GATE.
- **`make migrate` does not exist** — schema reseed is `make down && make up`.
- **Captain rejects `make up` requests when the local Docker state is mid-something** — assume the integration infra is the Captain's territory; do not auto-bring-up.
- **Auto-mode autonomy covers `git push origin <feature-branch>` (non-force), `git commit`, `gh pr create`, `/review-pr`. Force-push is NOT covered without explicit ask each time.**

---

## Resume commands

```bash
# 1. Bring local main in sync (won't change the worktree)
cd ~/Projects/usezombie && git fetch origin main

# 2. Enter the worktree
cd ~/Projects/usezombie-m66-001-byok-retirement
git status   # should be clean except this handoff doc

# 3. Confirm the lead branch
git log --oneline -3
# expected tip: c0f666c6 fix(m66-001): pre-landing review fixes — metering log field + spec reconciliation

# 4. Open the lead PR (gh pr create)
gh pr create --base main --title "..." --body "$(cat <<'EOF'
... see Body checklist above ...
EOF
)"

# 5. Open the companion docs PR
cd ~/Projects/docs
git status   # branch feat/m66-001-byok-retirement-docs, tip 11290fe
gh pr create --base main --title "..." --body "..."

# 6. Run /review-pr on the lead PR

# 7. Run kishore-babysit-prs on the lead PR (will reschedule itself)

# 8. Delete this handoff doc before merging the lead PR — ephemeral handoff
#    docs are forbidden in source history per AGENTS.md ("ephemeral handoff
#    docs deleted before PR").
```

---

## Open questions parked

None outstanding. Every adversarial-review finding was either fixed or explicitly classified as out-of-scope / baseline.

🤖 Authored by Claude Opus 4.7 (1M context). Hand off whenever.
