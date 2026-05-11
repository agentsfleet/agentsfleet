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

# M66_001: BYOK term retirement, nanos unit, traction rates, single support email

**Prototype:** v2.0.0
**Milestone:** M66
**Workstream:** 001
**Date:** May 10, 2026
**Status:** DONE
**Priority:** P1 — user-facing pricing change + breaking API rename + single canonical contact email; gates the marketing/economics shape for stealth-mode design partners.
**Categories:** API, CLI, DOCS, UI
**Batch:** B1 — single workstream, sequential sections.
**Branch:** feat/m66-001-byok-retirement
**Depends on:** none (M65 marketing rephrase work landed via PRs #310 and #311; this spec supersedes the M65 vocabulary split).

**Canonical architecture:** `docs/architecture/billing_and_byok.md` (renamed to `billing_and_provider_keys.md` by §5 of this spec) §0 vocabulary preamble, §1 two postures, §2 pure-credits.

---

## Implementing agent — read these first

1. **`src/state/tenant_billing.zig`** — current canonical rate constants (`STARTER_CREDIT_CENTS`, `EVENT_PLATFORM_CENTS`, `EVENT_BYOK_CENTS`, `STAGE_CENTS`). **Mirror this:** the constant naming + paired pin-test pattern. **Replace this:** the `_CENTS` suffix becomes `_NANOS` and the BYOK constant collapses into a single `EVENT_NANOS = 0`.
2. **`ui/packages/website/src/lib/rates.ts`** — TS mirror of the Zig constants with `RATES_CENTS` / `RATES_DISPLAY` shape. **Mirror this:** the named-constant + paired `rates.test.ts` discipline. **Replace this:** drop `eventPlatform` / `eventByok` distinction; collapse to `event` since both modes price events at zero post-M66.
3. **`~/Projects/docs/snippets/rates.mdx`** — Mintlify-side rate snippet. **Replace these values:** `STARTER_CREDIT = "$5"` (unchanged), `EVENT_RATE = "free"` (was `$0.01`), `STAGE_PLATFORM = "$0.001"`, `STAGE_SELF_MANAGED = "$0.0001"` (new key — two stage rates because the gradient between modes is the whole point). Role names must match Zig/TS exactly — see **Naming convention (cross-tier)** below.
4. **`docs/architecture/billing_and_byok.md` §0 + §1**" — current two-posture model. **Mirror this:** the architecture-doc convention of explicit posture matrix + §0 vocabulary preamble. **Replace these:** every "BYOK" surface in §1, plus the file rename to `billing_and_provider_keys.md`. Internal historical-note preserves the BYOK lineage.
5. **`docs/v2/done/M48_001_P1_API_CLI_UI_SELF_MANAGED_PROVIDER.md`** — original spec that introduced the BYOK posture. **Read this:** to understand the data model, vault path, and `crypto_store.load()` flow that this spec deliberately preserves.
6. **`docs/v2/done/M51_001_P1_DOCS_SITE_REWRITE_AND_ARCH_CROSSREF.md`** + the `M51 follow-up` changelog entry — establishes the pre-v2.0 pattern for breaking-API removal (HTTP 404, no graceful 410). This spec follows the same posture for the schema-enum rename and CLI-flag rename.
7. **`docs/REST_API_DESIGN_GUIDELINES.md`** §1 (URL design), §4 (error shapes), §10 (pre-PR gates) — the API value rename `mode: "byok"` → `mode: "self_managed"` lands a clean 4xx for the old value with a `replacement` hint per the error-registry pattern.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline. Cross-cutting: RULE UFS (string literals → named constants, applies to the new `SUPPORT_EMAIL` constants), RULE CTM (cross-tier mirroring, applies to rate constants pinned across Zig + TS + docs snippet), RULE TST-NAM (no milestone IDs in test names or code bodies).
- **`docs/ZIG_RULES.md`** — applies to every `.zig` edit in §1, §2, §6: pg-drain lifecycle (line 14-19), Tagged Unions for Result Types, Multi-Step Init errdefer Chain Pattern, Cross-Compile Verification.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — applies when the diff touches `src/http/handlers/tenant_provider.zig` or `public/openapi/**`. §1 URL design, §4 error shapes (clean 4xx with replacement hint), §7 5-place route registration, §8 `Hx` handler interface, §10 pre-PR gates.
- **`docs/SCHEMA_CONVENTIONS.md`** — applies to the in-place schema edits in §1 and §3 (pre-v2.0 clean-break: edit existing schema files, no `ALTER`, dev DB reseeded via `make down && make up`).
- **`docs/architecture/billing_and_byok.md`** (renamed to `billing_and_provider_keys.md` by this spec) — §0 vocabulary preamble + §1 two postures. After rename, this doc IS canonical for the runtime two-posture model; the spec keeps it consistent with code in the same diff.
- **`docs/LOGGING_STANDARD.md`** §3 wire format, §4 severity, §5 error-code embedding — log scope renames `byok_credential_*` → `self_managed_credential_*` must preserve scope-tag conventions.
- **`docs/AUTH.md`** — credential vault paths are deliberately unchanged; this spec does NOT touch `crypto_store.load()` semantics.
- **`docs/BUN_RULES.md`** — applies to `ui/packages/{website,app}/**.{ts,tsx}` edits in §3, §4: TS FILE SHAPE DECISION at PLAN, const/import/Bun-primitive discipline.
- **RULE NLG** (no legacy framing pre-v2.0) — explicitly invoked. This spec REMOVES the BYOK identifier; it does not add legacy scaffolding. No `--byok` alias on CLI; no `mode: "byok"` accept on API; no `Mode.byok` deprecated variant left in source.

---

## Overview

### Goal (testable)

After this spec lands: (a) `grep -rn '\bBYOK\b' src/ ui/ zombiectl/ public/ docs/architecture/` returns zero hits in active source/copy (with named exemptions for the architecture-doc historical note and the `~/Projects/docs/changelog.mdx` archive); (b) `mode: "byok"` to `PUT /v1/tenants/me/provider` returns HTTP 400 via the generic `UZ-REQ-001` `ERR_INVALID_REQUEST` fall-through with body message `"mode must be 'platform' or 'self_managed'"` — pre-v2.0 RULE NLG (Session Notes #2) ruled out the earlier-drafted special-case `UZ-PROVIDER-MODE-RENAMED` branch; (c) `cat src/state/tenant_billing.zig | grep -E "STAGE_NANOS|EVENT_NANOS|STARTER_CREDIT_NANOS"` shows `STAGE_PLATFORM_NANOS = 1_000_000`, `STAGE_SELF_MANAGED_NANOS = 100_000`, `EVENT_NANOS = 0`, `STARTER_CREDIT_NANOS = 5_000_000_000`; (d) every literal `hello@usezombie.com` is gone from the repo, replaced by an import of a per-repo `SUPPORT_EMAIL` constant resolving to `usezombie@agentmail.to`.

### Problem

"BYOK" is now the wrong term at every surface. M65 split the vocabulary (user-facing prose says "bring your own model"; internal identifiers stay BYOK), and the split is unstable — every code review now has to enforce the layer boundary, and drift will leak the internal term into user copy. Beyond the term, the M66_001 traction rates ($0/event both modes, $0.001/stage platform, $0.0001/stage self-managed) need a sub-cent-capable billing unit: cents (`i64`) cannot hold $0.0001, and even micros (1/1M USD) bottom out at $0.000001. The canonical billing unit must move to **nanos (1/1,000,000,000 USD = 9 decimal places of precision)**, giving headroom for $0.000001 today and any further sub-cent rate down to $0.000000001 without another unit change. `i64` BIGINT holding nanos caps a single tenant balance at ~$9.2B, far above any realistic cap. And the support email is split across `hello@usezombie.com` (Pricing.tsx) and `usezombie@agentmail.to` (Privacy/Terms/docs) — every contact surface should resolve to one constant per repo. Finally, the marketing pricing page (`Pricing.tsx`, `FAQ.tsx`, the docs site) still surfaces M65 rates and BYOK provider lists; without a paired update, the M66 rate change ships invisible to users.

### Solution summary

One workstream, six sections:

- **§1** rename the canonical billing unit from cents to nanos. Pre-v2.0 clean break: edit `schema/017_tenant_billing.sql` in place (column `balance_cents` → `balance_nanos`, CHECK rewritten), no migration file, no `ALTER`. Zig constants + paired pin tests follow.
- **§2** introduce M66 traction rates: `EVENT_NANOS = 0` (single, both postures), `STAGE_PLATFORM_NANOS = 1_000_000`, `STAGE_SELF_MANAGED_NANOS = 100_000`, `STARTER_CREDIT_NANOS = 5_000_000_000`. Update every consumer.
- **§3** retire the BYOK term aggressively: edit `schema/020_tenant_providers.sql` in place (the `mode` column is already `TEXT` with app-side enforcement per RULE STS — only the comment text changes), Zig `Mode.byok` → `Mode.self_managed`, log scope rename, API wire format rename (clean break — no alias), TS `Mode` type, app `ByokFields.tsx` → `ProviderKeyFields.tsx`, CLI `--byok` → `--self-managed`. Architecture docs renamed: `billing_and_byok.md` → `billing_and_provider_keys.md`, `scenarios/02_byok.md` → `scenarios/02_self_managed.md`.
- **§4** website pricing surface fix: `Pricing.tsx`, `FAQ.tsx`, `lib/rates.ts` updated for the new rates; introductory-rate framing ("stealth-mode testing rate — will rise post-GA"); platform-vs-self-managed gradient surfaced as the friction-reduction signal.
- **§5** single canonical `SUPPORT_EMAIL` constant per repo (Zig, website TS, app TS, CLI JS, docs MDX snippet) all asserting `usezombie@agentmail.to`. Paired pin tests. Sweep all `hello@usezombie.com` literals.
- **§6** documentation currency audit: walk every spec under `docs/v2/done/` and grep-confirm `~/Projects/docs/`, `docs/architecture/`, repo READMEs, and the org-profile README are aligned with what those specs actually shipped. Surface any drift; either fix in this PR (if mechanical) or file as an explicit follow-up spec.

User-visible outcome: the docs site, marketing site, dashboard, and CLI all speak one vocabulary ("self-managed provider key" or "you bring your provider and model"). The pricing page shows two stage rates with an explicit gradient, communicating "platform mode is convenient; self-managed mode is 10× cheaper to scale." Every contact surface points to `usezombie@agentmail.to`. The credit pool is denominated in nanos, leaving room for $0.00001 rates without another unit change. Dev DB is wiped + reseeded via `make down && make up` — no migration script, no `ALTER`, per the pre-v2.0 clean-break posture (RULE NLG, Schema Removal Guard pre-v2.0 path).

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `schema/014_zombie_execution_telemetry.sql` | EDIT (in place) | Rename column `credit_deducted_cents BIGINT` → `credit_deducted_nanos BIGINT`. Same pre-v2.0 in-place edit as `017`. (See Discovery — original spec missed this.) |
| `schema/017_tenant_billing.sql` | EDIT (in place) | Rename column `balance_cents` → `balance_nanos`. CHECK becomes `balance_nanos >= 0`. Update file-top comment to drop M65 cents prose. **No migration file, no `ALTER`** — pre-v2.0 clean break, dev DB reseeded via `make down && make up`. |
| `schema/019_model_caps.sql` | EDIT (in place) | Rename `input_cents_per_mtok INTEGER` → `input_nanos_per_mtok BIGINT`, same for `output_*`. Type widens because `$30/M` tokens in nanos = `3e10`, beyond INT32 max. Update INSERT, ON CONFLICT clause, and any seed VALUES rows in the same file. (See Discovery.) |
| `src/state/model_rate_cache.zig` | EDIT | Field rename `input_cents_per_mtok` → `input_nanos_per_mtok` (same for output); SQL column rename; cached struct + load() rebuilt. |
| `src/state/zombie_telemetry_store.zig` (+ `_test.zig`) | EDIT | Field rename `credit_deducted_cents` → `credit_deducted_nanos`; SQL columns; struct fields; insert + load + test fixtures. |
| `src/http/handlers/model_caps.zig` (+ `_integration_test.zig`) | EDIT | Wire-format rename `input_cents_per_mtok` → `input_nanos_per_mtok` (and output) in JSON serializer + `i32` → `i64` field types + integration-test assertions. (UI consumer in `ui/packages/app/` covered by the §3/§4 sweep.) |
| `src/zombie/metering.zig` (+ `_test.zig`) | EDIT | Charge-amount field rename `cents` → `nanos` in MeterResult / dispatch; SQL bound params; test fixtures. |
| `src/zombie/event_loop_writepath_integration_test.zig`, `src/state/signup_bootstrap_test.zig` | EDIT | SQL string updates for `balance_cents` column references. |
| `src/config/balance_policy.zig`, `src/zombie/event_loop_types.zig` | EDIT (comment-only) | Comment updates from `balance_cents` → `balance_nanos`. |
| `schema/020_tenant_providers.sql` | EDIT (in place) | Update file-top comment: `mode ∈ {platform, byok}` → `mode ∈ {platform, self_managed}`. The `mode` column is already `TEXT` (RULE STS — value enforcement lives in `src/state/tenant_provider.zig`), so no DDL change. Update the operator-query index comment from "list all BYOK tenants" to "list all self-managed tenants". |
| `src/state/tenant_billing.zig` | EDIT | Constants renamed `_CENTS` → `_NANOS`, value set rebuilt from M66 rate table (not a value-preserving rescale). Drop `EVENT_BYOK_CENTS` and `EVENT_PLATFORM_CENTS` (collapsed into single `EVENT_NANOS = 0`). Drop single `STAGE_CENTS`; add posture-dispatched `STAGE_PLATFORM_NANOS = 1_000_000` and `STAGE_SELF_MANAGED_NANOS = 100_000`. `STARTER_CREDIT_NANOS = 5_000_000_000`. Column type stays `BIGINT` (i64). |
| `src/state/tenant_billing_test.zig` | EDIT | Pin tests for all new constants. New negative test: parsing legacy `'byok'` string into `Mode` returns `error.UnknownMode`. |
| `src/state/tenant_provider.zig` | EDIT | `Mode.byok` → `Mode.self_managed`. Update `displayName()`, `parse()`, every match arm. |
| `src/http/handlers/tenant_provider.zig` | EDIT | Accept only `mode: "self_managed"` (or `"platform"`); any other value falls through the generic `ERR_INVALID_REQUEST` path (HTTP 400, `UZ-REQ-001`, body message `"mode must be 'platform' or 'self_managed'"`). No special-case `UZ-PROVIDER-MODE-RENAMED` branch — pre-v2.0 RULE NLG ruled out legacy-aware scaffolding (Session Notes #2). |
| `src/zombie/executor.zig` | EDIT | Posture matching arms. |
| `src/observability/scoped.zig` (or wherever scopes are declared) | EDIT | Log scope rename `byok_credential_*` → `self_managed_credential_*`. |
| `src/errors/error_registry.zig` | NO EDIT (drift from earlier draft) | Earlier draft planned a `UZ-PROVIDER-MODE-RENAMED` registry entry. RULE NLG ruled it out: byok is rejected via the existing generic `UZ-REQ-001` path, not a dedicated retired-value-aware code. |
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

### §1 — Nanos unit (clean break, pre-v2.0) — DONE (cbb23fac + 1cd35544)

Switch the canonical billing unit from cents (`i64`) to **nanos (1/1,000,000,000 USD = 9 decimal places)**. Pre-v2.0 RULE NLG + Schema Removal Guard pre-v2.0 path → **edit the existing schema file in place; no migration script; no `ALTER`.**

**Procedure:**
1. Edit `schema/017_tenant_billing.sql`: rename column declaration `balance_cents BIGINT NOT NULL CHECK (balance_cents >= 0)` → `balance_nanos BIGINT NOT NULL CHECK (balance_nanos >= 0)`. Update the file-top comment to drop M65 cents prose.
2. Reseed dev DB: `make down && make up` (this re-runs every `schema/*.sql` from a clean Postgres). The Schema Removal Guard pre-v2.0 path is the canonical procedure here — see `docs/gates/schema-removal.md`.
3. Update Zig constants in `src/state/tenant_billing.zig`: rename `_CENTS` → `_NANOS`, rebuild values from the M66 rate table. Update every reference to `balance_cents` across `src/` to `balance_nanos`.
4. Update paired pin test in `src/state/tenant_billing_test.zig`.
5. Update TS mirror `ui/packages/website/src/lib/rates.ts` per the cross-tier naming table.

**Why nanos, not micros:** micros (1/1,000,000 USD = 6 decimals) bottom out at $0.000001. Nanos give 3 more decimals of headroom, enough to express $0.000000001 (one billionth) cleanly. `i64` BIGINT holding nanos caps a single tenant balance at ~$9.2 billion (`i64::MAX` = 9.22e18 nanos = ~9.22e9 USD), so the type has no realistic overflow risk.

**Why no `ALTER`:** pre-v2.0 has no production data to preserve. Adding a migration script just to rescale a column on an empty dev DB would (a) need maintenance for the lifetime of the repo, (b) couple the rate change to migration plumbing that doesn't exist post-v2.0 either (we'd build it once and never use it), and (c) violate RULE NLG by carrying legacy framing into pre-v2.0 code. Edit the schema file, reseed, move on. When v2.0.0 ships, the Schema Removal Guard switches to the migration-required path automatically.

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

### §2 — M66 traction rates — DONE (cbb23fac)

Replace the M65 rate constants with M66 values. New constant set:
- `STARTER_CREDIT_NANOS = 5_000_000_000` ($5)
- `EVENT_NANOS = 0` (single value, both postures)
- `STAGE_PLATFORM_NANOS = 1_000_000` ($0.001 — usezombie pays Fireworks for tokens)
- `STAGE_SELF_MANAGED_NANOS = 100_000` ($0.0001 — user pays their own provider)

Drop `EVENT_PLATFORM_CENTS`, `EVENT_BYOK_CENTS`, `STAGE_CENTS` (the single cross-mode constant). Update `compute_*_charge` functions in `tenant_billing.zig` to dispatch on posture for stage cost.

The platform/self-managed gradient is the friction-reducer: on-ramp on platform mode without bringing a key; graduate to self-managed for 10× cheaper stages once the user is serious. Document this in the `<Update>` block as a deliberate stealth-mode subsidy.

### §3 — BYOK term retirement (every tier) — DONE (cbb23fac + d5d5b6ad)

Rename in lockstep across schema, Zig, API wire format, TS, app components, CLI, architecture docs.

**Implementation default:** use `git mv` for file renames to preserve history. **No DDL change for the `mode` column** — `core.tenant_providers.mode` is `TEXT` with app-side enforcement per RULE STS (see `schema/020_tenant_providers.sql:12-14` comment). Value-set drift lives entirely in `src/state/tenant_provider.zig`'s `Mode` enum + `parse()` function. The schema file's only edit is the prose comment switch from `byok` → `self_managed`. Dev DB reseed via `make down && make up`.

**Clean break — no alias:** the API rejects `mode: "byok"` with HTTP 400 via the existing generic `UZ-REQ-001` (`ERR_INVALID_REQUEST`) path, body message `"mode must be 'platform' or 'self_managed'"`. The CLI never accepted a `--byok` flag in this milestone's CLI shape (the verb is `tenant provider add --credential <name>`, posture is implicit on `add`). Pre-v2.0 RULE NLG forbids legacy scaffolding — including a dedicated retired-value error code that would only exist to soften the break. The generic fall-through is the break.

### §4 — Website pricing surface fix — DONE (9bfcac54)

Update `Pricing.tsx`, `FAQ.tsx`, `lib/rates.ts` to surface the new rates. Pricing card displays two stage rates side-by-side with the friction-reduction framing ("$0.001/stage on platform default, $0.0001/stage when you bring your own provider key — 10× cheaper to scale"). Drop the BYOK provider-list paragraph; the diagram below already names providers. Stealth-mode banner lands in the Pricing card itself (already shipped via PR #311; this section keeps it consistent with the new rates).

**Introductory-rate framing:** the rate line carries a small subscript "stealth-mode testing rate — will rise post-GA" so future ratchets are expected behavior, not surprise price hikes.

### §5 — Single canonical SUPPORT_EMAIL per repo — DONE (32001911)

Five new constant files (one per repo / package) all asserting `usezombie@agentmail.to`:
- `src/config/contact.zig`
- `ui/packages/website/src/lib/contact.ts`
- `ui/packages/app/lib/contact.ts`
- `zombiectl/src/lib/contact.js`
- `~/Projects/docs/snippets/contact.mdx`

Paired pin test in each repo asserts the exact string. Sweep replaces every `hello@usezombie.com` literal with the constant import. The org-profile README (`~/Projects/.github/profile/README.md`) carries a single literal (markdown can't import).

### §6 — Documentation currency audit — DONE (a9be55ed)

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

`mode: "byok"` (or any value other than `platform` / `self_managed`) returns the generic invalid-request response:
```
HTTP/1.1 400 Bad Request
Content-Type: application/problem+json

{
  "type": "https://docs.usezombie.com/errors/UZ-REQ-001",
  "title": "Invalid request",
  "status": 400,
  "code": "UZ-REQ-001",
  "detail": "mode must be 'platform' or 'self_managed'"
}
```

Earlier drafts of this spec called for a dedicated `UZ-PROVIDER-MODE-RENAMED` code carrying a `replacement` field hint. Pre-v2.0 RULE NLG (Session Notes #2) ruled it out — a retired-value-aware code is itself legacy scaffolding, and the generic fall-through is the clean break.

### CLI — `zombiectl tenant provider add`

**Before (M65 draft):** the spec's earlier draft assumed a `--byok` flag on a `tenant provider set` verb. Neither existed in the shipped CLI; the verb is `tenant provider add --credential <name>` and the mode is implicit on the `add` action (`PROVIDER_MODE.self_managed`). No flag was renamed; the spec drift is documented in Session Notes #2.
**After (M66):** unchanged. `zombiectl tenant provider add --credential <name>` is the supported surface.

### Schema — `core.tenant_providers.mode` and `billing.tenant_billing`

**Before:** `core.tenant_providers.mode TEXT` with comment-documented value-set `{platform, byok}`; value enforcement in `src/state/tenant_provider.zig` (RULE STS — no static-string CHECKs). `billing.tenant_billing.balance_cents BIGINT NOT NULL CHECK (balance_cents >= 0)`.

**After:** `core.tenant_providers.mode TEXT` with comment-documented value-set `{platform, self_managed}`; value enforcement still in `Mode.parse()` — schema-side change is comment-only. `billing.tenant_billing.balance_nanos BIGINT NOT NULL CHECK (balance_nanos >= 0)`.

**Procedure:** in-place edit of `schema/017_tenant_billing.sql` and `schema/020_tenant_providers.sql` per §1's pre-v2.0 clean-break path. **No migration script, no `ALTER`, no rescaling step.** Dev DB is reseeded via `make down && make up`; CI starts from clean Postgres so the schema change is invisible there. See `docs/gates/schema-removal.md` pre-v2.0 path.

### TS — `Mode` type alias

**Before:** `type Mode = "platform" | "byok"`.
**After:** `type Mode = "platform" | "self_managed"`.

---

## Failure Modes

| Mode | Cause | Handling | Test |
|---|---|---|---|
| Old client sends `mode: "byok"` | Pre-M66 SDK / cached client / curl from a runbook | HTTP 400 + `UZ-REQ-001` generic invalid-request, body says `mode must be 'platform' or 'self_managed'`. No alias, no dedicated retired-value code (per RULE NLG; see Session Notes #2). | Mode-parse rejection covered by `tenant_provider_test.zig` Mode-enum tests + the HTTP handler's branch coverage in the integration suite. |
| TS Mode parser receives `"byok"` from API | Stale client cache / network race during cutover | Strict TypeScript union → parse error → toast. No silent coerce. | `provider-selector.test.ts` PROVIDER_MODE constants. |
| CLI --byok flag passed | N/A — the flag never existed in this milestone's CLI | The CLI verb is `tenant provider add --credential <name>` with implicit `self_managed` posture; there is no `--byok` flag to reject. Earlier spec draft assumed otherwise. | n/a — spec drift documented in Session Notes #2. |
| Dev DB still holds pre-M66 column shape after pulling this branch | Developer didn't reseed | Pre-v2.0 procedure: `make down && make up` after merge. Schema files are the source of truth; no migration runner. CI starts from clean Postgres so this only affects local dev. | n/a — documented in PR Session Notes; no test (clean-slate is the contract) |
| Saved Grafana dashboards keyed on `byok_credential_*` log scopes | Log scope rename | Listed in Discovery; saved-search update is a follow-up doc-only change. Not blocking. | n/a — operational |
| Architecture-doc intra-doc links broken after `git mv` | `[link](billing_and_byok.md)` references in peer docs | DOC READ GATE on `docs/architecture/**` catches unfixed links during execute; sweep listed in §3. | `test_arch_doc_links_resolve` (grep-based) |
| Frontend `data-testid` rename breaks downstream e2e | Out-of-repo e2e suite depends on `provider-byok-*` IDs | Inventory all references during Discovery; flag any out-of-repo dependency in spec body before CHORE(open). | `test_provider_selector_test_ids_renamed` |
| Pin tests drift between Zig and TS rates | Cross-tier mirror not enforced | Paired pin tests in `tenant_billing_test.zig` and `rates.test.ts` assert the exact same values. CI fails on drift. | `test_rates_pinned_zig` + `test_rates_pinned_ts` |

---

## Invariants

1. **No surface mentions BYOK except named exemptions.** Enforced by `make lint` step that greps `\bBYOK\b` against `src/`, `ui/`, `zombiectl/`, `public/`, `docs/architecture/` and asserts zero hits. Allowlist file lists the architecture-doc historical-note line and the docs-site changelog archive.
2. **Both rate constants are pinned identically across Zig + TS.** Enforced by paired test asserting exact i64 values match.
3. **`SUPPORT_EMAIL` is `usezombie@agentmail.to` everywhere.** Enforced by per-repo pin test asserting the exact string + `make lint` step grepping for `hello@usezombie.com` and asserting zero hits.
4. **`mode` value-set is exactly `{platform, self_managed}` and lives in app code.** `core.tenant_providers.mode` stays `TEXT` (RULE STS — no static-string CHECKs). Enforced by string comparison in `src/http/handlers/tenant_provider.zig` (the JSON body's `mode` is matched against `"platform"` / `"self_managed"`; anything else falls through to `ERR_INVALID_REQUEST`). The Zig-side `Mode` enum holds the two variants for resolved-state representation, not for input parsing.
5. **Input strings other than `"platform"` / `"self_managed"` return 400 via the generic invalid-request path.** Enforced by the HTTP handler's branch coverage. There is no dedicated `Mode.parse()` helper; the handler does the string comparison inline and reaches `ERR_INVALID_REQUEST` on any non-match.
6. **API responses always serialize `mode` as `"platform"` or `"self_managed"` — never `"byok"`.** Enforced by the OpenAPI enum (`["platform","self_managed"]` at three sites in `public/openapi.json` and `public/openapi/paths/tenant-provider.yaml`).

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_starter_credit_nanos_pinned` | `STARTER_CREDIT_NANOS == 5_000_000_000` |
| `test_event_nanos_zero_both_postures` | `compute_event_charge(.platform) == 0`, `compute_event_charge(.self_managed) == 0` |
| `test_stage_platform_nanos_pinned` | `STAGE_PLATFORM_NANOS == 1_000_000` |
| `test_stage_self_managed_nanos_pinned` | `STAGE_SELF_MANAGED_NANOS == 100_000` |
| `test_compute_stage_charge_dispatches_on_posture` | `.platform` → 1_000_000 nanos, `.self_managed` → 100_000 nanos |
| ~~`test_mode_parse_self_managed_succeeds`~~ | Superseded — no `Mode.parse()` helper. Equivalent coverage: `tenant_provider_test.zig` "Mode label round-trips for both variants" + `resolveActiveProvider with self_managed row` (state-layer happy path). |
| ~~`test_mode_parse_byok_fails`~~ | Superseded — rejection happens at the HTTP layer's string comparison, not via a `Mode.parse` helper. Covered by the handler's branch coverage in the integration suite. |
| `test_schema_balance_nanos_column_exists` | Integration test: after `make up` from clean, `\d billing.tenant_billing` shows column `balance_nanos BIGINT NOT NULL` (and zero columns named `balance_cents`). |
| `test_schema_no_byok_in_comments` | grep `\bbyok\b` in `schema/*.sql` returns 0 hits. |
| ~~`test_provider_mode_byok_returns_400_with_replacement`~~ | Superseded — implementation took the generic `UZ-REQ-001` fall-through path. Wire behavior: 400 + `code=UZ-REQ-001` + detail `"mode must be 'platform' or 'self_managed'"`. RULE NLG ruled out the special-case retired-value-aware code (Session Notes #2). |
| `test_provider_mode_self_managed_accepted` | Same endpoint with `{"mode":"self_managed","credential_ref":"my-key"}` → 200 |
| `test_provider_mode_response_never_emits_byok` | `GET /v1/tenants/me/provider` after a `self_managed` set → response.mode === "self_managed" |
| ~~`test_cli_legacy_byok_flag_emits_error`~~ | Superseded — the `--byok` flag never existed in this milestone's CLI shape; the verb is `tenant provider add --credential <name>`. Spec drift documented in Session Notes #2. |
| `test_cli_self_managed_add_works` | `zombiectl tenant provider add --credential <name>` → exit 0, PUT body `{ mode: "self_managed", credential_ref: "<name>" }`. Covered by `zombiectl/test/tenant_provider.unit.test.js` (tests at L67–L95). |
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
- [ ] Schema reseed yields the new column shape — verify: `make down && make up && psql "$DATABASE_URL" -c "\d billing.tenant_billing" | grep -E '^ balance_(cents|nanos)\s'` shows `balance_nanos` (and zero `balance_cents` rows). No migration runner is invoked — pre-v2.0 clean-break path per §1.
- [ ] No `byok` literal in `schema/*.sql` — verify: `grep -rn '\bbyok\b' schema/ | wc -l` → 0
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

# E10: Schema mode value-set check (mode is TEXT — value enforcement lives in Mode.parse())
psql "$DATABASE_URL" -c "\d core.tenant_providers" | grep -E '^ mode\s+\| text\b'
echo "E10a: should print one row showing 'mode | text | not null'"
zig build test 2>&1 | grep -E 'test_mode_parse_(self_managed_succeeds|byok_fails)'
echo "E10b: both Mode.parse pin tests should be PASS"
grep -c '\bbyok\b' schema/*.sql
echo "E10c: should print 0"

# E11: API value rename — generic fall-through, no dedicated retired-value code
curl -s -X PUT http://localhost:9090/v1/tenants/me/provider \
  -H "Content-Type: application/json" \
  -d '{"mode":"byok","credential_ref":"x"}' | jq -r '.code'
echo "E11: should print UZ-REQ-001 (generic ERR_INVALID_REQUEST; RULE NLG ruled out a dedicated retired-value code)"

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

**§1 scope expansion (logged at EXECUTE entry, May 10):** the original Files-Changed table listed only `schema/017_tenant_billing.sql` for the cents → nanos unit change. Grep across `schema/` and `src/` surfaced two additional cents-typed schemas that must flip in the same commit to keep "canonical billing unit = nanos" honest:

- `schema/014_zombie_execution_telemetry.sql` — `credit_deducted_cents BIGINT` → `credit_deducted_nanos BIGINT`. Consumed by `src/state/zombie_telemetry_store.zig` + tests + `src/zombie/metering.zig`.
- `schema/019_model_caps.sql` — `input_cents_per_mtok INTEGER`, `output_cents_per_mtok INTEGER` → `input_nanos_per_mtok BIGINT`, `output_nanos_per_mtok BIGINT`. Type widens INTEGER → BIGINT because `$30/M tokens` in nanos = `3e10`, beyond `INT32_MAX` (~2.1e9). Consumed by `src/state/model_rate_cache.zig`, `src/http/handlers/model_caps.zig`, the model_caps integration test, and `src/state/tenant_billing.zig::computeStageCharge`.

Without this expansion, `STAGE_PLATFORM_NANOS + in_cents + out_cents` would mix nanos and cents and produce nonsense charges. Spec body's Files-Changed table is amended in the same commit so the spec stays the source of truth.

Expected further entries:
- Inventory of `provider-byok-*` data-testid usage (out-of-repo e2e dependency check) before §3 begins.
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

| Check | Command | Result | Pass? |
|---|---|---|---|
| Unit tests (Zig) | `make test` | 29/29 + skill evals green | ✅ |
| Unit tests (website) | `cd ui/packages/website && bun run test` | 129/129 (18 files) | ✅ |
| Unit tests (app) | `cd ui/packages/app && bun run test` | 357/357 (34 files) | ✅ |
| Unit tests (zombiectl) | `cd zombiectl && bun test` | 567/567 (57 files) | ✅ |
| Integration tests | `make test-integration` | full suite passed | ✅ |
| Lint | `make lint` | all green (Zig, eslint, openapi bundle + schemas + url shape) | ✅ |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | both targets green | ✅ |
| Gitleaks | `gitleaks detect` | 1715 commits scanned, no leaks | ✅ |
| UFS audit | `bash scripts/audit-ufs.sh --diff` | 4 violations, all baseline / by-design (CHARGE_TYPE / PROVIDER_MODE / SELF_MANAGED_SENTINELS are Zig enums vs JS SCREAMING_SNAKE wrappers; RATES_DISPLAY is presentation-only) | ⚠ — see Session Notes |
| BYOK term sweep | `grep -rn '\bBYOK\b' src/ ui/ zombiectl/ public/ docs/architecture/` | 1 hit — `Pricing.test.tsx` negative assertion verifying BYOK is absent from the rate card (intentional) | ✅ (assertion only) |
| `hello@usezombie.com` sweep | `grep -rn 'hello@usezombie\.com' src/ ui/ zombiectl/ docs/ public/` | 0 hits in source/copy; remaining hits are in this spec body + the deleted handoff doc | ✅ |
| Schema `mode` column shape (TEXT, not enum) | `grep -nE 'mode TEXT NOT NULL' schema/020_tenant_providers.sql` + `grep -c '\bbyok\b' schema/*.sql` | TEXT confirmed; 0 byok hits in schema | ✅ |
| OpenAPI enum | `jq '.components.schemas.TenantProviderMode.enum' public/openapi.json` | `["platform","self_managed"]` | ✅ |

---

## Out of Scope

- **TEMPLATE.md determinism upgrades** (the seven-point list from the M66 proposal). Filed as future work; orthogonal to BYOK / rates / email.
- **Grafana saved-search rename** for `byok_credential_*` log scopes. Listed in §6 Discovery; will be a follow-up doc-only branch updating saved queries, not blocking on this spec.
- **Schema column rename `posture`/`mode` if applicable.** Existing column name preserved; only the enum value changes.
- **Public docs publishing of `UZ-PROVIDER-MODE-RENAMED` error code page on docs.usezombie.com.** Error registry update lands in this spec; the docs page lands in the paired docs PR's `<Update>` block.
- **Currency / locale display formatting.** All rates render in USD; localization is future work.
- **Volume-tier pricing or post-GA ratchet schedule.** Captured as a marketing/strategy decision; not in this spec.

---

## Session Notes (CHORE close)

**Branch:** `feat/m66-001-byok-retirement` · **Commits:** `3db21927` → `a9be55ed` (10 commits, single workstream).

**Decisions made during EXECUTE (cross-references to Discovery):**

1. **§1 scope expansion to three schemas.** Original Files-Changed table listed only `schema/017_tenant_billing.sql`; grep surfaced `schema/014_zombie_execution_telemetry.sql` (`credit_deducted_cents` → `credit_deducted_nanos`) and `schema/019_model_caps.sql` (`input_cents_per_mtok`/`output_cents_per_mtok` → `input_nanos_per_mtok`/`output_nanos_per_mtok`, with type widen INTEGER → BIGINT because `$30/M tokens` in nanos overflows i32). Logged in commit `e9f4621a`.
2. **No special-case retired-mode branch.** Initial §3 implementation had an `if (input.mode == "byok")` branch returning `UZ-PROVIDER-MODE-RENAMED`. Per RULE NLG pre-v2.0 (no legacy retention) the special-case branch was removed in the §3 tail; `mode: "byok"` now flows through the generic mode-not-recognized fall-through with a "mode must be one of: platform, self_managed" message. `UZ-PROVIDER-005` registry entry retired.
3. **§4 pricing redesign — fix install button stretch.** Captain flagged the install CTA looked stretched in the rendered Pricing card. Root cause: `flex flex-col` Card stretches inline-flex Buttons. Fix: `self-start` on the Button (mirrors the existing `<Badge className="self-start">` precedent two children up).
4. **§4 BILLED_FLOW realism.** Captain asked for concrete platform-ops cells instead of abstract "stage 1: reason · act". Refactored to: event = "deploy webhook fires", stage 1 = "read CI logs", stage 2 = "correlate commits", stage N = "post Slack diagnosis" — mirrors the install transcript on Hero.tsx.
5. **§5 cross-tier `SUPPORT_EMAIL`.** Created per-repo named constants in Zig + 2× TS + JS plus the paired Mintlify snippet. Swept two `support@usezombie.com` literals from the dashboard (`BillingBalanceCard`, `ExhaustionBanner`) and three `usezombie@agentmail.to` literals from the website (`Pricing.tsx`, `Privacy.tsx`, `Terms.tsx`) into the constant. The org-profile README literal is kept per Captain's "skip .github/profile" decision.
6. **§5 also fixed `ui/packages/app/package.json` test glob.** vitest's `lib/**/*.test.ts` pattern was silently skipping `lib/contact.test.ts` (no intermediate directory under `lib/`); script expanded to `lib/*.test.ts lib/**/*.test.ts`.
7. **§6 architecture-doc depinning.** Captain's directive: "rate constants will keep changing; the docs shouldn't have to follow each ratchet." `docs/architecture/billing_and_provider_keys.md` rewrote shape-first — function signatures and constant *names* live in the doc, *values* live behind three authoritative sources (`tenant_billing.zig`, `snippets/rates.mdx`, the `model-caps.json` endpoint). Scenario walk-throughs (01_default_install, 03_balance_gate) got mechanical column-name fixes + "Rate snapshot" banners pointing readers at the canonical doc; the cent-by-cent arithmetic is preserved as instructional narrative rather than rewritten for the current rate table.
8. **§6 broken cross-reference fix in pending spec.** `docs/v2/pending/M50_001_*.md` L26 cited "OSS + BYOK + markdown-defined" as the architecture's three pillars; `high_level.md` now reads "open source + self-managed provider keys + markdown-defined". Fixed inline so M50_001 reads against the current architecture state when it lands.

**Assumptions surfaced and confirmed:**

- **Sealed history stays.** `docs/v2/done/M48_001_*` keeps `STARTER_GRANT_CENTS = 1000`, `RECEIVE_PLATFORM_CENTS = 1`, etc. — those describe what shipped at that time. Same for M48-era changelog entries.
- **Display strings ≠ domain constants.** `RATES_DISPLAY` map (TS) and `STAGE_PLATFORM`/`STAGE_SELF_MANAGED` (Mintlify) are presentation-layer; they don't get Zig mirrors. The cross-tier parity rule applies to domain integers (`STARTER_CREDIT_NANOS`, etc.), not their `$`-formatted display siblings.
- **Mintlify changelog snippets are forward-only.** The May 9 (M65) entry imported `EVENT_RATE` / `STAGE_RATE` from `rates.mdx`; M66 rewrote those exports. To preserve the M65 entry's meaning, hardcoded `EVENT_RATE_M65 = "$0.01"` and `STAGE_RATE_M65 = "$0.10"` placeholders were defined at the import header and substituted only in the May 9 entry. Future changelog entries should hardcode their rate values inline rather than rely on snippet imports if they describe a state of the world that pre-dates the current rate table.

**Dead ends / discarded approaches:**

- **Adding `pub const RATES_DISPLAY` in Zig.** Considered as a way to clear the UFS-gate `RATES_DISPLAY absent-in-zig` violation. Rejected — Zig isn't a presentation runtime; the const would be dead code (NLR violation) and the cross-tier rule applies to domain values, not display strings.
- **Filing `docs/architecture/billing_and_provider_keys.md` cent-numerics fix as a follow-up spec.** Initial plan was to file a follow-up. Switched to inline after Captain's "fix all drift inline in this PR" decision, then again to "depin from concrete values entirely" after Captain's mid-§6 guidance.

**UFS-gate violations remaining (4):** all by design / baseline.

| Violation | Why it persists |
|---|---|
| `CHARGE_TYPE absent-in-zig` | Zig holds this as enum `ChargeType` (PascalCase); JS exports SCREAMING_SNAKE wrapper. Audit's regex matches `pub const NAME`, not enum types. Pre-existing baseline since d5d5b6ad. |
| `PROVIDER_MODE absent-in-zig` | Same shape: Zig `enum Mode`, JS `PROVIDER_MODE` SCREAMING_SNAKE. Pre-existing baseline. |
| `SELF_MANAGED_SENTINELS absent-in-zig` | JS-only test fixture for the install-skill frontmatter substitution; no Zig analog by design. Pre-existing baseline. |
| `RATES_DISPLAY absent-in-zig` | TS/JS display-strings map; no Zig analog by design (Zig isn't a presentation runtime). Introduced in §4; flagged as out-of-scope baseline rather than fixed. |

**`/write-unit-test` outcome:** TBD — runs as the first step of CHORE(close) after this commit lands.
**`/review` outcome:** TBD — runs after `/write-unit-test`.
**`/review-pr` outcome:** TBD — runs after `gh pr create`.
**`kishore-babysit-prs` outcome:** TBD — runs after every push.

**Companion docs PR:** `usezombie/docs#feat/m66-001-byok-retirement-docs` (commit `11290fe`). Branch pushed; PR opens after this lead PR is up so the docs side cannot diverge.

**Pre-push integration suite:** 1508/0 locally and on the pre-push hook each iteration. No state-pollution flake seen this session (Gotcha 13 from the resume handoff didn't recur).
