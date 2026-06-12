<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M92_002: Rebrand identity surfaces from usezombie to agentsfleet (repo, architecture docs, branding, website identity, docs-repo assets)

**Prototype:** v2.0.0
**Milestone:** M92
**Workstream:** 002
**Date:** Jun 12, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — customer-facing identity: the product is now `agentsfleet` on `agentsfleet.net`; every README visitor, docs reader, and website tab still sees the retired brand
**Categories:** DOCS, UI
**Batch:** B1 — lands before M92_001 (the positioning copy is authored under the new brand)
**Branch:** feat/m92-002-agentsfleet-rebrand (merged as #405) · feat/m92-002-wordmark-refresh (continuation)
**Test Baseline:** unit=1947 integration=182
**Depends on:** —
**Provenance:** agent-generated (rebranding session with Indy, Jun 12, 2026) — grounded in `branding/` (agentsfleet wordmarks + favicon already authored Jun 12), a zero-reference scan of `assets/`, and reads of README/LICENSE/website config; re-confirm at PLAN.

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` (brand-mark rules: the pulse disc, Operational Restraint) + `branding/README.md` (asset usage map — itself rewritten by this spec).

---

## Implementing agent — read these first

1. `branding/README.md` — the asset-to-surface usage map (GitHub avatar, README hero, Mintlify docs, social cards); this spec renames the assets it documents.
2. `ui/packages/website/src/config.ts` — the single-source URL/install constants; the rename principle below decides each constant's fate.
3. `ui/packages/website/src/marketing-spec.test.ts` + `vocab-guard.test.ts` — must stay green; this workstream touches identity surfaces, NOT marketing copy (that is M92_001).
4. `~/Projects/docs/docs.json` — the Mintlify config naming `/favicon.svg` + `/logo/{light,dark}.svg`; the docs-repo asset swap targets exactly those paths.
5. `dispatch/write_ts_adhere_bun.md` — fires on the `config.ts`/`index.html`-adjacent TS edits.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `chore(m92): rebrand identity surfaces usezombie -> agentsfleet`
- **Intent (one sentence):** the repo, architecture docs, brand assets, website identity surfaces, and the docs site's logos/favicon all present `agentsfleet` (domains `agentsfleet.net`, `app.agentsfleet.net`, `docs.agentsfleet.net`), while every operational identifier that still resolves under the old name keeps working untouched.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm against the live world: (a) §1 cutover verification — which agentsfleet.net hosts resolve TODAY; any that don't → STOP and surface per §1, (b) the `assets/` zero-reference scan still holds, (c) `~/Projects/docs` HEAD state for the cross-repo branch flow. A `[?]` blocks EXECUTE.

**The rename principle (load-bearing):** *strings that brand get rebranded; strings that resolve keep resolving until their cutover is verified.* Rebranded now: product noun, wordmark/logo assets, copyright, page titles/meta, display links. Untouched in this workstream: `usezombie.sh` install domain (Indy-confirmed verbatim), `github.com/usezombie/**` org/repo URLs (org rename is the follow-up spec), `zombiectl`/`zombied` binary names, `@usezombie/*` package names, `core.zombie_*` schema names, `team@usezombie.com` (mail routing cutover not yet enumerated). Domain links (`agentsfleet.net`, `app.agentsfleet.net`, `docs.agentsfleet.net`) flip only after §1 verification passes.

---

## Product Clarity

1. **Successful user moment** — a TechStars reviewer opens the GitHub repo from the application, sees the agentsfleet wordmark and `agentsfleet.net` links, clicks through to a docs site whose favicon and logo match — one brand, no seams.
2. **Preserved user behaviour** — `curl -fsSL https://usezombie.sh | bash` still installs; `zombiectl` still talks to the API; every `github.com/usezombie` link still lands; docs content is unchanged (assets only).
3. **Optimal-way check** — string/asset rename across enumerated surfaces is the whole job; no abstraction wanted. The unconstrained-optimal (flip everything including org, packages, binaries at once) is rejected: each of those has an external dependency (GitHub org rename, npm scope, installer infra) that deserves its own enumerated cutover.
4. **Rebuild-vs-iterate** — iterate; this is a rename pass with verification gates, not a redesign. The agentsfleet marks reuse the pulse geometry (DESIGN_SYSTEM unchanged).
5. **What we build** — cutover verification, README/LICENSE rebrand, architecture-docs brand pass, branding/ consolidation, `assets/` removal, website identity surfaces (config domains, title/meta, favicon), docs-repo logo/favicon swap.
6. **What we do NOT build** — GitHub org rename, npm/package renames, binary renames, schema renames, mail cutover, docs.json textual rebrand, marketing-copy rewrite (M92_001), any redirect infrastructure.
7. **Fit with existing features** — unblocks M92_001 (copy authored as agentsfleet); must not destabilize the install path or the rates display (neither is touched).
8. **Surface order** — DOCS + UI identity only; no CLI/API behaviour change.
9. **Dashboard restraint** — N/A; no new UI surface, only identity strings/assets.
10. **Confused-user next step** — a user holding an old `usezombie.com` link: §1 requires Indy's hosting cutover to keep the old domain serving (redirect or alias) so old links don't dead-end; if that cannot be arranged, the flip for that surface stays parked and is surfaced to Indy.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE NDC (no dead assets: superseded wordmark deleted, not parked), RULE NLR (touch-it-fix-it: stale brand prose in touched docs goes), RULE ORP (cross-layer orphan sweep on every renamed/deleted asset — README image refs, branding README, docs.json paths), RULE UFS (domain strings already live in `config.ts` constants; no new literals scattered), RULE TST-NAM.
- **`dispatch/write_ts_adhere_bun.md`** — `config.ts` edit discipline; no behaviour change hidden in the rename.
- **`docs/DESIGN_SYSTEM.md`** — brand-mark rules; the agentsfleet assets already conform (authored from the usezombie geometry).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` | — |
| File & Function Length | no — renames/deletes; no file grows | — |
| UFS (literals) | yes — domain strings | edit the existing `config.ts` constants in place; tests import them |
| UI Substitution / DESIGN TOKEN | no — no component markup changes | — |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — no runtime surface | — |
| CI/CD edit guard (`.github/workflows/**`) | yes — hostname strings in deploy/smoke workflows | Indy-granted in the Jun 12, 2026 session (scope: hostname/brand strings only, zero workflow-logic changes); each workflow edit listed in the PR body |

---

## Overview

**Goal (testable):** after merge, a repo-wide grep finds `usezombie` only in the allowlisted operational set (install domain, GitHub org URLs, package/binary/schema identifiers, team email), the README/architecture docs/website tab present agentsfleet, `assets/` is gone with zero dangling references, and the docs site serves the agentsfleet favicon and logos.

**Problem:** the product rebranded to agentsfleet (TechStars application, Jun 2026) but every identity surface — README hero, LICENSE copyright, nine architecture docs, browser tab, docs-site favicon — still says usezombie; the new wordmarks sit unused in `branding/`.

**Solution summary:** a verified-cutover rename pass: enumerate and verify the agentsfleet.net host set first, then flip identity strings and assets surface by surface, deleting what the rename supersedes, with grep-based eval gates proving both the flips and the deliberate non-flips.

---

## Prior-Art / Reference Implementations

- **Asset map** → `branding/README.md` — the existing usage table (avatar, README hero, Mintlify, social) is the checklist; the rename walks it row by row.
- **Brand-noun guard pattern** → `vocab-guard.test.ts` (allowlist + rendered-copy scan) — M92_001 extends it for agentsfleet; this workstream only keeps it green.
- **Cross-repo docs flow** → operating-model "Docs-repo edits on own branch": branch off `main` in `~/Projects/docs`, commit assets, PR separately.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `README.md` | EDIT | wordmark image, product noun, display links (`agentsfleet.net`, `docs.agentsfleet.net`), copyright line; repo-table URLs stay on the unchanged GitHub org |
| `LICENSE` | EDIT | copyright holder → agentsfleet |
| `docs/architecture/*.md` (9 brand-bearing files) | EDIT | product noun + display domains in prose; operational identifiers untouched |
| `branding/usezombie-mark.svg` → `branding/agentsfleet-mark.svg` | RENAME | mark carries no text; rename for coherence |
| `branding/usezombie-mark-glow.svg` / `.png` → `agentsfleet-mark-glow.*` | RENAME | README hero + social asset |
| `branding/usezombie-wordmark.svg` | DELETE | superseded by `agentsfleet-{dark,light}.svg` |
| `branding/README.md` | EDIT | rewrite the asset map under the new names |
| `assets/` (4 files) | DELETE | zero references repo-wide (verified at authoring; re-verify at EXECUTE) |
| `ui/packages/website/src/config.ts` | EDIT | `APP_BASE_URL` → `app.agentsfleet.net` (+ dev host), `DOCS_URL` → `docs.agentsfleet.net`; `INSTALL_COMMAND`, `GITHUB_URL`, `TEAM_EMAIL` untouched |
| `ui/packages/website/src/config.test.ts` | EDIT | pins the flipped constants AND the deliberately-unflipped ones |
| `ui/packages/website/index.html` | EDIT | `<title>`, meta description, favicon link → agentsfleet |
| `ui/packages/website/public/favicon.svg` | CREATE | copied from `branding/favicon.svg` |
| `.github/workflows/*` (hostname-bearing only) | EDIT | deploy/smoke hostname strings follow the §1-verified cutover; zero logic changes |
| `~/Projects/docs/favicon.svg`, `favicon.ico`, `logo/dark.svg`, `logo/light.svg`, `logo/mark-glow.svg` | EDIT (cross-repo, own branch) | replaced with the agentsfleet assets from `branding/` |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream for all identity surfaces — they form one user-visible seam; splitting them ships a half-branded product for the gap between PRs. The follow-up spec (org/package/binary/mail renames) is split because each piece has an external cutover dependency.
- **Alternatives considered:** folding into M92_001 — rejected: blows the 320-line spec bound and mixes mechanical renames with copywriting; the dependency edge (B1→B2) keeps them coherent instead.
- **Patch-vs-refactor verdict:** **patch** (rename + asset swap with verification gates). Follow-up named: `M9X` org/package/binary/mail rename spec.

---

## Sections (implementation slices)

### §1 — Cutover enumeration & verification (blocks every link flip)

Agent-first sequencing: enumerate every host this rename points traffic at, hand Indy the console checklist, verify before flipping. Human steps (Indy): agentsfleet.net registrar + DNS (`@`, `app`, `app.dev`, `docs`), hosting domain attach (website + app projects), Mintlify custom domain, Clerk allowed-origin for `app.agentsfleet.net`, and old-domain aliasing/redirects so existing `usezombie.com` links keep landing. Fail loud listing every unverified host before any link flip; unverified host → that surface's flip parks, the rest proceed.

- **Dimension 1.1** — DONE — checklist surfaced (Jun 12); apex `agentsfleet.net` answers 301→usezombie.com (display flips proceed); `app.`/`docs.`/`www.` unanswered → their flips parked per this section's rule → Eval `E1`

### §2 — Repo identity: README + LICENSE

README presents agentsfleet: wordmark/hero image, product noun, display links, copyright footer; the lede sentence updates to brand-neutral phrasing that survives M92_001's positioning rewrite (no deploy-era headline rework here — M92_001 owns the message). LICENSE copyright holder flips.

- **Dimension 2.1** — DONE → Eval `E2` (clean; `cd usezombie` clone-dir is operational)
- **Dimension 2.2** — DONE → Eval `E2`

### §3 — Architecture docs brand pass

The nine brand-bearing `docs/architecture/*.md` files swap the product noun and display domains in prose. Operational identifiers (schema names, binary names, API hostnames inside payload examples that must match the running system) stay byte-identical — RULE NLR applies to brand prose only.

- **Dimension 3.1** — DONE → Eval `E3` (clean; 36 brand tokens flipped across 9 files)

### §4 — branding/ consolidation

Marks rename to agentsfleet-*, the superseded text-bearing wordmark deletes, `branding/README.md` rewrites its usage map under the new names, and every reference (README hero, docs pointers) follows — RULE ORP sweep across both repos.

- **Dimension 4.1** — DONE → Eval `E4` (clean)

### §5 — assets/ removal

`assets/` duplicates `branding/` and has zero references. Re-verify, then delete the directory.

- **Dimension 5.1** — DONE → Eval `E4` (directory removed, zero refs)

### §6 — Website identity surfaces

`config.ts` flips the display/app/docs domains (post-§1); `index.html` gets the agentsfleet title/meta/favicon; the favicon ships in `public/`. `config.test.ts` pins both directions: flipped constants AND the four deliberately-unflipped ones (install command, GitHub URL, team email, dev API fallbacks as applicable) so the follow-up spec must consciously unpin them.

- **Dimension 6.1** — PARKED (IN_PROGRESS) — `app.`/`docs.agentsfleet.net` unanswered at EXECUTE; parked values pinned instead (see 6.2); flips + `test_config_agentsfleet_domains` land when §1 verifies, on this branch or the cutover follow-up
- **Dimension 6.2** — DONE — rebrand-pin suite added to `config.test.ts` (install command, GitHub URL, team email, parked docs host)
- **Dimension 6.3** — DONE — shipped in #405 as the `smoke.spec.ts` brand trio (title regex, favicon wired + resolves 200, meta description names agentsfleet); marker reconciled in the wordmark-refresh follow-up → Test `test_site_identity_meta` (e2e)

### §7 — Workflow hostname pass

Deploy/smoke workflows referencing flipped hostnames follow §1; brand strings in workflow names/comments update. Zero changes to triggers, jobs, steps, or permissions — the diff for each workflow must read as string-substitution only.

- **Dimension 7.1** — DONE — audit found every workflow `usezombie` string operational (Postgres user/db, Redis password, ghcr images, Vercel hostname, live health URLs): zero edits → Eval `E5` (empty diff)

### §8 — Docs-repo brand assets (cross-repo)

In `~/Projects/docs` on a fresh branch off `main` (`chore/m92-002-agentsfleet-brand-assets`): `favicon.svg`, `favicon.ico`, `logo/dark.svg`, `logo/light.svg`, `logo/mark-glow.svg` replaced with the agentsfleet equivalents from `branding/`. Asset swap only — `docs.json` textual rebrand rides the follow-up spec. Separate PR on that repo; linked from this spec's PR body.

- **Dimension 8.1** — DONE — usezombie/docs#91, checksums matched → Eval `E6`

---

## Interfaces

No code interface changes. The locked contract is `config.ts`: constant *names* and the unflipped *values* (`INSTALL_COMMAND`, `GITHUB_URL`, `TEAM_EMAIL`) must not change; flipped values change exactly once, pinned by `config.test.ts`.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Host unverified | DNS/hosting step not done | that surface's link flip parks; checklist re-surfaced to Indy with the failing `curl` output; everything else proceeds |
| Old links dead-end | no alias/redirect from usezombie.com hosts | §1 requires the alias before the flip; absent → flip parks, surfaced |
| Broken README hero | image renamed but a ref missed | RULE ORP sweep (E4) catches; render-check README on the PR |
| Docs repo dirty/diverged | `~/Projects/docs` HEAD ≠ main | stash or fresh worktree off `main` per the operating model; never commit on a stale branch |
| Stale favicon in browsers | aggressive favicon caching | accepted; note in PR body — no cache-busting infra for a favicon |

---

## Invariants

1. `INSTALL_COMMAND` is byte-identical to `curl -fsSL https://usezombie.sh | bash` — enforced by `test_config_operational_strings_unchanged`.
2. `GITHUB_URL` and `TEAM_EMAIL` unchanged — same test.
3. No rendered-copy regression: vocab-guard, marketing-spec, no-pr-validator-framing all green untouched — enforced by the existing suites in `make test-unit-website`.
4. Zero dangling references to renamed/deleted assets — enforced by Eval `E4` (grep set) wired into Acceptance.
5. Schema (`core.zombie_*`), binary (`zombiectl`, `zombied`), and package (`@usezombie/*`) identifiers appear in the diff zero times — enforced by Eval `E7` diff-scope check.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | e2e (manual-verified) | Eval `E1` | every flipped host answers HTTP 200/30x before its dependent edit lands |
| 2.1–2.2 | eval | Eval `E2` | README/LICENSE greps: agentsfleet present; non-allowlisted usezombie absent |
| 3.1 | eval | Eval `E3` | architecture docs grep clean against the allowlist |
| 4.1, 5.1 | eval | Eval `E4` | zero references to deleted/renamed paths |
| 6.1 | unit | `test_config_agentsfleet_domains` | `APP_BASE_URL`/`DOCS_URL` equal the agentsfleet.net hosts |
| 6.2 | unit | `test_config_operational_strings_unchanged` | install command, GitHub URL, team email byte-identical to pre-rename values |
| 6.3 | e2e | `test_site_identity_meta` — landed as the `smoke.spec.ts` brand trio (`home page loads`, `brand favicon is wired and resolves`, `brand meta description names agentsfleet`) | dry lane: document title contains agentsfleet; favicon link resolves |
| 7.1 | eval | Eval `E5` | workflow diffs are string-substitution only |
| 8.1 | eval | Eval `E6` | docs-repo files at docs.json paths are the agentsfleet assets (checksum match vs `branding/`) |

**Regression:** full website unit suite + dry lane green (no copy changed, so all three marketing guards must pass unmodified). **Idempotency/replay:** N/A — rename pass.

---

## Acceptance Criteria

- [ ] Cutover hosts verified or parked-with-surface — verify: Eval `E1` output in PR body
- [ ] Website unit + lint + dry lane green — verify: `make test-unit-website && make lint-website && make dry-smoke`
- [ ] Brand greps clean (flips done, non-flips intact) — verify: Evals `E2`–`E5`, `E7`
- [ ] Docs-repo branch pushed with asset swap, PR linked — verify: Eval `E6` + PR URL in Session Notes
- [ ] `gitleaks detect` clean

---

## Eval Commands (post-implementation)

```bash
# E1: Cutover verification (per flipped host)
for h in agentsfleet.net app.agentsfleet.net docs.agentsfleet.net; do curl -sI "https://$h" | head -1; done
# E2: README/LICENSE flips (expect agentsfleet hits; expect NO usezombie outside allowlist)
grep -nE "usezombie" README.md LICENSE | grep -vE "usezombie\.sh|github\.com/usezombie|@usezombie/|zombiectl|team@usezombie"
# E3: Architecture docs (archive excluded)
grep -rnE "usezombie" docs/architecture --include='*.md' --exclude-dir=archive | grep -vE "usezombie\.sh|github\.com/usezombie|@usezombie/|zombiectl|core\.zombie"
# E4: Orphan sweep for renamed/deleted assets (expect empty)
grep -rn "usezombie-mark\|usezombie-wordmark\|^assets/\|(assets/\|\"assets/" --include='*.{md,ts,tsx,html,json,yml,svg}' . | grep -v node_modules | head
# E5: Workflow diffs are strings-only (manual review of git diff .github/)
git diff origin/main -- .github/ | grep -E "^[-+]" | grep -vE "^[-+]{3}|usezombie|agentsfleet" | head
# E6: Docs-repo assets match branding/ (run in ~/Projects/docs)
shasum ~/Projects/usezombie/branding/agentsfleet-dark.svg logo/dark.svg
# E7: Operational identifiers byte-stable — count compare HEAD vs working tree (expect all OK)
for t in "usezombie-admin" "zombied" "core\.zombie_" "@usezombie/" "x-usezombie" "usezombie\.sh"; do a=$(git grep -c "$t" origin/main -- docs/architecture README.md | awk -F: '{s+=$NF}END{print s+0}'); b=$(grep -rc "$t" docs/architecture README.md | awk -F: '{s+=$NF}END{print s+0}'); echo "$t $([ "$a" = "$b" ] && echo OK || echo DRIFT)"; done
```

---

## Dead Code Sweep

| File to delete | Verify |
|----------------|--------|
| `branding/usezombie-wordmark.svg` | `test ! -f branding/usezombie-wordmark.svg` |
| `assets/` (directory, 4 files) | `test ! -d assets` |

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `usezombie-wordmark` | Eval `E4` | 0 matches |
| `assets/` path refs | Eval `E4` | 0 matches |

---

## Discovery (consult log)

- **Skill chain (Jun 12, 2026):** `/write-unit-test` clean, iteration 1 — diff ledger 7/7 resolved (5 tested, 2 won't-test with reason); one gap (meta-description assertion) found and closed in-run. `/review` (adversarial subagent, fresh context) — 4 FIXABLE findings, all fixed: untracked `public/favicon.svg` (would have shipped a 404 + red smoke), 2× "a agentsfleet" grammar casualties, stale `usezombie-mark.svg` reference in the kept glow-SVG comment (E4 include-set extended to `*.svg`). 2 INVESTIGATE dispositioned: M92_001 spec lines in the branch diff = lifecycle artifact, not implementation scope (rejected); Footer `BRAND_NAME = "usezombie"` vs agentsfleet tab title = deliberate B1/B2 gap, surfaced to Indy (rendered copy is M92_001's surface). Also reverted tooling drift in `ui/packages/app/next-env.d.ts` (dry-smoke dev server regenerated it; restored via edit, not destructive git).
- **EXECUTE findings (Jun 12, 2026):** §1 probe — apex 301→usezombie.com (registered, redirect currently points backwards), subdomains dead → 6.1 parked, checklist surfaced to Indy mid-turn. §7 — zero brand-display strings in workflows; all hits operational; no `.github/` edits shipped. The agentsfleet brand assets were untracked on disk (never committed) — tracked into this branch; `branding/github-avatar.png` + `github-social.png` remain untracked in the main checkout for the org-rename spec. E7 eval refined from a diff-line grep (false-positives on unchanged identifiers inside brand-edited lines) to a HEAD-vs-tree count compare — all six identifier families byte-stable.
- **Authoring-time decisions (Indy, Jun 12, 2026 session):** `usezombie.sh` stays verbatim ("usezombie.sh to usezombie.sh"); `app.usezombie.com` → `app.agentsfleet.net` and `docs.usezombie.com` → `docs.agentsfleet.net` (Indy, mid-session); GitHub org, npm scope, binaries, schema names, team email → follow-up spec; `.github/` edits granted, strings-only scope; `assets/` removal requested if unused (zero references confirmed at authoring).
- **Continuation — wordmark refresh (Indy, Jun 12, 2026 evening session):** both lockups drop their baked background rects (transparent — baked backgrounds stay reserved for self-contained renders like social cards/avatars) and tighten the canvas 720×160 → 600×160, same disc/type spec; `branding/README.md` asset map updated to token-named colours (`--bg`/`--text`/`--pulse`/`--pulse-dim`) and the new dims; docs-repo `logo/{dark,light}.svg` re-propagation rides the companion branch `chore/m92-002-wordmark-refresh` per Indy ("docs branding must use the newly updated branding/ logos") — Eval `E6` re-run after the swap; output and cross-links land in both PR bodies. Dimension 6.3 marker reconciled: its assertions shipped in #405 (`smoke.spec.ts` title/favicon/meta trio) but the dimension line was never flipped to DONE.
- **§1 evening re-probe (Jun 12, 2026):** apex still 301→usezombie.com; `www`/`app`/`app.dev`/`docs` still publish no DNS records → 6.1 stays parked; console checklist re-surfaced to Indy with curl evidence. Old-host baseline: `usezombie.com` 307→`www` (200), `docs.usezombie.com` 200, `app.usezombie.com` resolves nowhere — the app host cutover is greenfield setup, not a migration. Merge-deploy triage (run 27413674218), all non-rebrand: `deploy-worker-dev` red on missing `nftables` at the dev worker host; `cli-acceptance-dev` red on one fixture-bootstrap case (`hydrateWorkspacesForToken: tenant has no workspaces`); the earlier post-deploy smoke failure sits on pre-rebrand main (#403).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification | Clean; outcome in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec + the rename principle | Clean or dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR (both repos' PRs) | Addressed before human review |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Cutover hosts | Eval `E1` | | |
| Website suite | `make test-unit-website && make lint-website && make dry-smoke` | | |
| Brand greps | Evals `E2`–`E5`, `E7` | | |
| Docs-repo assets | Eval `E6` | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- GitHub org rename (`github.com/usezombie` → agentsfleet) + repo URLs, npm scope `@usezombie/*`, binary names (`zombiectl`, `zombied`), Postgres schema (`core.zombie_*`), `team@usezombie.com` mail cutover, `api.usezombie.com` — the follow-up rename spec, each behind its own external cutover.
- `usezombie.sh` installer domain — stays, per Indy.
- `~/Projects/docs` textual content rebrand (`docs.json` name, page prose) — follow-up spec; this workstream swaps assets only.
- Marketing copy/positioning — M92_001 (B2).
- Redirect infrastructure beyond what §1 asks of the hosting console.
