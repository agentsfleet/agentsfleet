<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
- See docs/TEMPLATE.md "Prohibited" section above for canonical list.
-->

# M66_001: BYOK term retirement, nanos unit migration, traction rates, single support email

**Prototype:** v2.0.0
**Milestone:** M66
**Workstream:** 001
**Date:** May 10, 2026
**Status:** PENDING
**Priority:** P1 — user-facing pricing change + breaking API rename + single canonical contact email; gates the marketing/economics shape for stealth-mode design partners.
**Categories:** API, CLI, DOCS, UI
**Batch:** B1 — single workstream, sequential sections.
**Branch:** feat/m66-001-byok-retirement (created at CHORE(open))
**Depends on:** none (M65 marketing rephrase work landed via PRs #310 and #311; this spec supersedes the M65 vocabulary split).

**Canonical architecture:** `docs/architecture/billing_and_byok.md` (renamed to `billing_and_provider_keys.md` by §5 of this spec) §0 vocabulary preamble, §1 two postures, §2 pure-credits.

---

## Implementing agent — read these first

1. **`src/state/tenant_billing.zig`** — current canonical rate constants (`STARTER_CREDIT_CENTS`, `EVENT_PLATFORM_CENTS`, `EVENT_BYOK_CENTS`, `STAGE_CENTS`). **Mirror this:** the constant naming + paired pin-test pattern. **Replace this:** the `_CENTS` suffix becomes `_NANOS` and the BYOK constant collapses into a single `EVENT_NANOS = 0`.
2. **`ui/packages/website/src/lib/rates.ts`** — TS mirror of the Zig constants with `RATES_CENTS` / `RATES_DISPLAY` shape. **Mirror this:** the named-constant + paired `rates.test.ts` discipline. **Replace this:** drop `eventPlatform` / `eventByok` distinction; collapse to `event` since both modes price events at zero post-M66.
3. **`~/Projects/docs/snippets/rates.mdx`** — Mintlify-side rate snippet. **Replace these values:** `STARTER_CREDIT = "$5"` (unchanged), `EVENT_RATE = "free"` (was `$0.01`), `STAGE_PLATFORM = "$0.001"`, `STAGE_SELF_MANAGED = "$0.0001"` (new key — two stage rates because the gradient between modes is the whole point). Role names must match Zig/TS exactly — see **Naming convention (cross-tier)** below.
4. **`docs/architecture/billing_and_byok.md` §0 + §1**" — current two-posture model. **Mirror this:** the architecture-doc convention of explicit posture matrix + §0 vocabulary preamble. **Replace these:** every "BYOK" surface in §1, plus the file rename to `billing_and_provider_keys.md`. Internal historical-note preserves the BYOK lineage.
5. **`docs/v2/done/M48_001_P1_API_CLI_UI_BYOK_PROVIDER.md`** — original spec that introduced the BYOK posture. **Read this:** to understand the data model, vault path, and `crypto_store.load()` flow that this spec deliberately preserves.
6. **`docs/v2/done/M51_001_P1_DOCS_SITE_REWRITE_AND_ARCH_CROSSREF.md`** + the `M51 follow-up` changelog entry — establishes the pre-v2.0 pattern for breaking-API removal (HTTP 404, no graceful 410). This spec follows the same posture for the schema-enum rename and CLI-flag rename.
7. **`docs/REST_API_DESIGN_GUIDELINES.md`** §1 (URL design), §4 (error shapes), §10 (pre-PR gates) — the API value rename `mode: "byok"` → `mode: "self_managed"` lands a clean 4xx for the old value with a `replacement` hint per the error-registry pattern.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline. Cross-cutting: RULE UFS (string literals → named constants, applies to the new `SUPPORT_EMAIL` constants), RULE CTM (cross-tier mirroring, applies to rate constants pinned across Zig + TS + docs snippet), RULE TST-NAM (no milestone IDs in test names or code bodies).
- **`docs/ZIG_RULES.md`** — applies to every `.zig` edit in §1, §2, §6: pg-drain lifecycle (line 14-19), Tagged Unions for Result Types, Multi-Step Init errdefer Chain Pattern, Cross-Compile Verification.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — applies when the diff touches `src/http/handlers/tenant_provider.zig` or `public/openapi/**`. §1 URL design, §4 error shapes (clean 4xx with replacement hint), §7 5-place route registration, §8 `Hx` handler interface, §10 pre-PR gates.
- **`docs/SCHEMA_CONVENTIONS.md`** — applies to the new migration in §1.
- **`docs/architecture/billing_and_byok.md`** (renamed to `billing_and_provider_keys.md` by this spec) — §0 vocabulary preamble + §1 two postures. After rename, this doc IS canonical for the runtime two-posture model; the spec keeps it consistent with code in the same diff.
- **`docs/LOGGING_STANDARD.md`** §3 wire format, §4 severity, §5 error-code embedding — log scope renames `byok_credential_*` → `self_managed_credential_*` must preserve scope-tag conventions.
- **`docs/AUTH.md`** — credential vault paths are deliberately unchanged; this spec does NOT touch `crypto_store.load()` semantics.
- **`docs/BUN_RULES.md`** — applies to `ui/packages/{website,app}/**.{ts,tsx}` edits in §3, §4: TS FILE SHAPE DECISION at PLAN, const/import/Bun-primitive discipline.
- **RULE NLG** (no legacy framing pre-v2.0) — explicitly invoked. This spec REMOVES the BYOK identifier; it does not add legacy scaffolding. No `--byok` alias on CLI; no `mode: "byok"` accept on API; no `Mode.byok` deprecated variant left in source.

---

## Overview

### Goal (testable)

After this spec lands: (a) `grep -rn '\bBYOK\b' src/ ui/ zombiectl/ public/ docs/architecture/` returns zero hits in active source/copy (with named exemptions for the architecture-doc historical note and the `~/Projects/docs/changelog.mdx` archive); (b) `mode: "byok"` to `PUT /v1/tenants/me/provider` returns HTTP 400 with error code `UZ-PROVIDER-MODE-RENAMED` naming `self_managed` as the replacement; (c) `cat src/state/tenant_billing.zig | grep -E "STAGE_NANOS|EVENT_NANOS|STARTER_CREDIT_NANOS"` shows `STAGE_PLATFORM_NANOS = 1_000_000`, `STAGE_SELF_MANAGED_NANOS = 100_000`, `EVENT_NANOS = 0`, `STARTER_CREDIT_NANOS = 5_000_000_000`; (d) every literal `hello@usezombie.com` is gone from the repo, replaced by an import of a per-repo `SUPPORT_EMAIL` constant resolving to `usezombie@agentmail.to`.

### Problem

"BYOK" is now the wrong term at every surface. M65 split the vocabulary (user-facing prose says "bring your own model"; internal identifiers stay BYOK), and the split is unstable — every code review now has to enforce the layer boundary, and drift will leak the internal term into user copy. Beyond the term, the M66_001 traction rates ($0/event both modes, $0.001/stage platform, $0.0001/stage self-managed) require a unit migration: cents (`i64`) cannot hold $0.0001, and even micros (1/1M USD) bottom out at $0.000001. The canonical billing unit must move to **nanos (1/1,000,000,000 USD = 9 decimal places of precision)**, giving headroom for $0.000001 today and any further sub-cent rate down to $0.000000001 without another unit migration. `i64` BIGINT holding nanos caps a single tenant balance at ~$9.2B, far above any realistic cap. And the support email is split across `hello@usezombie.com` (Pricing.tsx) and `usezombie@agentmail.to` (Privacy/Terms/docs) — every contact surface should resolve to one constant per repo. Finally, the marketing pricing page (`Pricing.tsx`, `FAQ.tsx`, the docs site) still surfaces M65 rates and BYOK provider lists; without a paired update, the M66 rate change ships invisible to users.

### Solution summary

One workstream, six sections:

- **§1** rename the canonical billing unit from cents to nanos across schema (column type + value scaling), Zig constants, paired pin tests.
- **§2** introduce M66 traction rates: `EVENT_NANOS = 0` (single, both postures), `STAGE_PLATFORM_NANOS = 1_000_000`, `STAGE_SELF_MANAGED_NANOS = 100_000`, `STARTER_CREDIT_NANOS = 5_000_000_000`. Update every consumer.
- **§3** retire the BYOK term aggressively: schema enum value `'byok'` → `'self_managed'` (PG14 `ALTER TYPE RENAME VALUE` or constraint swap), Zig `Mode.byok` → `Mode.self_managed`, log scope rename, API wire format rename (clean break — no alias), TS `Mode` type, app `ByokFields.tsx` → `ProviderKeyFields.tsx`, CLI `--byok` → `--self-managed`. Architecture docs renamed: `billing_and_byok.md` → `billing_and_provider_keys.md`, `scenarios/02_byok.md` → `scenarios/02_self_managed.md`.
- **§4** website pricing surface fix: `Pricing.tsx`, `FAQ.tsx`, `lib/rates.ts` updated for the new rates; introductory-rate framing ("stealth-mode testing rate — will rise post-GA"); platform-vs-self-managed gradient surfaced as the friction-reduction signal.
- **§5** single canonical `SUPPORT_EMAIL` constant per repo (Zig, website TS, app TS, CLI JS, docs MDX snippet) all asserting `usezombie@agentmail.to`. Paired pin tests. Sweep all `hello@usezombie.com` literals.
- **§6** documentation currency audit: walk every spec under `docs/v2/done/` and grep-confirm `~/Projects/docs/`, `docs/architecture/`, repo READMEs, and the org-profile README are aligned with what those specs actually shipped. Surface any drift; either fix in this PR (if mechanical) or file as an explicit follow-up spec.

User-visible outcome: the docs site, marketing site, dashboard, and CLI all speak one vocabulary ("self-managed provider key" or "you bring your provider and model"). The pricing page shows two stage rates with an explicit gradient, communicating "platform mode is convenient; self-managed mode is 10× cheaper to scale." Every contact surface points to `usezombie@agentmail.to`. The credit pool is denominated in nanos, leaving room for $0.00001 rates without re-migration.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `schema/{NNN}_nanos_unit_and_provider_mode_rename.sql` | CREATE | New migration: rename `balance_cents` column → `balance_nanos` (multiply existing rows × 10,000,000 (cents → nanos: 1¢ = 10M nanos)), rename `tenant_provider_mode` enum value `'byok'` → `'self_managed'`. Single transaction. |
| `schema/embed.zig` | EDIT | Embed the new migration. |
| `src/cmd/common.zig` | EDIT | Append to migration array. |
| `src/state/tenant_billing.zig` | EDIT | Constants renamed `_CENTS` → `_NANOS`, value set rebuilt from M66 rate table (not a value-preserving rescale). Drop `EVENT_BYOK_CENTS` and `EVENT_PLATFORM_CENTS` (collapsed into single `EVENT_NANOS = 0`). Drop single `STAGE_CENTS`; add posture-dispatched `STAGE_PLATFORM_NANOS = 1_000_000` and `STAGE_SELF_MANAGED_NANOS = 100_000`. `STARTER_CREDIT_NANOS = 5_000_000_000`. Column type stays `BIGINT` (i64). |
| `src/state/tenant_billing_test.zig` | EDIT | Pin tests for all new constants. New negative test: parsing legacy `'byok'` string into `Mode` returns `error.UnknownMode`. |
| `src/state/tenant_provider.zig` | EDIT | `Mode.byok` → `Mode.self_managed`. Update `displayName()`, `parse()`, every match arm. |
| `src/http/handlers/tenant_provider.zig` | EDIT | Accept only `mode: "self_managed"` on input; reject `"byok"` with HTTP 400 + `UZ-PROVIDER-MODE-RENAMED`. Update response body. |
| `src/zombie/executor.zig` | EDIT | Posture matching arms. |
| `src/observability/scoped.zig` (or wherever scopes are declared) | EDIT | Log scope rename `byok_credential_*` → `self_managed_credential_*`. |
| `src/errors/error_registry.zig` | EDIT | Add `UZ-PROVIDER-MODE-RENAMED` with `replacement: "self_managed"` hint. |
| `src/config/contact.zig` | CREATE | `pub const SUPPORT_EMAIL: []const u8 = "usezombie@agentmail.to";` |
| `src/config/contact_test.zig` | CREATE | Pin test asserting the exact string. |
| `public/openapi.json` | EDIT | Rename enum value `byok` → `self_managed` in `TenantProviderMode` schema; bump example payloads. |
| `docs/architecture/billing_and_byok.md` → `docs/architecture/billing_and_provider_keys.md` | RENAME (`git mv`) | File rename. Body §0 vocabulary preamble updated to record the BYOK retirement; §1 prose rewrites BYOK → self-managed. Historical note preserves the lineage. |
| `docs/architecture/scenarios/02_byok.md` → `docs/architecture/scenarios/02_self_managed.md` | RENAME (`git mv`) | File rename. Body sweep. Update intra-doc links from peer scenarios. |
| `docs/architecture/{high_level,data_flow,capabilities,office_hours_v2,plan_engg_review_v2}.md`, `docs/architecture/README.md` | EDIT | Sweep BYOK → "self-managed provider keys" in prose. Fix any `[link](billing_and_byok.md)` references. |
| `README.md` (repo root) | EDIT | "Markdown-defined. BYOK." → "Markdown-defined. Self-managed provider keys." |
| `ui/packages/website/src/lib/rates.ts` | EDIT | Drop `RATES_CENTS` object wrapper. Export named constants matching Zig identifier-for-identifier: `STARTER_CREDIT_NANOS = 5_000_000_000n`, `EVENT_NANOS = 0n`, `STAGE_PLATFORM_NANOS = 1_000_000n`, `STAGE_SELF_MANAGED_NANOS = 100_000n` (BigInt to preserve precision past `Number.MAX_SAFE_INTEGER`). Drop `eventByok` / nested-object shape. Display strings live in a paired `RATES_DISPLAY` map keyed by the same role names (`STARTER_CREDIT`, `EVENT_RATE`, `STAGE_PLATFORM`, `STAGE_SELF_MANAGED`) — see **Naming convention (cross-tier)** below. |
| `ui/packages/website/src/lib/rates.test.ts` | EDIT | Pin tests for new shape. |
| `ui/packages/website/src/lib/contact.ts` | CREATE | `export const SUPPORT_EMAIL = "usezombie@agentmail.to";` |
| `ui/packages/website/src/lib/contact.test.ts` | CREATE | Pin test. |
| `ui/packages/website/src/components/Pricing.tsx` | EDIT | Rate line shows two stage rates. Drop `eventPlatform`/`eventByok` from `BILLED_FLOW`; render event cell as "free". Drop "BYOK on…" prose at L45. Drop "BYOK · your bill" at L127 → "Your provider · your bill". Add introductory-rate framing. Reference `SUPPORT_EMAIL` import. |
| `ui/packages/website/src/components/Pricing.test.tsx` | EDIT | Assertions updated for new rates + new copy + new email. |
| `ui/packages/website/src/components/FAQ.tsx`, `FAQ.test.tsx` | EDIT | Three answers reference BYOK; rewrite to "self-managed provider key" / "your provider". |
| `ui/packages/website/src/components/Footer.tsx` | EDIT | Drop BYOK badge. |
| `ui/packages/website/src/components/FeatureFlow.tsx` | EDIT | Sweep. |
| `ui/packages/website/src/pages/{Home,Privacy,Terms}.tsx` + paired tests | EDIT | Sweep BYOK references. Switch all email literals to `SUPPORT_EMAIL` import. |
| `ui/packages/app/app/(dashboard)/settings/provider/components/ByokFields.tsx` → `ProviderKeyFields.tsx` | RENAME (`git mv`) | File + component rename. |
| `ui/packages/app/app/(dashboard)/settings/provider/components/ProviderSelector.tsx` | EDIT | "Switch to BYOK" → "Use my own provider key". Tab "BYOK" → "Self-managed". Test IDs `provider-byok-*` → `provider-self-managed-*`. |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` | EDIT | Badge + tooltip rephrase. |
| `ui/packages/app/lib/types.ts` | EDIT | `Mode = "platform" \| "byok"` → `Mode = "platform" \| "self_managed"`. Strict break. |
| `ui/packages/app/lib/api/tenant_provider.ts`, `tenant_provider.test.ts` | EDIT | Wire format `self_managed` only. |
| `ui/packages/app/lib/contact.ts` + paired test | CREATE | `SUPPORT_EMAIL` constant + pin test. |
| `ui/packages/app/tests/{provider-selector,billing-usage-tab,dashboard-coverage}.test.ts` | EDIT | Copy + test ID assertions. |
| `zombiectl/src/cli.js` | EDIT | `tenant provider set --byok` → `--self-managed`. Stderr error on legacy flag. Help text rephrase. |
| `zombiectl/src/lib/api.js` | EDIT | Send `mode: "self_managed"`. |
| `zombiectl/src/lib/contact.js` + paired test | CREATE | `SUPPORT_EMAIL` constant + pin test. |
| `zombiectl/README.md` | EDIT | Sweep BYOK references. |
| `~/Projects/docs/snippets/rates.mdx` | EDIT (paired companion PR) | Display-shape exports keyed identically to Zig/TS role names: `STARTER_CREDIT = "$5"`, `EVENT_RATE = "free"`, `STAGE_PLATFORM = "$0.001"`, `STAGE_SELF_MANAGED = "$0.0001"`. Role-name parity is load-bearing — see **Naming convention (cross-tier)**. |
| `~/Projects/docs/snippets/contact.mdx` | CREATE (paired companion PR) | `SUPPORT_EMAIL` export. |
| `~/Projects/docs/{index,concepts,quickstart,zombies/credentials,zombies/overview,zombies/install}.mdx` | EDIT (paired companion PR) | Sweep BYOK in prose. Use `{SUPPORT_EMAIL}` interpolations. New `<Update>` entry in `changelog.mdx` announcing the M66 rate cut + term retirement. |
| `~/Projects/.github/profile/README.md` | EDIT (separate one-off branch) | Sweep BYOK references. Single literal contact email. |

> **Anti-pattern check:** every entry above names a FILE and ROLE, no line numbers or symbol names beyond what's load-bearing for scope contract.

---

## Sections (implementation slices)

### §1 — Micros unit migration

Switch the canonical billing unit from cents (`i64`) to **nanos (1/1,000,000,000 USD = 9 decimal places)**. New schema column `balance_nanos` replaces `balance_cents`; existing rows scaled `× 10_000_000` (1¢ = 10,000,000 nanos) in the same migration transaction. Zig constants rename `_CENTS` → `_NANOS`, values rebuilt from the M66 rate table (the old M65 numbers are dropped wholesale, so this isn't a unit-conversion of existing constants — it's a fresh constant set). Paired pin test in `tenant_billing_test.zig`. TS mirror in `lib/rates.ts` follows.

**Why nanos, not micros:** micros (1/1,000,000 USD = 6 decimals) bottom out at $0.000001. Nanos give 3 more decimals of headroom, enough to express $0.000000001 (one billionth) cleanly. `i64` BIGINT holding nanos caps a single tenant balance at ~$9.2 billion (`i64::MAX` = 9.22e18 nanos = ~9.22e9 USD), so the type has no realistic overflow risk.

**Implementation default:** use Postgres native column rename + `ALTER TABLE … ALTER COLUMN balance_cents TYPE BIGINT USING balance_cents * 10000000` then `ALTER COLUMN … RENAME TO balance_nanos`. If the column type stays `BIGINT`, only the `× 10_000_000` UPDATE + RENAME is needed (1¢ = 10M nanos at 1B nanos/USD). The agent confirms PG version supports this on the dev Docker image.

### Naming convention (cross-tier)

**Role names are identical across Zig, TypeScript, and Mintlify snippets.** RULE CTM (cross-tier mirroring) is load-bearing here — a renamed constant in one tier without the others drifts silently and is exactly the kind of bug paired pin tests are designed to catch.

| Role | Zig (`tenant_billing.zig`) | TS (`lib/rates.ts`) | Docs (`snippets/rates.mdx`) |
|---|---|---|---|
| Starter credit | `STARTER_CREDIT_NANOS` | `STARTER_CREDIT_NANOS` | `STARTER_CREDIT` |
| Event charge | `EVENT_NANOS` | `EVENT_NANOS` | `EVENT_RATE` |
| Stage charge (platform) | `STAGE_PLATFORM_NANOS` | `STAGE_PLATFORM_NANOS` | `STAGE_PLATFORM` |
| Stage charge (self-managed) | `STAGE_SELF_MANAGED_NANOS` | `STAGE_SELF_MANAGED_NANOS` | `STAGE_SELF_MANAGED` |

**Suffix rule:**
- `_NANOS` suffix on raw-integer constants (Zig, TS) — the underlying unit. The value is the same `i64`/`bigint` integer in both languages.
- Bare role name on Mintlify display snippets — the value is the user-facing `$`-formatted string (`"$5"`, `"free"`, `"$0.001"`). The display layer's job is presentation, not unit math.

**Pin tests assert role-name parity in both directions:**
- `src/state/tenant_billing_test.zig` asserts each Zig constant by name + value.
- `ui/packages/website/src/lib/rates.test.ts` asserts each TS export by name + value AND asserts the integer values match the Zig-side numbers (hard-coded literal mirror, with a comment pointing back to `tenant_billing.zig`).
- `~/Projects/docs/snippets/rates.test.mdx` (or equivalent docs-CI check, if introduced this milestone) asserts the four display keys exist and resolve to the expected `$`-strings.

A renamed role triggers test failures in **all three** layers, not just one.

### §2 — M66 traction rates

Replace the M65 rate constants with M66 values. New constant set:
- `STARTER_CREDIT_NANOS = 5_000_000_000` ($5)
- `EVENT_NANOS = 0` (single value, both postures)
- `STAGE_PLATFORM_NANOS = 1_000_000` ($0.001 — usezombie pays Fireworks for tokens)
- `STAGE_SELF_MANAGED_NANOS = 100_000` ($0.0001 — user pays their own provider)

Drop `EVENT_PLATFORM_CENTS`, `EVENT_BYOK_CENTS`, `STAGE_CENTS` (the single cross-mode constant). Update `compute_*_charge` functions in `tenant_billing.zig` to dispatch on posture for stage cost.

The platform/self-managed gradient is the friction-reducer: on-ramp on platform mode without bringing a key; graduate to self-managed for 10× cheaper stages once the user is serious. Document this in the `<Update>` block as a deliberate stealth-mode subsidy.

### §3 — BYOK term retirement (every tier)

Rename in lockstep across schema, Zig, API wire format, TS, app components, CLI, architecture docs.

**Implementation default:** use `git mv` for file renames to preserve history. PG enum value rename via `ALTER TYPE tenant_provider_mode RENAME VALUE 'byok' TO 'self_managed'` (PG14+). If the column uses a CHECK constraint with string literals instead of a Postgres enum, drop-and-recreate the constraint with the new literal then `UPDATE` existing rows (same transaction).

**Clean break — no alias:** the API rejects `mode: "byok"` with HTTP 400 + `UZ-PROVIDER-MODE-RENAMED` (replacement hint = `"self_managed"`). The CLI rejects `--byok` with stderr message naming the new flag. Pre-v2.0 RULE NLG forbids legacy scaffolding.

### §4 — Website pricing surface fix

Update `Pricing.tsx`, `FAQ.tsx`, `lib/rates.ts` to surface the new rates. Pricing card displays two stage rates side-by-side with the friction-reduction framing ("$0.001/stage on platform default, $0.0001/stage when you bring your own provider key — 10× cheaper to scale"). Drop the BYOK provider-list paragraph; the diagram below already names providers. Stealth-mode banner lands in the Pricing card itself (already shipped via PR #311; this section keeps it consistent with the new rates).

**Introductory-rate framing:** the rate line carries a small subscript "stealth-mode testing rate — will rise post-GA" so future ratchets are expected behavior, not surprise price hikes.

### §5 — Single canonical SUPPORT_EMAIL per repo

Five new constant files (one per repo / package) all asserting `usezombie@agentmail.to`:
- `src/config/contact.zig`
- `ui/packages/website/src/lib/contact.ts`
- `ui/packages/app/lib/contact.ts`
- `zombiectl/src/lib/contact.js`
- `~/Projects/docs/snippets/contact.mdx`

Paired pin test in each repo asserts the exact string. Sweep replaces every `hello@usezombie.com` literal with the constant import. The org-profile README (`~/Projects/.github/profile/README.md`) carries a single literal (markdown can't import).

### §6 — Documentation currency audit

Before merging, walk every spec under `docs/v2/done/` and grep-confirm:
- `~/Projects/docs/` (Mintlify site) prose aligns with what shipped.
- `docs/architecture/` (in-repo) reflects the current runtime shape.
- Repo READMEs (`usezombie/README.md`, `zombiectl/README.md`, architecture `README.md`) are current.
- Org profile (`~/Projects/.github/profile/README.md`) is current.

Any drift is either (a) fixed in this PR (mechanical wording fixes) or (b) filed as an explicit follow-up spec under `docs/v2/pending/M66_NNN_DOCS_DRIFT_*.md`. The Discovery section captures every drift finding before close.

---

## Interfaces

### HTTP — `PUT /v1/tenants/me/provider`

**Before (M65):** request `{ "mode": "platform" | "byok", "credential_ref"?: string }`.
**After (M66):** request `{ "mode": "platform" | "self_managed", "credential_ref"?: string }`.

`mode: "byok"` returns:
```
HTTP/1.1 400 Bad Request
Content-Type: application/problem+json

{
  "type": "https://docs.usezombie.com/errors/UZ-PROVIDER-MODE-RENAMED",
  "title": "Provider mode 'byok' was renamed to 'self_managed'",
  "status": 400,
  "code": "UZ-PROVIDER-MODE-RENAMED",
  "replacement": "self_managed"
}
```

### CLI — `zombiectl tenant provider set`

**Before (M65):** `--byok <credential>` flag.
**After (M66):** `--self-managed <credential>`. `--byok` → stderr error + exit 2:

```
zombiectl: --byok was renamed to --self-managed in M66.
See https://docs.usezombie.com/zombies/credentials.
```

### Schema — `core.tenant_providers.mode`

**Before:** `tenant_provider_mode` Postgres enum with values `('platform', 'byok')`.
**After:** `('platform', 'self_managed')`. Migration scales `core.tenant_billing.balance_cents` by × 10,000,000 (cents → nanos: 1¢ = 10M nanos) and renames to `balance_nanos`.

### TS — `Mode` type alias

**Before:** `type Mode = "platform" | "byok"`.
**After:** `type Mode = "platform" | "self_managed"`.

---

## Failure Modes

| Mode | Cause | Handling | Test |
|---|---|---|---|
| Old client sends `mode: "byok"` | Pre-M66 SDK / cached client / curl from a runbook | HTTP 400 + `UZ-PROVIDER-MODE-RENAMED` with `replacement: "self_managed"`. No alias. | `test_provider_mode_byok_returns_400_with_replacement` |
| Existing tenant rows with `mode = 'byok'` after deploy | Migration didn't run | Migration runs in single transaction with the schema rename; PG14+ `ALTER TYPE RENAME VALUE` is metadata-only. | `test_migration_renames_existing_byok_rows` |
| TS Mode parser receives `"byok"` from API | Stale client cache / network race during cutover | Strict TypeScript union → parse error → toast. No silent coerce. | `test_mode_parser_rejects_byok` |
| CLI --byok flag passed | User following old docs / shell history | Stderr error with replacement flag name; non-zero exit. | `test_cli_legacy_byok_flag_emits_error` |
| Migration on non-PG14 cluster | Older Postgres in some environment | Fall back to (a) add `'self_managed'` value, (b) UPDATE existing rows, (c) DROP `'byok'` value, three transactions. Detected at migration runtime via `SELECT version()` check. | `test_migration_pg14_path_and_pre14_fallback` |
| Saved Grafana dashboards keyed on `byok_credential_*` log scopes | Log scope rename | Listed in Discovery; saved-search update is a follow-up doc-only change. Not blocking. | n/a — operational |
| Architecture-doc intra-doc links broken after `git mv` | `[link](billing_and_byok.md)` references in peer docs | DOC READ GATE on `docs/architecture/**` catches unfixed links during execute; sweep listed in §3. | `test_arch_doc_links_resolve` (grep-based) |
| Frontend `data-testid` rename breaks downstream e2e | Out-of-repo e2e suite depends on `provider-byok-*` IDs | Inventory all references during Discovery; flag any out-of-repo dependency in spec body before CHORE(open). | `test_provider_selector_test_ids_renamed` |
| Cents → nanos migration overflows | `i64` BIGINT max = 9.22e18 nanos = ~$9.22B per tenant. Even 1 trillion cents (~$10B) ×10M = 1e19 — overflows. | Pre-flight check: `SELECT MAX(balance_cents) FROM core.tenant_billing` must be < 9e11 cents (~$9B) before migration runs. Realistic balances are 4–5 orders of magnitude below this. | `test_migration_no_overflow_at_max_balance` + pre-flight assertion |
| Pin tests drift between Zig and TS rates | Cross-tier mirror not enforced | Paired pin tests in `tenant_billing_test.zig` and `rates.test.ts` assert the exact same values. CI fails on drift. | `test_rates_pinned_zig` + `test_rates_pinned_ts` |

---

## Invariants

1. **No surface mentions BYOK except named exemptions.** Enforced by `make lint` step that greps `\bBYOK\b` against `src/`, `ui/`, `zombiectl/`, `public/`, `docs/architecture/` and asserts zero hits. Allowlist file lists the architecture-doc historical-note line and the docs-site changelog archive.
2. **Both rate constants are pinned identically across Zig + TS.** Enforced by paired test asserting exact i64 values match.
3. **`SUPPORT_EMAIL` is `usezombie@agentmail.to` everywhere.** Enforced by per-repo pin test asserting the exact string + `make lint` step grepping for `hello@usezombie.com` and asserting zero hits.
4. **Schema enum has exactly two values: `('platform', 'self_managed')`.** Enforced by integration test querying `pg_enum` on `tenant_provider_mode` after migration.
5. **`Mode.parse("byok")` returns `error.UnknownMode`.** Enforced by Zig pin test.
6. **API responses always serialize `mode` as `"platform"` or `"self_managed"` — never `"byok"`.** Enforced by integration test on `GET /v1/tenants/me/provider` after migration.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_starter_credit_nanos_pinned` | `STARTER_CREDIT_NANOS == 5_000_000_000` |
| `test_event_nanos_zero_both_postures` | `compute_event_charge(.platform) == 0`, `compute_event_charge(.self_managed) == 0` |
| `test_stage_platform_nanos_pinned` | `STAGE_PLATFORM_NANOS == 1_000_000` |
| `test_stage_self_managed_nanos_pinned` | `STAGE_SELF_MANAGED_NANOS == 100_000` |
| `test_compute_stage_charge_dispatches_on_posture` | `.platform` → 1_000_000 nanos, `.self_managed` → 100_000 nanos |
| `test_mode_parse_self_managed_succeeds` | `Mode.parse("self_managed")` → `.self_managed` |
| `test_mode_parse_byok_fails` | `Mode.parse("byok")` → `error.UnknownMode` |
| `test_migration_renames_existing_byok_rows` | After migration, `SELECT mode FROM core.tenant_providers WHERE mode = 'byok'` returns 0 rows; previously-byok rows now show `'self_managed'` |
| `test_migration_balance_cents_to_nanos_scales` | After migration, every tenant's `balance_nanos` == old `balance_cents` × 10_000_000 |
| `test_migration_no_overflow_at_max_balance` | Migration on a tenant with the largest pre-migration balance does not overflow `BIGINT` |
| `test_migration_pg14_path_and_pre14_fallback` | Migration succeeds on PG14+ (single ALTER TYPE) and on PG13 (3-step fallback). Skip on environments unable to test both. |
| `test_provider_mode_byok_returns_400_with_replacement` | `PUT /v1/tenants/me/provider` with `{"mode":"byok"}` → 400 + body `{"code":"UZ-PROVIDER-MODE-RENAMED","replacement":"self_managed"}` |
| `test_provider_mode_self_managed_accepted` | Same endpoint with `{"mode":"self_managed","credential_ref":"my-key"}` → 200 |
| `test_provider_mode_response_never_emits_byok` | `GET /v1/tenants/me/provider` after a `self_managed` set → response.mode === "self_managed" |
| `test_cli_legacy_byok_flag_emits_error` | `zombiectl tenant provider set --byok foo` → exit 2 + stderr names `--self-managed` |
| `test_cli_self_managed_flag_works` | `zombiectl tenant provider set --self-managed foo` → exit 0 |
| `test_rates_pinned_zig` | `tenant_billing_test.zig` pins all four nanos constants. |
| `test_rates_pinned_ts` | `rates.test.ts` pins TS mirror to identical values. |
| `test_pricing_renders_two_stage_rates` | `Pricing.tsx` test asserts both `pricing-rate-stage-platform` and `pricing-rate-stage-self-managed` test IDs render with $0.001 / $0.0001 strings. |
| `test_pricing_renders_introductory_rate_subscript` | Pricing card shows "stealth-mode testing rate — will rise post-GA". |
| `test_pricing_drops_byok_provider_list` | `screen.queryByText(/BYOK on/)` returns null. |
| `test_provider_selector_test_ids_renamed` | All `provider-byok-*` test IDs absent; `provider-self-managed-*` present. |
| `test_provider_key_fields_component_renamed` | `import { ProviderKeyFields }` resolves; `import { ByokFields }` is a TypeScript error. |
| `test_support_email_pinned_zig` | `src/config/contact_test.zig` asserts `SUPPORT_EMAIL == "usezombie@agentmail.to"` |
| `test_support_email_pinned_ts_website` | `ui/packages/website/src/lib/contact.test.ts` same assertion. |
| `test_support_email_pinned_ts_app` | Same in app. |
| `test_support_email_pinned_cli` | Same in CLI. |
| `test_no_hello_at_usezombie_dot_com_literal_remains` | grep `hello@usezombie.com` against full repo returns zero hits (allowlist: changelog archives if any). |
| `test_no_byok_term_in_active_source` | grep `\bBYOK\b` against active source/copy returns zero hits (allowlist: arch-doc historical note line, docs-site changelog archive). |
| `test_arch_doc_links_resolve` | grep all `[label](*.md)` references in `docs/architecture/**` and assert each target exists. |

---

## Acceptance Criteria

- [ ] `make lint` clean — verify: `make lint`
- [ ] `make test` passes — verify: `make test`
- [ ] `make test-integration` passes from clean state — verify: `make down && make up && make test-integration`
- [ ] `make memleak` clean — verify: `make memleak | tail -3`
- [ ] Cross-compile clean — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean — verify: `gitleaks detect | tail -3`
- [ ] No file over 350 lines added — verify: `git diff --name-only origin/main | grep -v -E '\.md$|^vendor/' | xargs wc -l 2>/dev/null | awk '$1 > 350'`
- [ ] BYOK term sweep passes — verify: `grep -rn '\bBYOK\b' src/ ui/ zombiectl/ public/ docs/architecture/ | grep -v -E '(historical|legacy lineage)' | wc -l` → 0
- [ ] `hello@usezombie.com` literal sweep passes — verify: `grep -rn 'hello@usezombie\.com' src/ ui/ zombiectl/ docs/ public/ ~/Projects/docs/ | wc -l` → 0
- [ ] Migration runs cleanly on dev Docker — verify: `make down && make up && make migrate && psql -c "SELECT enum_range(NULL::tenant_provider_mode)"` returns `{platform,self_managed}`
- [ ] OpenAPI updated — verify: `cat public/openapi.json | jq '.components.schemas.TenantProviderMode.enum'` returns `["platform","self_managed"]`
- [ ] `bun run test` green in `ui/packages/{website,app}` — verify: `cd ui && bun run test`
- [ ] `make check-pg-drain` clean — verify: `make check-pg-drain`
- [ ] `make check-version` clean — verify: `make check-version`
- [ ] Documentation currency audit complete — verify: every `docs/v2/done/M*.md` walked, drift either fixed in this PR or filed as `docs/v2/pending/M66_NNN_DOCS_DRIFT_*.md`

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Build
zig build 2>&1 | tail -5

# E2: Tests (Zig)
make test && echo "PASS" || echo "FAIL"

# E3: Tests (Bun packages)
cd ui && bun run test 2>&1 | tail -10 && cd ..
cd zombiectl && bun test 2>&1 | tail -10 && cd ..

# E4: Integration
make down && make up && make test-integration 2>&1 | tail -10

# E5: Lint
make lint 2>&1 | grep -E "✓|FAIL"

# E6: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E7: Gitleaks
gitleaks detect 2>&1 | tail -3

# E8: BYOK term sweep
grep -rn '\bBYOK\b' src/ ui/ zombiectl/ public/ docs/architecture/ | \
  grep -v -E '(historical|legacy lineage)' | wc -l
echo "E8: should be 0"

# E9: hello@ literal sweep
grep -rn 'hello@usezombie\.com' src/ ui/ zombiectl/ docs/ public/ | wc -l
echo "E9: should be 0"

# E10: Schema enum check
psql "$DATABASE_URL" -c "SELECT enum_range(NULL::tenant_provider_mode)"
echo "E10: should print {platform,self_managed}"

# E11: API value rename
curl -s -X PUT http://localhost:9090/v1/tenants/me/provider \
  -H "Content-Type: application/json" \
  -d '{"mode":"byok","credential_ref":"x"}' | jq -r '.code'
echo "E11: should print UZ-PROVIDER-MODE-RENAMED"

# E12: 350-line gate
git diff --name-only origin/main | grep -v -E '\.md$|^vendor/' | \
  xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: "$2": "$1 }'

# E13: Dead code sweep — no orphan references to renamed symbols
grep -rn '\.byok\b\|"byok"\|--byok\b\|ByokFields\|EVENT_BYOK_CENTS\|STAGE_CENTS\b' src/ ui/ zombiectl/ public/ | head -10
echo "E13: should be empty"
```

---

## Dead Code Sweep

**1. Orphaned files — must be deleted from disk and git:**

| File to delete | Verify deleted |
|---|---|
| `docs/architecture/billing_and_byok.md` (renamed via `git mv`) | `test ! -f docs/architecture/billing_and_byok.md` |
| `docs/architecture/scenarios/02_byok.md` (renamed via `git mv`) | `test ! -f docs/architecture/scenarios/02_byok.md` |
| `ui/packages/app/.../components/ByokFields.tsx` (renamed via `git mv`) | `test ! -f ui/packages/app/app/(dashboard)/settings/provider/components/ByokFields.tsx` |

**2. Orphaned references — zero remaining imports or uses:**

| Deleted symbol or import | Grep command | Expected |
|---|---|---|
| `EVENT_BYOK_CENTS` | `grep -rn 'EVENT_BYOK_CENTS' src/ ui/ zombiectl/ public/` | 0 matches |
| `EVENT_PLATFORM_CENTS` | `grep -rn 'EVENT_PLATFORM_CENTS' src/ ui/ zombiectl/` | 0 matches |
| `STAGE_CENTS` | `grep -rn '\bSTAGE_CENTS\b' src/ ui/ zombiectl/` | 0 matches |
| `STARTER_CREDIT_CENTS` | `grep -rn '\bSTARTER_CREDIT_CENTS\b' src/ ui/ zombiectl/` | 0 matches |
| `Mode.byok` (Zig) | `grep -rn '\.byok\b' src/` | 0 matches |
| `mode: "byok"` (TS / JS) | `grep -rn '"byok"' src/ ui/ zombiectl/ public/` | 0 matches |
| `--byok` flag | `grep -rn '\-\-byok\b' zombiectl/ docs/` | 0 matches |
| `ByokFields` import | `grep -rn 'ByokFields' src/ ui/` | 0 matches |
| `provider-byok-` test ID | `grep -rn 'provider-byok-' ui/` | 0 matches |
| `byok_credential_*` log scope | `grep -rn 'byok_credential' src/` | 0 matches |
| `hello@usezombie\.com` literal | `grep -rn 'hello@usezombie\.com' src/ ui/ zombiectl/ docs/ public/` | 0 matches |
| `RATES_DISPLAY.eventByok` | `grep -rn 'eventByok' ui/` | 0 matches |
| `[link](billing_and_byok\.md)` | `grep -rn 'billing_and_byok\.md' docs/` | 0 matches |

---

## Discovery (consult log)

(Empty at spec creation — populated as Legacy-Design Consult Guard, Architecture Consult, and other action-triggered guards fire during EXECUTE.)

Expected entries:
- Inventory of `provider-byok-*` data-testid usage (out-of-repo e2e dependency check) before §3 begins.
- Postgres version on dev Docker image confirmed before §1 migration approach is chosen.
- Documentation drift findings from §6 audit; either fixed in PR or filed as follow-up specs.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|---|---|---|---|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage against this spec's Test Specification. Catches happy-path-only tests, missing negatives, fixture drift. | Skill returns clean. Iteration count + final coverage summary in PR Session Notes. |
| After tests pass, still before CHORE(close) | `/review` | Adversarial diff review against this spec, `docs/architecture/billing_and_provider_keys.md`, `docs/REST_API_DESIGN_GUIDELINES.md` (handler), `docs/ZIG_RULES.md`, Failure Modes, Invariants. | Skill returns clean OR every finding dispositioned. |
| After `gh pr create` opens the PR | `/review-pr` | Review-comments the open PR against the now-immutable diff. | Comments addressed inline before requesting human review or merging. |
| After every push | `kishore-babysit-prs` | Polls Greptile per cadence, walks every review id, triages P0/P1 vs RULES.md, fixes+replies+reschedules. Stops on two consecutive empty polls. | Final report in PR Session Notes. |

---

## Verification Evidence

(Filled during VERIFY.)

| Check | Command | Result | Pass? |
|---|---|---|---|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Lint | `make lint` | | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | | |
| Memleak | `make memleak \| tail -3` | | |
| Gitleaks | `gitleaks detect \| tail -3` | | |
| 350L gate | `git diff --name-only origin/main \| ...` | | |
| BYOK term sweep | `grep -rn '\bBYOK\b' ...` | | |
| `hello@usezombie.com` sweep | `grep -rn 'hello@usezombie\.com' ...` | | |
| Schema enum | `psql -c "SELECT enum_range(NULL::tenant_provider_mode)"` | | |
| OpenAPI enum | `jq '.components.schemas.TenantProviderMode.enum' public/openapi.json` | | |

---

## Out of Scope

- **TEMPLATE.md determinism upgrades** (the seven-point list from the M66 proposal). Filed as future work; orthogonal to BYOK / rates / email.
- **Grafana saved-search rename** for `byok_credential_*` log scopes. Listed in §6 Discovery; will be a follow-up doc-only branch updating saved queries, not blocking on this spec.
- **Schema column rename `posture`/`mode` if applicable.** Existing column name preserved; only the enum value changes.
- **Public docs publishing of `UZ-PROVIDER-MODE-RENAMED` error code page on docs.usezombie.com.** Error registry update lands in this spec; the docs page lands in the paired docs PR's `<Update>` block.
- **Currency / locale display formatting.** All rates render in USD; localization is future work.
- **Volume-tier pricing or post-GA ratchet schedule.** Captured as a marketing/strategy decision; not in this spec.
