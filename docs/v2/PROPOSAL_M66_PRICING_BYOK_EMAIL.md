# Proposal — M66 traction pricing + BYOK retirement + email standardization

**Date:** 2026-05-10
**Status:** PROPOSAL — awaiting Captain decision before any spec is authored.
**Scope:** cross-repo (`usezombie/`, `~/Projects/docs/`, `~/Projects/.github/profile/`).

This is a proposal, not a spec. Once approved (or shredded) I'll author the spec(s) via `kishore-spec-new` and execute under the normal lifecycle.

---

## TL;DR — what this proposal changes

1. **Pricing → traction shape.** Both event rates → **$0.000**. Stage rate **$0.10 → $0.001** (one-tenth of a cent). Starter credit stays **$5**, granted to every new tenant (no special "design partner" carve-out). Hides the rate-pricing entirely below the noise floor for early adopters; we ratchet up post-PMF.
2. **BYOK retirement (user-facing only).** Strip "BYOK" from every surface a user reads — website, dashboard, docs, CLI help, FAQ. Internal identifiers (`Mode.byok` enum, `mode: "byok"` API value, `posture` column, `core.tenant_providers.mode`) keep `byok` until v2.0 schema break. The split announced in M65 was the half-step; this finishes it.
3. **One support email everywhere.** Replace every literal `hello@usezombie.com` with `usezombie@agentmail.to` and route through a single shared constant in each repo. Privacy/Terms pages already use the `agentmail.to` address — Pricing.tsx and the docs are the remaining outliers.

---

## Captain decisions needed (please answer before spec authoring)

### D1. Cents → mills unit, or batched billing?

`$0.001` doesn't fit cleanly in `i64 cents`. Two viable approaches:

| Option | What it means | Cost | Reversibility |
|---|---|---|---|
| **A. Switch unit to mills** (1 mill = $0.001) | `balance_cents` → `balance_mills` (×1000), all rate constants × 1000, schema migration, API field rename | ~1 day Zig + Postgres migration + paired TS pin tests + docs/snippet update | High (we're pre-v2.0; rule NLG ban-on-legacy applies) |
| **B. Keep cents, batch stages** | Charge `$0.01 per 10 stages` (1 cent debit every 10th stage). Stage counter in `core.zombie_sessions`. Display copy still reads "$0.001 per stage" | ~½ day, no schema migration | Medium (counter logic survives unit change later) |

**Recommend A.** We're pre-v2.0; this is the cheap moment. Mills give us 3 decimal places of precision forever and let us drop $0.0001 micro-rates in the future without another migration. Option B has hidden footguns around partial-batch refunds on cancelled events.

### D2. BYOK term scope — exactly how aggressive?

| Tier | What gets renamed | Examples |
|---|---|---|
| **User-facing prose** (must) | All copy a user reads | "BYOK on Anthropic..." → "Pick the model and pay your provider directly." Footer "BYOK" badge → drop. FAQ keys with `byok` in the answer body. CLI `--help` strings. |
| **User-facing identifiers** (should) | Component names, test IDs, dashboard tab labels | `ByokFields.tsx` → `ProviderKeyFields.tsx`. `ProviderSelector` "Switch to BYOK" button → "Use my own provider key". `BillingUsageTab` BYOK badge → "your provider". |
| **API surface** (decide) | `mode: "byok"` request/response value, `?mode=byok` query | Either keep (internal identifier; not user-facing in dashboards) or rename to `mode: "self_managed"` with a one-cycle alias |
| **Schema column** (no — pre-v2.0 floor) | `core.tenant_providers.mode` enum value `'byok'` | Keep. Pre-v2.0 rule NLG: no schema-rename without functional reason. |
| **Architecture docs** (no) | `docs/architecture/billing_and_byok.md`, `scenarios/02_byok.md` | Keep. These describe the engine's two postures; the term is correct internally. Add a §0 cross-link saying "user surfaces no longer expose this term." |

**Recommend the first three tiers** (user-facing prose, user-facing identifiers, **and** API surface — rename `mode: "byok"` → `mode: "self_managed"` with 1-cycle alias accepted on input, only `self_managed` returned). Keeps internal/schema layer untouched, kills the term in everything a developer sees.

### D3. The "design partners run free" copy block

Currently `Pricing.tsx:49-62` reads:

> Early-access design partners run free — every charge waived while we calibrate the model with you. Email hello@usezombie.com to enroll.

You said this was misread — actual intent is "everyone (design partners and users) gets the $5 starter credit." Two options:

| Option | Copy |
|---|---|
| **A. Drop the block entirely** | Pricing card just shows the rate + $5 starter badge. |
| **B. Reframe as "design-partner program is open"** | "Design-partner program is open. Email usezombie@agentmail.to if you want a hand calibrating your zombie." (No charge waiver — they get the same $5 starter as everyone else. The value of being a design partner is product attention, not free credits.) |

**Recommend B.** Captures the real ask (we want depth-of-engagement design partners) without misframing the credit policy.

---

## Per-surface proposal

### 1. `usezombie` repo (server, worker, canonical constants, arch doc)

**Files:**

| File | Change |
|---|---|
| `src/state/tenant_billing.zig` | `STARTER_CREDIT_CENTS: 500` → `STARTER_CREDIT_MILLS: 5000`. `EVENT_PLATFORM_CENTS: 1` → `EVENT_PLATFORM_MILLS: 0`. `EVENT_BYOK_CENTS: 0` → `EVENT_SELF_MANAGED_MILLS: 0`. `STAGE_CENTS: 10` → `STAGE_MILLS: 1`. (Assumes D1=A.) |
| `schema/006_tenant_billing.sql` (or wherever `balance_cents` lives) | New migration: `balance_cents` → `balance_mills`, multiply existing balances ×1000. Per pre-v2.0 schema-removal rule: this is an evolution, not a teardown. |
| `src/state/tenant_billing_test.zig` | "rates pinned" test asserts the four mill values; add a "mills > cents migration" test multiplying ×1000 cleanly. |
| `src/http/handlers/tenant_provider.zig` (or wherever) | Accept `mode: "byok"` and `mode: "self_managed"` on input (one-cycle alias); return only `self_managed` (assumes D2 = first three tiers). |
| `src/state/tenant_provider.zig` enum | Keep variant `byok` (internal identifier per D2 floor); add a `displayName()` returning `"self_managed"` for serialization. |
| `docs/architecture/billing_and_byok.md` | Add §0 vocabulary preamble: "Internal identifier remains `byok` for the schema/enum/log scope. User-facing surfaces use 'self-managed provider key' or 'pick your own provider'. Architecture docs and runtime logs keep BYOK because the engine has two postures and the term is technically accurate at that layer." (Note: this is the inverse of M65 §0 — M65 said the enum keeps BYOK; M66 says user surfaces strip it entirely.) |
| `README.md` (repo root) | "Markdown-defined. BYOK." → "Markdown-defined. Pick your own model provider." Pricing badge unchanged ($5 starter still correct). |
| `tenant_billing.zig` & paired TS test | Drop `EVENT_BYOK_*` distinct constant; both event rates collapse to one `EVENT_MILLS: 0` (since both are zero). Posture still differentiates by stage cost? No — stage rate is uniform across postures already. Net result: posture stops being a billing distinction at all. **Side-effect: this proposal collapses the two-rate-table into one rate table.** Worth flagging — once both events are $0 and stage is uniform, posture only differentiates which vault holds the inference key, not what gets billed. |

**Single-source email:** add `pub const SUPPORT_EMAIL: []const u8 = "usezombie@agentmail.to";` in `src/config/contact.zig` (new file). Reference it from any handler that surfaces a contact email (currently none, but future-proof).

### 2. `website/` (`ui/packages/website/`)

**Files & changes:**

| File | Change |
|---|---|
| `src/lib/rates.ts` | `RATES_CENTS` → `RATES_MILLS` (or keep cents and divide for display — see D1). `RATES_DISPLAY.eventPlatform`/`.eventByok` → single `RATES_DISPLAY.event = "$0.000"` (or drop the event row entirely if free). `RATES_DISPLAY.stage = "$0.001"`. `WORKED_EXAMPLE.total` recomputes from constants. |
| `src/lib/contact.ts` (new) | `export const SUPPORT_EMAIL = "usezombie@agentmail.to";` |
| `src/components/Pricing.tsx` | Drop `BILLED_FLOW` event cell (or label price as "free"). Headline: "$0.001 per stage execution · events free". Drop "BYOK on Anthropic, OpenAI, ..." paragraph; replace with "Pick your model provider — Anthropic, OpenAI, Fireworks, Together, Groq, Moonshot — and pay them for tokens directly. We don't mark up inference." Replace `hello@usezombie.com` → `{SUPPORT_EMAIL}`. Reframe design-partner block per D3. Replace "BYOK" hardcoded text from `Footer.tsx` import. |
| `src/components/Pricing.test.tsx` | Update worked-example math, drop BYOK assertions, swap email assertion. |
| `src/components/FAQ.tsx` | Rewrite the three answers that mention BYOK. Reuse the "pick your provider" framing. |
| `src/components/FAQ.test.tsx` | Strip BYOK assertions; assert "pick your provider" / "self-managed" copy. |
| `src/components/Footer.tsx` | Drop BYOK badge or rephrase to "self-managed provider keys". |
| `src/components/FeatureFlow.tsx` | Replace any BYOK label. |
| `src/pages/Home.tsx` | One mention to drop/rephrase. |
| `src/pages/Privacy.tsx` / `Terms.tsx` | Already use `usezombie@agentmail.to`. Switch to `{SUPPORT_EMAIL}` import for consistency. Drop or rephrase BYOK references in the body copy where present. |

### 3. `app/` (`ui/packages/app/`)

**Files & changes:**

| File | Change |
|---|---|
| `app/(dashboard)/settings/provider/components/ByokFields.tsx` | Rename file → `ProviderKeyFields.tsx`. Component export rename. All callers updated. |
| `app/(dashboard)/settings/provider/components/ProviderSelector.tsx` | "Switch to BYOK" button → "Use my own provider key". Tab label "BYOK" → "Self-managed". Test IDs `provider-byok-*` → `provider-self-managed-*` (breaking for any downstream e2e — flag in PR notes). |
| `app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` | Badge "BYOK" → "Self-managed". Tooltip copy update. |
| `lib/types.ts` | Type alias `Mode = "platform" | "byok"` → `Mode = "platform" | "self_managed"` with a `// alias accepted: "byok" still parsed for one cycle` comment. |
| `lib/api/tenant_provider.ts` | If we ship D2's API alias, the client sends `self_managed`; helper accepts either on parse. |
| `lib/contact.ts` (new) | `export const SUPPORT_EMAIL = "usezombie@agentmail.to";` |
| `tests/provider-selector.test.ts` | Update copy assertions. |
| `tests/billing-usage-tab.test.ts` | Update badge assertions. |
| `tests/dashboard-coverage.test.ts` | If it greps for "BYOK", flip to "Self-managed". |

### 4. CLI (`zombiectl/`)

**Files & changes:**

| File | Change |
|---|---|
| `zombiectl/src/cli.js` | Any `--help` text mentioning BYOK rephrased. `tenant provider set --byok` flag (if present) → `--self-managed` with a deprecation warning aliasing `--byok` for one cycle. |
| `zombiectl/src/lib/contact.js` (new) | `export const SUPPORT_EMAIL = "usezombie@agentmail.to";` |
| `zombiectl/README.md` | One-line rephrase if it mentions BYOK. |
| Any `zombiectl install` or `tenant provider set` flow that prints contact info | Reference the constant, not a literal. |

(I haven't grepped `zombiectl/` exhaustively yet — will do during spec authoring.)

### 5. `docs/` (separate Mintlify repo at `~/Projects/docs/`)

**Files & changes:**

| File | Change |
|---|---|
| `snippets/rates.mdx` | `EVENT_RATE = "$0.01"` → `EVENT_RATE = "$0.000"` (or drop and replace `{EVENT_RATE}` usages with `"free"`). `STAGE_RATE = "$0.10"` → `STAGE_RATE = "$0.001"`. Comment block updated. |
| `snippets/contact.mdx` (new) | `export const SUPPORT_EMAIL = "usezombie@agentmail.to";` |
| `index.mdx` | Replace `usezombie@agentmail.to` literal with `{SUPPORT_EMAIL}` import. Strip any BYOK mention; reuse "pick your model provider" phrasing. Update "Early Access" banner if it mentions BYOK. |
| `concepts.mdx` | Drop BYOK from prose. Tree diagram already uses `provider:` (M65 fix). The "BYOK on…" provider list line gets the same rephrase. |
| `quickstart.mdx` | Already free of BYOK after M65; sweep to confirm. Replace any literal email. |
| `changelog.mdx` | **Historical entries stay BYOK.** Per Captain's "rewrite the past" call on rates only — terminology in changelog archives the term-at-the-time. M66's new `<Update>` block calls out the term retirement explicitly. |
| `zombies/credentials.mdx` | If it mentions BYOK, rephrase. |

### 6. `orgs/README.md` (`~/Projects/.github/profile/README.md`)

| Section | Change |
|---|---|
| Tagline | "Durable agent runtime. Wake-on-event. Evidence-driven." — keep. |
| Pricing copy (if any) | Sweep — currently appears not to mention pricing; verify. |
| Repo descriptions | Verify no BYOK mentions; rephrase if so. |
| Contact line (if added) | Use `usezombie@agentmail.to` directly (this repo can't import a constant — single literal). |

---

## Single-source variable, per-repo

Since these repos can't share a runtime module, the "single variable" requirement becomes "one constant per repo, all pointing at the same value":

| Repo | File | Constant |
|---|---|---|
| `usezombie/` Zig | `src/config/contact.zig` | `pub const SUPPORT_EMAIL = "usezombie@agentmail.to";` |
| `usezombie/` website | `ui/packages/website/src/lib/contact.ts` | `export const SUPPORT_EMAIL = "usezombie@agentmail.to";` |
| `usezombie/` app | `ui/packages/app/lib/contact.ts` | same |
| `usezombie/` CLI | `zombiectl/src/lib/contact.js` | same |
| `~/Projects/docs/` | `snippets/contact.mdx` | `export const SUPPORT_EMAIL = "usezombie@agentmail.to";` |
| `~/Projects/.github/profile/README.md` | n/a — literal (markdown doesn't import) | `usezombie@agentmail.to` |

A paired pin test in each repo (`contact.test.ts`, `contact_test.zig`) asserts the exact string. Drift between repos requires a paired multi-repo PR — same model as the rates pin.

---

## Risk + sequencing

**Risk register:**

| Risk | Mitigation |
|---|---|
| Mills migration corrupts existing tenant balances | Migration multiplies `balance_cents × 1000` in a single transaction; backup table created in same migration; tested locally with `make test-integration` from clean state. |
| API alias `mode: "byok"` ↔ `mode: "self_managed"` breaks third-party integrations | One-cycle dual-acceptance, with a deprecation header `Sunset: <date>` per RFC 8594. CLI emits a stderr warning when sent. |
| Test ID rename breaks downstream e2e tests | Inventory all `data-testid` references in this proposal's spec, ship grep-clean rename. |
| Pricing change is a one-way door (hard to raise later) | True. Mitigated by: (a) pricing copy on website + docs explicitly says "introductory rate, will increase post-GA"; (b) tracked decision in `docs/architecture/billing_and_byok.md` with rationale. |
| The proposal collapses two-rate structure into one rate (since both event rates are $0) | Surfaced in §1 above. May still want to keep `RATES_DISPLAY.event` rendering as `"free"` rather than dropping the event cell from the billing-flow diagram, so users still understand events are billed but at zero. |

**Sequencing (assumes D1=A, D2=first three tiers, D3=B):**

1. **Spec 1 — `M66_001_P1_PRICING_TRACTION_RATES`** (Zig + canonical TS rates + docs snippet). Lands the unit change, new constants, paired pin tests, migration. Includes the architecture-doc cross-effect.
2. **Spec 2 — `M66_002_P1_TERMINOLOGY_BYOK_RETIREMENT`** (website + app + CLI + docs). API alias rollout, copy sweep, component rename, test ID flip. References the M65 §0 vocab preamble — extends it.
3. **Spec 3 — `M66_003_P2_CONTACT_EMAIL_STANDARDIZATION`** (single constant per repo + paired tests + sweep). Lowest risk; could land first or fold into Spec 2.
4. **org `.github/profile/README.md`** — single direct edit on its own branch.

Specs 1 and 2 are independent (different file sets, different acceptance criteria); could land as parallel feature branches and merge order is flexible. Spec 3 is small enough to fold into 2.

---

## Open questions for Captain

1. **D1 — mills vs batched cents?** Recommend mills.
2. **D2 — BYOK retirement scope?** Recommend user-facing prose + identifiers + API alias; keep schema/enum/arch docs.
3. **D3 — design-partner copy?** Recommend reframing (option B) over deleting (option A).
4. **Pricing copy framing for "introductory rate."** Should the website say something like "Traction-stage pricing — rates rise post-GA"? Or leave the rate naked? Naked reads cleaner; framed sets expectation. Marginal call.
5. **Anything else you want stripped from user-facing surfaces alongside BYOK?** Posture toggle wording, "credit pool" vs "balance," "tenant" vs "account/workspace" — all candidates if we're already paying the rename cost.

🤖 Authored by Claude Opus 4.7 (1M context). Awaiting Captain decision before any spec is written.
