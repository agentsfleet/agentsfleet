# Handoff — resume M66_001 implementation

**Date:** May 10, 2026
**Captain:** Kishore
**Author:** Claude Opus 4.7 (1M context)
**Status:** §1 + §2 + §3-Zig+API done and pushed; §3-tail + §4 + §5 + §6 + paired docs PR remain.

This is the second handoff for M66_001. The first (`HANDOFF_M66_001_EXECUTE.md`) covered the pre-CHORE(open) gate and the six-section sequencing strategy — that file is still accurate for everything except the section-progress table.

---

## Where things are

**Branch:** `feat/m66-001-byok-retirement` on `usezombie/usezombie`
**Worktree:** `~/Projects/usezombie-m66-001-byok-retirement/`
**Origin tip:** `1cd35544` (pushed; CI on this commit is whatever GitHub Actions ran for the pre-push hook locally — the branch isn't on a PR yet, so no GH-side CI runs)

**Spec status:** `Status: IN_PROGRESS` in `docs/v2/active/M66_001_P1_API_CLI_DOCS_UI_BYOK_RETIREMENT_AND_TRACTION_RATES.md`. Files-Changed table was extended in `e9f4621a` to log the §1 scope expansion (3 schemas, not 1).

**Commits on the branch:**

| SHA | Subject | What it did |
|---|---|---|
| `3db21927` | chore(m66-001): open — Status IN_PROGRESS, spec → active/ | CHORE(open) move only |
| `e9f4621a` | docs(m66-001): log §1 scope expansion in Discovery — 3 schemas, not 1 | Spec body extension after grep surfaced `014_zombie_execution_telemetry` + `019_model_caps` as cents-typed |
| `cbb23fac` | feat(m66-001): nanos billing unit + traction rates + BYOK→self_managed (zig+api) | 42 files. Schemas, Zig constants, Mode rename, function renames, HTTP wire format clean break, error registry, openapi.json enum, schema/020 comment |
| `1cd35544` | fix(m66-001): integration test failures from §1+§2+§3 batch | signup_bootstrap_test pin, balanceCoversEstimate drain amount, model_caps i32→i64 widen + test assertions |

**Verification at session end (local):**

- `make test` — 29 pass · 0 fail · 0 skipped · 4.86s
- `make test-integration` — 1508 tests, 0 failed, against real Postgres + Redis from clean reseed
- Pre-push hook on the actual push: 1287 passed · 0 failed · 221 skipped · memleak ✅
- `make lint` — green (gitleaks clean, openapi 39 paths REST §1 compliant)

---

## Section progress

| Section | State | What "done" means |
|---|---|---|
| **§1 Nanos unit** | ✅ Done | 3 schema files in place (014, 017, 019); Zig constants `_CENTS` → `_NANOS`; `model_caps` columns INTEGER → BIGINT; pin tests rebuilt |
| **§2 M66 traction rates** | ✅ Done | `STARTER_CREDIT_NANOS`, `EVENT_NANOS`, `STAGE_PLATFORM_NANOS`, `STAGE_SELF_MANAGED_NANOS` pinned; `computeStageCharge` posture-dispatched |
| **§3 Zig + API portion** | ✅ Done | `Mode.byok` → `Mode.self_managed` everywhere; `*Byok` functions renamed; HTTP rejects `mode: "byok"` with HTTP 400 + `UZ-PROVIDER-005`; log scopes renamed; openapi.json + schema/020 comment updated |
| **§3 UI/CLI/arch-doc tail** | ⬜ Not started | See **Next steps** §3-tail below |
| **§4 Website pricing** | ⬜ Not started | See **Next steps** §4 below |
| **§5 SUPPORT_EMAIL** | ⬜ Not started | See **Next steps** §5 below |
| **§6 Docs currency audit** | ⬜ Not started | See **Next steps** §6 below |
| **Paired docs PR** | ⬜ Not started | See **Next steps** docs below |

---

## Next steps (in implementation order)

The handoff's six-section order put §1 nanos first, then §2, §5, §3, §4, §6. Since §3-Zig + API already shipped, the remaining order is:

### 1. §3 UI/CLI/arch-doc tail (single coherent commit)

The lib/types.ts `Mode` rename cascades through ~12 TS files; the website doesn't use `Mode` directly but the file renames touch UI; CLI flag rename is independent. **The trap I hit and reverted:** changing `lib/types.ts` and `lib/rates.ts` independently produces a half-converted state that won't typecheck. Land all of these together:

**ui/packages/app:**
- `lib/types.ts` — `ProviderMode = "platform" | "byok"` → `"platform" | "self_managed"`; `PROVIDER_MODE.byok` → `.self_managed`; `balance_cents` → `balance_nanos`; `credit_deducted_cents` → `credit_deducted_nanos`. (Just `git restore`-ing this file, then re-applying carefully.)
- `lib/api/tenant_provider.ts` — body sends `mode: "self_managed"`
- `lib/api/tenant_provider.test.ts` — assertions
- `app/(dashboard)/settings/provider/components/ByokFields.tsx` → `git mv ProviderKeyFields.tsx`; component name + JSX
- `app/(dashboard)/settings/provider/components/ProviderSelector.tsx` — "Switch to BYOK" → "Use my own provider key"; tab "BYOK" → "Self-managed"; test IDs `provider-byok-*` → `provider-self-managed-*`
- `app/(dashboard)/settings/provider/components/ModeRadio.tsx` — Mode.byok references
- `app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` — `e.posture === "byok"` ternary; badge variant + label
- `app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx` — balance_cents → balance_nanos display + display formatting (now divide by 1e9 for $, not 100)
- `app/(dashboard)/settings/billing/lib/groupCharges.ts` — credit_deducted_cents → _nanos
- `app/(dashboard)/page.tsx` — sweep
- `tests/{provider-selector,billing-usage-tab,dashboard-coverage,billing-card,billing-grouping,coverage-edges,zombies}.test.ts` — `"byok"` literals + balance_cents/credit_deducted_cents references + Mode.byok mocks

**zombiectl:**
- `src/commands/tenant_provider.js` — `--byok` flag → `--self-managed`; legacy `--byok` rejected with stderr message + exit 2; send `mode: "self_managed"`
- `src/program/io.js` — sweep BYOK references
- `test/tenant_provider.unit.test.js` — flag + body assertions
- `test/golden/help-no-color.txt` — regenerate (`bun test --update-snapshots` or equivalent)
- `README.md` — sweep BYOK references

**Architecture docs:**
- `git mv docs/architecture/billing_and_byok.md docs/architecture/billing_and_provider_keys.md`
- `git mv docs/architecture/scenarios/02_byok.md docs/architecture/scenarios/02_self_managed.md`
- BYOK → self-managed prose sweep across: `docs/architecture/{high_level,data_flow,capabilities,user_flow,ship_reflection,plan_engg_review_v2,README}.md` + `docs/architecture/scenarios/{README,01_default_install,03_balance_gate}.md`
- Fix any `[link](billing_and_byok.md)` references in peer docs after the `git mv`
- `ui/packages/design-system/src/design-system/Select.tsx` — has a stray BYOK comment

**Repo root:**
- `README.md` — "Markdown-defined. BYOK." → "Markdown-defined. Self-managed provider keys."

**Verification:** `make lint && make test && make test-integration`. The integration suite will exercise the new wire format.

### 2. §4 Website pricing surface fix (separate commit)

This depends on the §3 lib/types.ts changes already being in. Files:

- `ui/packages/website/src/lib/rates.ts` — replace with nanos-shape cross-tier exports per the spec's **Naming convention (cross-tier)** table:
  ```ts
  export const STARTER_CREDIT_NANOS = 5_000_000_000n;
  export const EVENT_NANOS = 0n;
  export const STAGE_PLATFORM_NANOS = 1_000_000n;
  export const STAGE_SELF_MANAGED_NANOS = 100_000n;
  export const RATES_DISPLAY = { STARTER_CREDIT: "$5", EVENT_RATE: "free", STAGE_PLATFORM: "$0.001", STAGE_SELF_MANAGED: "$0.0001" } as const;
  ```
  (I drafted this file once this session — the content is correct; it was reverted because the consumer rewrites weren't ready in the same commit.)
- `ui/packages/website/src/lib/rates.test.ts` — paired pin tests asserting role names + values + 10× gradient invariant
- `ui/packages/website/src/components/Pricing.tsx` — full rewrite of rate display:
  - Drop `WORKED_EXAMPLE` references (constant gone)
  - Drop `RATES_DISPLAY.eventPlatform` / `.eventByok` / `.stage` (renamed)
  - Two stage rates side-by-side: `RATES_DISPLAY.STAGE_PLATFORM` ($0.001) + `RATES_DISPLAY.STAGE_SELF_MANAGED` ($0.0001) with the 10× gradient framing
  - Subscript "stealth-mode testing rate — will rise post-GA"
  - Drop the BYOK provider-list paragraph
- `ui/packages/website/src/components/Pricing.test.tsx` — assertions for new copy + new rates + new email
- `ui/packages/website/src/components/FAQ.tsx` + test — three answers reference BYOK; rephrase to "self-managed provider key" / "your provider"
- `ui/packages/website/src/pages/Terms.tsx` — rate references update
- `ui/packages/website/src/components/Footer.tsx` — drop BYOK badge
- `ui/packages/website/src/components/FeatureFlow.tsx` — sweep
- `ui/packages/website/src/pages/{Home,Privacy}.tsx` + tests — BYOK prose sweep

### 3. §5 SUPPORT_EMAIL per repo (separate commit)

Five new files, all asserting `usezombie@agentmail.to`:
- `src/config/contact.zig` + `src/config/contact_test.zig` (Zig)
- `ui/packages/website/src/lib/contact.ts` + `contact.test.ts`
- `ui/packages/app/lib/contact.ts` + paired test
- `zombiectl/src/lib/contact.js` + paired test
- `~/Projects/docs/snippets/contact.mdx` (lands in the paired docs PR, not the lead PR)

Then sweep every `hello@usezombie.com` literal across `src/`, `ui/`, `zombiectl/`, `docs/`, `public/` and replace with the imported constant. Keep the in-tree `~/Projects/.github/profile/README.md` literal as-is per Captain's "skip .github/profile" decision.

### 4. §6 Documentation currency audit (separate commit)

Walk every spec under `docs/v2/done/M*.md` (~92 files) and grep-confirm against `~/Projects/docs/`, `docs/architecture/`, repo READMEs. Per Captain's earlier directive: **fix all drift inline in this PR** (not as follow-up specs). Capture findings in the spec's Discovery section.

### 5. Paired docs PR on `~/Projects/docs/`

Branch: `feat/m66-001-byok-retirement-docs`

- `~/Projects/docs/snippets/rates.mdx` — flip values: `STARTER_CREDIT = "$5"` (unchanged), `EVENT_RATE = "free"` (was `$0.01`), `STAGE_PLATFORM = "$0.001"`, `STAGE_SELF_MANAGED = "$0.0001"` (new key)
- `~/Projects/docs/snippets/contact.mdx` — new file with `SUPPORT_EMAIL = "usezombie@agentmail.to"` export
- BYOK prose sweep across: `index.mdx`, `concepts.mdx`, `quickstart.mdx`, `zombies/credentials.mdx`, `zombies/overview.mdx`, `zombies/install.mdx`, others as found
- `~/Projects/docs/changelog.mdx` — new `<Update>` block announcing the M66 rate cut + term retirement (template + version-bump matrix in `~/Projects/dotfiles/skills/release-template.md`)

Order: this lands AFTER the lead PR's content is locked (no further rate changes mid-review), per the original handoff's coordination rule.

### 6. CHORE(close)

Per AGENTS.md:
1. `/write-unit-test` — coverage audit against the spec's Test Specification table
2. `/review` — adversarial diff review against `docs/architecture/billing_and_provider_keys.md`, `docs/REST_API_DESIGN_GUIDELINES.md`, `docs/ZIG_RULES.md`, Failure Modes, Invariants
3. `gh pr create` — open the lead PR
4. `/review-pr` — comments on the open PR
5. `kishore-babysit-prs` — Greptile poll loop (we hit ~3 rounds on PR #312 this session; expect similar)

Mark all Dimensions/Sections `DONE` in the spec body, move `docs/v2/active/M66_001_*.md` → `docs/v2/done/`, write the changelog `<Update>`, fill in PR Session Notes (decisions, assumptions, dead ends, deferrals).

---

## Critical gotchas learned this session

1. **Schema scope was 3 files, not 1.** `schema/014_zombie_execution_telemetry.sql` (`credit_deducted_cents`) and `schema/019_model_caps.sql` (`input/output_cents_per_mtok`) MUST flip to nanos in the same commit as `017_tenant_billing.sql`. Otherwise `STAGE_PLATFORM_NANOS + in_cents + out_cents` mixes units.

2. **`model_caps` rate columns widen INTEGER → BIGINT.** `$30/M tokens` in nanos = `3e10`, beyond `INT32_MAX` (~2.1e9). The HTTP handler's `ModelCap` struct fields and the `row.get(i32, …)` reads must match — caught this when integration tests returned 503 because the i32 read against a BIGINT column errored.

3. **`mode` column is `TEXT`, not a Postgres enum.** RULE STS — value enforcement lives in `Mode.parse()` in `src/state/tenant_provider.zig`. Never write `enum_range(NULL::tenant_provider_mode)` queries; use `\d core.tenant_providers` + `Mode.parse` pin tests + `grep \bbyok\b schema/*.sql` for verification. (Greptile caught this twice this session.)

4. **No ALTER pre-v2.0.** Schema Removal Guard pre-v2.0 path (`docs/gates/schema-removal.md`) — edit existing schema files in place, no migration scripts, no `ALTER`, dev DB reseeded via `make down && make up`. Spec's earlier "ALTER TABLE … USING balance_cents * 10000000" guidance was wrong and got rewritten in `cbb23fac`.

5. **BSD `sed` doesn't support `\b` word boundaries.** First attempt at `sed -e 's/\bBYOK\b/self-managed/g'` did nothing on macOS. Use plain `s/BYOK/self-managed/g` after verifying no substring collisions (e.g. `EVENT_BYOK_CENTS` was already renamed to `EVENT_NANOS` so the broad sed was safe).

6. **`bun install` is required in a fresh worktree.** Pre-push hook runs `make test-unit-website` which invokes `vitest`; without `node_modules` the push fails with `vitest: command not found`. Run `bun install` from worktree root once after `git worktree add`.

7. **Multi-worktree container collision.** `make up` fails with "container name `zombie-postgres` already in use" if another worktree's containers are stopped-but-present. Recovery: `docker rm -f zombie-postgres zombie-redis` then retry `make up`.

8. **`zombied-api` container needs OIDC env to start.** It exits 1 with `OidcRequired` if `OIDC_JWKS_URL`/`OIDC_ISSUER`/`OIDC_AUDIENCE` aren't set. Integration tests don't need the API container — the test harness spins up its own in-process. So the API exit doesn't block test-integration.

9. **Local main diverges from origin after squash-merges.** PR #312 squash-merge created `5739c95d` on `origin/main`; local `main` had the 6 unsquashed commits, so `git pull --ff-only` rejected. Cleanup: `git fetch origin main && git reset --hard origin/main` (destructive — needs explicit user nod).

10. **GitHub SSH connection drops mid-push.** Saw this once after the pre-push hook had already passed. Symptom: `Connection to github.com closed by remote host` / `send-pack: unexpected disconnect`. Recovery: just retry the push. Pre-push hook will re-run (~12 min for the full integration + memleak suite).

11. **No hardcoded numerics in tests** (Captain's directive this session). Tests assert against `tenant_billing.STARTER_CREDIT_NANOS`, `STAGE_PLATFORM_NANOS`, etc., not against literals like `5_000_000_000` or `1_000_000` — except in pin tests where the literal IS the contract.

12. **Cross-tier role-name parity is load-bearing.** Spec's **Naming convention (cross-tier)** subsection pins identifiers identical across Zig (`STARTER_CREDIT_NANOS`), TS (`STARTER_CREDIT_NANOS`), and Mintlify snippets (`STARTER_CREDIT`). Suffix encodes the unit (`_NANOS` for raw integers, bare for display strings).

---

## Resume commands

```bash
# 1. Bring local main in sync (no merges since session end)
cd ~/Projects/usezombie && git checkout main && git pull --ff-only origin main

# 2. Enter the worktree
cd ~/Projects/usezombie-m66-001-byok-retirement
git pull --ff-only origin feat/m66-001-byok-retirement
git status   # should be clean except this handoff doc

# 3. Confirm tooling is hot
ls node_modules >/dev/null 2>&1 || bun install
docker ps --filter 'name=zombie' --format '{{.Names}} {{.Status}}'   # postgres + redis healthy?

# 4. If schemas need re-applying after pulling:
make down && make up   # reseeds dev DB

# 5. Sanity check before editing
make test   # 29/29 expected
```

---

## Open questions / decisions parked

None outstanding. Captain's prior decisions still apply:

- **Cross-repo scope:** lead PR + paired docs PR only; skip `.github/profile`
- **§6 drift policy:** fix all drift inline in this PR (not follow-up specs)
- **Migration:** forward-only, pre-v2.0 RULE NLG clean break (no ALTER, no rescaling)
- **Naming:** cross-tier role names identical Zig + TS; bare role name on Mintlify display strings
- **`make migrate` does not exist** in this codebase — schema reseed is `make down && make up`

---

🤖 Authored by Claude Opus 4.7 (1M context). Hand off whenever.
