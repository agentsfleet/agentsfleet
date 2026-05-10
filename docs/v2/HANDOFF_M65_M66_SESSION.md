# Handoff — M65 docs polish + M66 proposal

**Date:** 2026-05-10
**Captain:** Kishore
**Author:** Claude Opus 4.7 (1M context), this session
**Status:** Two PRs open, one larger spec drafted, awaiting Captain review

---

## What you asked for, in order

1. **Continue from the M65 handoff** — finish the docs PR and land the AGENTS.md voice block.
2. **Fix Greptile feedback** on PR #47 (2 inline comments + 1 follow-up).
3. **Address one more inline review** on `concepts.mdx:34` (`model:` → `provider:`).
4. **Drop the hardcoded $31.00 worked example** from the M65 changelog entry.
5. **Pricing reality check** — too pricey, what would I suggest, given competition + design-partner stage?
6. **Lock in pricing direction** — events $0, stage $0.001, retire BYOK, single support email, fix it across website / docs / CLI / app / usezombie / orgs README.
7. **TEMPLATE.md determinism upgrades** — what would I change so an executing agent has fewer holes to fall into.
8. **Run /review** on the open work.
9. **Apply the KiloCode "you bring your provider, model... we sunset BYOK" framing**, swap **Early Access Preview → stealth-mode testing** with a single contact email, mirror on the website, then write this handoff.

You picked answers along the way: D1 = micros (1/1M USD, future-proofed past $0.0001), D2 = aggressive BYOK retirement at every tier, D3 = reframe the design-partner block (don't waive charges).

---

## What shipped this session

### Landed (or pushed and ready)

| # | Repo | Branch / PR | Contents | Status |
|---|---|---|---|---|
| 1 | `~/Projects/dotfiles/` | commit `8812ce0` on `master` | `## Changelog voice (Mintlify-style)` section in `AGENTS.md`. Invariance audit + sign-off green. | **Pushed.** |
| 2 | `usezombie/docs` | `chore/m65-byok-marketing-rephrase` → [PR #47](https://github.com/usezombie/docs/pull/47) | 11 commits net `c2e0aac → 7c29ceb`. Greptile fixes (`8224813`, `603e562`, `d607b0f`), full rate-snippet rewire (`8d4d302`), KiloCode "you bring your provider" rephrase + stealth-mode banner (`7c29ceb`). | **Open + MERGEABLE**, all checks ✅. |
| 3 | `usezombie/usezombie` | `feat/m65-stealth-mode-banner` → [PR #311](https://github.com/usezombie/usezombie/pull/311) | `Pricing.tsx` design-partner block reframed to stealth-mode, contact email switched to `usezombie@agentmail.to`, paired test updated. `bun run test` 13/13. `make lint` clean. | **Open.** |
| 4 | `usezombie/usezombie` (untracked, this repo) | `docs/v2/PROPOSAL_M66_PRICING_BYOK_EMAIL.md` | Cross-repo proposal covering micros migration, full BYOK retirement at every tier, single support-email constant per repo. Pre-spec; not on a branch. | **Drafted, awaiting your go to author specs.** |
| 5 | `usezombie/usezombie` (untracked, this repo) | `docs/v2/HANDOFF_M65_M66_SESSION.md` (this file) | This handoff. Supersedes the prior `HANDOFF_M65_DOCS_REPHRASE.md`. | **Drafted.** |

### Greptile reply log on PR #47

All four review comments addressed and threaded:

| Comment | File:line | Resolution |
|---|---|---|
| `r3214221422` | `concepts.mdx:34` (`model:` → `provider:`) | Fixed in `603e562`. Reply: ✅ |
| `quickstart.mdx:42` (starter-credit inference claim) | P1 | Fixed in `8224813` — clause aligns with BYOK framing. Top-level reply. |
| `snippets/rates.mdx:12-14` (rates coupling) | P2 | Comment added per suggestion in `8224813`; full rewire in `8d4d302`. |
| `changelog.mdx:26` (hardcoded `$31.00`) | New comment, P2 | Worked example dropped in `d607b0f`. Top-level reply. |

---

## The pricing call you locked in

Three decisions, with my recommendation alongside:

| # | Question | Your call | Effect |
|---|---|---|---|
| D1 | Cents → mills, or further? | **Micros (1/1M USD)** | One migration covers $0.001 today and $0.0001 / $0.00001 in the future without re-migration. `balance_micros`, `STAGE_MICROS`. |
| D2 | How aggressive on BYOK retirement? | **Every tier** | User-facing prose + identifiers + API value + schema enum + arch-doc rename. No alias — clean break (pre-v2.0 permits it). |
| D3 | "Design partners run free" copy | **Reframe** (option B) | Design-partner program stays open; nobody gets charges waived (everyone gets the same $5 starter). |

Final rate table you signed off on:

| Constant (proposed) | Value | Display |
|---|---|---|
| `STARTER_CREDIT_MICROS` | `5_000_000` | $5 |
| `EVENT_MICROS` (single, both postures) | `0` | free |
| `STAGE_MICROS` | `1_000` | $0.001 |

Worked-example math: 100 events × 3 stages × $0.001 = **$0.30**. Starter credit covers ~5,000 stages = ~1,666 events at 3-stage shape. 1,000 events/day × 3 stages = $3/day = ~$90/month at sustained mid-volume — well below peer benchmarks (Lindy.ai $0.005/task; LangSmith $0.001/step), gives design partners genuine runway, and headlines as "introductory rate, will rise post-GA" so future ratchets are expected.

---

## What's queued (M66 specs to author)

These come straight from the proposal doc. None authored yet — that's the next action when you say go.

| Spec | Title | Workstreams |
|---|---|---|
| `M66_001_P1` | `API_BILLING_TRACTION_RATES_MICRO_UNIT` | Schema migration `balance_cents` × 1M → `balance_micros`. Zig + TS rate constants. Paired pin tests. Snippet update. Changelog entry announcing rate cut + introductory-rate framing. |
| `M66_002_P1` | `API_CLI_UI_DOCS_BYOK_FULL_RETIREMENT` | Schema enum value `'byok'` → `'self_managed'`. `Mode` enum + log scopes rename. `mode: "byok"` API value rename (clean break). `ByokFields.tsx` → `ProviderKeyFields.tsx`. CLI flag rename. Website / docs / dashboard copy sweep. Architecture doc rename: `billing_and_byok.md` → `billing_and_provider_keys.md`, `scenarios/02_byok.md` → `scenarios/02_self_managed.md`. README.md tagline fix. |
| `M66_003_P2` | `API_CLI_UI_DOCS_SUPPORT_EMAIL_CANONICAL` | Six `SUPPORT_EMAIL` constants (one per repo) all asserting `usezombie@agentmail.to`. Paired pin tests. Sweep all `hello@usezombie.com` literals. |
| `M66_004_P2` | `DOCS_SPEC_TEMPLATE_DETERMINISM_UPGRADE` | TEMPLATE.md edits + `audit-spec-template.sh` extensions + `AGENTS_INVARIANCE.md` questions + DOC READ GATE row + AGENTS.md cross-link, all in one commit per the rule-extension protocol. |

Sequencing: 1 first (cleanest, smallest blast radius), then 2 + 3 in parallel feature branches (3 could fold into 2 if you want one PR), 4 whenever you want better executor discipline going forward.

---

## TEMPLATE.md determinism upgrades — proposed

Seven concrete additions to `docs/TEMPLATE.md`, ordered by leverage:

1. **§0 — "Decisions resolved (input to execute)"** — captures D-style proposal-phase answers as load-bearing context. Agent treats them as contract.
2. **"Authoritative pin sources" sub-table** under Files Changed — lists files where shared values live + their paired tests. Drift between pin sources is a blocking violation.
3. **Bidirectional Failure-Mode ↔ Test mapping** — every failure mode row cites a `test_<name>`; every test cites the goal/failure-mode it proves. Auditable.
4. **"Cross-repo coordination" section** (conditional) — for specs spanning multiple repos: which branches in which repos, in which order, paired commits.
5. **"Executor anti-patterns"** (separate from author anti-patterns) — pulls recurring CLAUDE.md / AGENTS.md rules into the spec body so they're load-bearing during execute (no `make` targets, no NLG framing, no MS-IDs in code, etc.).
6. **Tighter "Implementing agent — read these first"** — each entry must point at concrete files OR a `docs/*.md` doc with §N, AND state WHAT TO LEARN (not just WHY).
7. **Branch + worktree as fixed frontmatter line** — `feat/m{N}-{slug}` off `main`, worktree at `~/Projects/usezombie-m{N}-{slug}`, base always `main`.

These themselves trigger the rule-extension protocol (4 same-diff steps); spec'd as **M66_004**.

---

## Two open questions for you

1. **Should I author the four M66 specs now?** I have the full plan in the proposal doc + this handoff. Ready to invoke `kishore-spec-new` × 4, place each in `docs/v2/pending/`, and set them up for sequenced execution. Say "go" and they're written this session or the next.
2. **PR ordering.** Both PRs (`docs#47`, `usezombie#311`) are docs/copy-only and independent of M66's substance. Two paths:
   - **Land both now**, then ship M66_001 (rates) on top, since rate constants flip via the snippet automatically. Cleanest history.
   - **Hold both until M66_001 is drafted**, then land all three together. Tighter narrative, slower.
   My vote: land both now. M65 is M65; M66 is M66. The M65 PRs ship a coherent unit (BYOK retired from prose, stealth-mode banner up, contact email standardized on the surfaces I touched).

---

## Repo state at handoff

```
~/Projects/usezombie/        main, ahead of origin/main by 5 commits (pre-existing)
                              M docs/v2/pending/M50_001_*.md  (pre-existing, not mine)
                              ?? docs/v2/HANDOFF_M65_DOCS_REPHRASE.md  (prior handoff, untracked)
                              ?? docs/v2/HANDOFF_M65_M66_SESSION.md    (this file, untracked)
                              ?? docs/v2/PROPOSAL_M66_PRICING_BYOK_EMAIL.md  (untracked)
~/Projects/docs/             chore/m65-byok-marketing-rephrase, in sync, clean
~/Projects/dotfiles/         master, in sync, AGENTS.md changelog-voice section live
~/Projects/.github/profile/  not touched this session
```

Open PRs:
- usezombie/docs#47 — chore/m65-byok-marketing-rephrase (mergeable, all checks green)
- usezombie/usezombie#311 — feat/m65-stealth-mode-banner (just opened, CI pending)

Branches the future you should know about:
- `feat/m65-stealth-mode-banner` exists locally + remote on usezombie repo. Once #311 merges, prune.
- No active spec in `docs/v2/active/` for M66 yet. Worktree creation comes after CHORE(open).

---

## How to resume

```bash
# 1. Quick sanity
cd ~/Projects/usezombie && git status && git log --oneline -3
gh pr list --repo usezombie/usezombie | head
gh pr list --repo usezombie/docs | head

# 2. Land the open M65 PRs
gh pr merge 47 --repo usezombie/docs --squash      # docs site copy + brevity pass
gh pr merge 311 --repo usezombie/usezombie --squash  # website stealth-mode banner

# 3. Author M66 specs (next agent / future you)
#    Say "/kishore-spec-new" or just "go"; the proposal doc has the
#    complete brief for each of the four specs, including:
#      - Files Changed tables
#      - Authoritative pin sources
#      - Decisions resolved (D1=micros, D2=aggressive, D3=reframe)
#      - Cross-repo coordination
#      - Test specifications

# 4. Read the proposal in full before authoring specs
cat ~/Projects/usezombie/docs/v2/PROPOSAL_M66_PRICING_BYOK_EMAIL.md
```

---

🤖 Authored by Claude Opus 4.7 (1M context). Hand off whenever.
