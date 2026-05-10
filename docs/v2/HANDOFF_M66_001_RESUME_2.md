# Handoff — resume M66_001 implementation (round 2)

**Date:** 2026-05-11
**Captain:** Kishore
**Author:** Claude Opus 4.7 (1M context)
**Status:** §1 + §2 + §3 (Zig+API) + §3-tail done and pushed; §4 + §5 + §6 + paired docs PR + CHORE(close) remain.

This is the second handoff for M66_001. The first round committed §1+§2+§3-Zig in `cbb23fac` and the BYOK→self_managed retirement across UI / CLI / arch docs / manifests / tests in this commit. What's left is the website pricing-surface rewrite (§4), the SUPPORT_EMAIL constants (§5), the docs currency audit (§6), the paired Mintlify docs PR, and CHORE(close) wrapping.

---

## Where things are

**Branch:** `feat/m66-001-byok-retirement` on `usezombie/usezombie`
**Worktree:** `~/Projects/usezombie-m66-001-byok-retirement/`
**Origin tip:** `d5d5b6ad` (pushed; pre-push integration suite was green: 1508/0 locally, 1503/1508 on the hook run was a state-pollution fluke that resolved on re-attempt — see Gotcha 13).

**Spec location** (renamed in §3 tail): `docs/v2/active/M66_001_P1_API_CLI_DOCS_UI_SELF_MANAGED_RETIREMENT_AND_TRACTION_RATES.md`. Status `IN_PROGRESS`, branch field set.

**Commits on the branch:**

| SHA | Subject | What it did |
|---|---|---|
| `3db21927` | chore(m66-001): open — Status IN_PROGRESS, spec → active/ | CHORE(open) move only |
| `e9f4621a` | docs(m66-001): log §1 scope expansion in Discovery — 3 schemas, not 1 | Spec extension for `014_zombie_execution_telemetry` + `019_model_caps` |
| `cbb23fac` | feat(m66-001): nanos billing unit + traction rates + BYOK→self_managed (zig+api) | Schemas, Zig constants, Mode rename, function renames, HTTP rejects byok, error registry, openapi.json enum, schema/020 comment |
| `1cd35544` | fix(m66-001): integration test failures from §1+§2+§3 batch | signup_bootstrap pin, balanceCoversEstimate drain, model_caps i32→i64 widen |
| `d5d5b6ad` | feat(m66-001): §3 tail — BYOK retirement across UI, CLI, arch docs, manifests, tests | This handoff's parent commit. 98 files, 651/+1600 |

**Verification at handoff:**
- App vitest **356/356** · website vitest **129/129** · zombiectl bun test **566/566**
- `make test` 29/29 · `make lint` green · `make test-integration` 1508/0
- `bash scripts/audit-ufs.sh --diff`: 0 violations across 37 files

---

## Section progress

| Section | State | What "done" means |
|---|---|---|
| **§1 Nanos unit** | ✅ Done | Schemas 014, 017, 019 in nanos shape; Zig `_CENTS` → `_NANOS`; `model_caps` rate columns BIGINT |
| **§2 M66 traction rates** | ✅ Done | `STARTER_CREDIT_NANOS`, `EVENT_NANOS`, `STAGE_PLATFORM_NANOS`, `STAGE_SELF_MANAGED_NANOS`; `computeStageCharge` posture-dispatched |
| **§3 Zig + API** | ✅ Done | `Mode.byok` → `.self_managed`; HTTP rejects via generic mode-not-recognized fallthrough (no special case, no `UZ-PROVIDER-005`); openapi.json + paths/*.yaml flipped |
| **§3 UI/CLI/arch-doc tail** | ✅ Done | 98-file commit; `PROVIDER_MODE`+`CHARGE_TYPE`+`NANOS_PER_USD` named-const discipline applied across Zig/TS/JS; 8 file/directory renames; transient handoff+proposal docs deleted |
| **§4 Website pricing** | ⬜ Not started | See **Next steps** §4 below |
| **§5 SUPPORT_EMAIL** | ⬜ Not started | See **Next steps** §5 below |
| **§6 Docs currency audit** | ⬜ Not started | See **Next steps** §6 below |
| **Paired docs PR** | ⬜ Not started | See **Next steps** docs below |

---

## Next steps (in implementation order)

### 1. §4 website pricing surface (separate commit)

**Note:** §3 tail already swept BYOK terminology from the website. Field names on `RATES_CENTS` / `RATES_DISPLAY` were renamed (`eventByok` → `eventSelfManaged`); display strings retired BYOK; FAQ key text uses "self-managed" now. **Values are still in cents-shape and reflect the OLD pricing.** §4 is the value/shape rewrite, not a terminology pass.

Files:

- `ui/packages/website/src/lib/rates.ts` — replace with nanos-shape exports per spec **Naming convention (cross-tier)**:
  ```ts
  export const STARTER_CREDIT_NANOS = 5_000_000_000n;
  export const EVENT_NANOS = 0n;
  export const STAGE_PLATFORM_NANOS = 1_000_000n;
  export const STAGE_SELF_MANAGED_NANOS = 100_000n;
  export const RATES_DISPLAY = {
    STARTER_CREDIT: "$5",
    EVENT_RATE: "free",
    STAGE_PLATFORM: "$0.001",
    STAGE_SELF_MANAGED: "$0.0001",
  } as const;
  ```
  *Open question:* `bigint` or `number`? `5_000_000_000` fits in JS Number safely (≤ 2^53). The app side uses `number`. Recommend matching: `number` everywhere, with `NANOS_PER_USD` already named in `app/lib/types.ts`. Import `NANOS_PER_USD` from a shared file or duplicate at the website level — the website doesn't depend on `@usezombie/app`. Cleanest: `ui/packages/website/src/lib/rates.ts` defines its own `NANOS_PER_USD` (cross-runtime parity rule says identical name; doesn't say "single export site").
- `ui/packages/website/src/lib/rates.test.ts` — paired pin tests asserting role names + values + 10× gradient invariant (STAGE_PLATFORM == 10 × STAGE_SELF_MANAGED).
- `ui/packages/website/src/components/Pricing.tsx` — full rewrite of rate display:
  - Drop `WORKED_EXAMPLE` references (constant gone; spec wants traction-rate framing instead)
  - Drop `RATES_DISPLAY.eventPlatform` / `.eventSelfManaged` references (collapsed to one EVENT_RATE)
  - Two stage rates side-by-side: `RATES_DISPLAY.STAGE_PLATFORM` ($0.001) + `RATES_DISPLAY.STAGE_SELF_MANAGED` ($0.0001) with the 10× gradient framing
  - Subscript "stealth-mode testing rate — will rise post-GA"
  - Drop the BYOK/self-managed provider-list paragraph
- `ui/packages/website/src/components/Pricing.test.tsx` — assertions for new copy + new rates + new email
- `ui/packages/website/src/components/FAQ.tsx` + test — three answers reference rates; rephrase math examples to use the new numbers
- `ui/packages/website/src/pages/Terms.tsx` — rate references update
- `ui/packages/website/src/components/Footer.tsx` — drop the legacy rate badge (FAQ already terminology-clean)
- `ui/packages/website/src/components/FeatureFlow.tsx`, `Home.tsx`, `Privacy.tsx` — already terminology-clean from §3 tail; pass through for any rate-number prose

**Captain's standing decision:** "no hardcoded numerics in tests — assert against named constants." Pin tests where the literal IS the contract carry an inline `// pin test: literal is the contract` comment per RULE UFS (now a full gate — see Gotcha 14).

### 2. §5 SUPPORT_EMAIL per repo (separate commit)

Five new constant files asserting `usezombie@agentmail.to`:
- `src/config/contact.zig` + `src/config/contact_test.zig` (Zig)
- `ui/packages/website/src/lib/contact.ts` + `contact.test.ts`
- `ui/packages/app/lib/contact.ts` + paired test
- `zombiectl/src/lib/contact.js` + paired test
- `~/Projects/docs/snippets/contact.mdx` (lands in the paired docs PR, not the lead PR)

Then sweep every `hello@usezombie.com` / `support@usezombie.com` literal across `src/`, `ui/`, `zombiectl/`, `docs/`, `public/` and replace with the imported constant. Keep the `~/Projects/.github/profile/README.md` literal as-is per Captain's "skip .github/profile" decision.

**RULE UFS applies:** define `SUPPORT_EMAIL` once per runtime, identical name across Zig/TS/JS (the cross-runtime parity rule is **general** — all constants share names, not just `NANOS_PER_USD`).

### 3. §6 Documentation currency audit (separate commit)

Walk every spec under `docs/v2/done/M*.md` (~92 files) and grep-confirm against `~/Projects/docs/`, `docs/architecture/`, repo READMEs. Per Captain's earlier directive: **fix all drift inline in this PR** (not as follow-up specs). Capture findings in the spec's Discovery section.

§3 tail already fixed broken cross-references to renamed architecture files in 7 done specs (`billing_and_byok.md` → `billing_and_provider_keys.md`, `02_byok.md` → `02_self_managed.md`, `byok-handoff.md` → `self-managed-handoff.md`). The remaining audit is: rate references, schema column references, removed endpoints, model-cap shape, and the M48 BYOK historical mentions (which stay — "historical entries are archives").

### 4. Paired docs PR on `~/Projects/docs/`

Branch: `feat/m66-001-byok-retirement-docs`

- `~/Projects/docs/snippets/rates.mdx` — flip values: `STARTER_CREDIT = "$5"` (unchanged), `EVENT_RATE = "free"` (was `$0.01`), `STAGE_PLATFORM = "$0.001"`, `STAGE_SELF_MANAGED = "$0.0001"` (new key)
- `~/Projects/docs/snippets/contact.mdx` — new file with `SUPPORT_EMAIL = "usezombie@agentmail.to"` export
- BYOK prose sweep across: `index.mdx`, `concepts.mdx`, `quickstart.mdx`, `zombies/credentials.mdx`, `zombies/overview.mdx`, `zombies/install.mdx`, others as found
- `~/Projects/docs/changelog.mdx` — new `<Update>` block announcing the M66 rate cut + term retirement (template + version-bump matrix in `~/Projects/dotfiles/skills/release-template.md` — re-source each release, never paraphrase)

**Coordination:** opens AFTER the lead PR's content is locked (no further rate changes mid-review).

### 5. CHORE(close)

Per AGENTS.md skill chain (mandatory order):

1. `/write-unit-test` — coverage audit against the spec's Test Specification table
2. `/review` — adversarial diff review against `docs/architecture/billing_and_provider_keys.md`, `docs/REST_API_DESIGN_GUIDELINES.md`, `docs/ZIG_RULES.md`, Failure Modes, Invariants
3. `gh pr create` — open the lead PR
4. `/review-pr` — comments on the open PR
5. `kishore-babysit-prs` — Greptile poll loop

Mark all Dimensions/Sections `DONE` in the spec body, move `docs/v2/active/M66_001_*.md` → `docs/v2/done/`, write the changelog `<Update>`, fill in PR Session Notes (decisions, assumptions, dead ends, deferrals, audit-ufs results).

---

## Critical gotchas learned this session

(In addition to the original 12 in the deleted `HANDOFF_M66_001_RESUME.md` — schema scope, model_caps widen, `mode` is TEXT, no ALTER pre-v2.0, BSD `sed`, `bun install`, multi-worktree containers, `zombied-api` OIDC env, local-vs-origin main divergence, GitHub SSH drops, no hardcoded numerics, cross-tier role names — those still apply.)

13. **Pre-push integration sometimes flakes on state pollution.** Saw `1503/1508 tests passed; 5 failed` on the first push attempt; immediate `make test-integration` standalone passed 1508/0. Recovery: re-run the push. Multi-worktree shared Postgres + Redis is the suspect.

14. **RULE UFS is now a full gate.** Promoted from a single bullet in BUN_RULES.md §2 to `docs/gates/ufs.md` with `scripts/audit-ufs.sh` (symlinked into the worktree). Three discipline points enforced:
    - Repeat string literals (≥2 sites in a file) → named const
    - Semantic numeric literals (powers-of-ten, unit factors, sub-cent rates) → named const
    - Cross-runtime parity — every constant shares its identifier verbatim across Zig+TS+JS, not a curated subset
    - Pin-test carve-out: `// pin test: literal is the contract` comment
    Run `bash scripts/audit-ufs.sh --diff` at HARNESS VERIFY.

15. **No special-case retired-mode rejection branch.** §3 tail removed the `if (input.mode == "byok")` rejection branch + `UZ-PROVIDER-005` error code. Captain's read: implies migration support that doesn't belong pre-v2.0. The generic `"mode must be 'platform' or 'self_managed'"` fall-through handles every unsupported value uniformly. Don't reintroduce a special-case branch for any other retired wire value.

16. **Numeric literal sweep extends to test fixtures.** `posture: "platform"` in mock returns, `balance_nanos: 1_000_000_000` in fixture objects, `mode: "self_managed"` in mockResolvedValue all need the named constant. TypeScript narrowing isn't a substitute. (Pin tests where the literal IS the contract are the explicit carve-out.)

17. **Filename renames retire the term, but historical references in `done/` specs DO NOT.** Sealed-history milestone specs (`docs/v2/done/M48_001_*`, `docs/v2/done/M49_001_*`, etc.) keep their BYOK term-mentions because they describe what was true at their time. Only fix BROKEN CROSS-REFERENCES (links pointing to renamed files) — leave term-mentions alone. "Historical entries are archives."

18. **Branch name `feat/m66-001-byok-retirement` is immutable.** Cannot rename mid-flight; left as-is. Cosmetic; doesn't affect anything.

---

## Resume commands

```bash
# 1. Bring local main in sync
cd ~/Projects/usezombie && git checkout main && git pull --ff-only origin main

# 2. Enter the worktree
cd ~/Projects/usezombie-m66-001-byok-retirement
git pull --ff-only origin feat/m66-001-byok-retirement
git status   # should be clean except this handoff doc

# 3. Confirm tooling
ls node_modules >/dev/null 2>&1 || bun install
docker ps --filter 'name=zombie' --format '{{.Names}} {{.Status}}'

# 4. If schemas drifted:
make down && make up

# 5. Sanity check
make test       # 29/29 expected
bash scripts/audit-ufs.sh --diff    # 0 violations expected

# 6. Begin §4 — website pricing surface rewrite
$EDITOR ui/packages/website/src/lib/rates.ts
```

---

## Open questions / decisions parked

None outstanding. Captain's standing decisions still apply:

- **Cross-repo scope:** lead PR + paired docs PR only; skip `.github/profile`
- **§6 drift policy:** fix all drift inline in this PR (not follow-up specs)
- **Migration:** forward-only, pre-v2.0 RULE NLG clean break (no ALTER, no rescaling, no compat shims, no special-case rejections of retired values)
- **Naming:** cross-tier role names identical across Zig/TS/JS for ALL constants (general — not a curated subset)
- **`make migrate` does not exist** — schema reseed is `make down && make up`
- **No new `audit-*.sh` make-lint wiring.** Sibling audit scripts (`audit-logging`, `audit-error-codes`, `audit-deinit-pairs`, `audit-spec-template`, `audit-ufs`) are invoked at HARNESS VERIFY ceremony, not from `make lint`. Convention; don't deviate.

---

🤖 Authored by Claude Opus 4.7 (1M context). Hand off whenever.
