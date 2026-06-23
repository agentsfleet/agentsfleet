<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M98_001: Dashboard adopts one terminal-native design language; own-key model setup targets any OpenAI-compatible endpoint

**Prototype:** v2.0.0
**Milestone:** M98
**Workstream:** 001
**Date:** Jun 23, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing: the dashboard reads as several half-finished pages (mixed typefaces, two tab styles, stranded controls, a cluttered Models screen, an invisible email in the dark account modal), and own-key model setup cannot target a self-hosted / gateway OpenAI-compatible endpoint — both erode trust and block real customers.
**Categories:** Application Programming Interface (API), Command-Line Interface (CLI), User Interface (UI)
**Batch:** B1 — single workstream; suggested commit staging is the UI polish first (§1–§5), then the custom-endpoint feature (§6–§8) on top.
**Branch:** feat/m98-dashboard-polish-custom-endpoints
**Test Baseline:** unit=2015 integration=201 (Zig `src/**`, `make _lint_zig_test_depth` at CHORE(open); VERIFY Test Delta compares against this. UI vitest + CLI bun lanes track separately via their own coverage gates.)
**Depends on:** none
**Provenance:** agent-generated (interactive design-review session with Indy, Jun 23, 2026) — grounded in a clickable mockup (Billing / Models / Credentials, dark+light) Indy signed off screen-by-screen, the decision to bundle custom endpoints here (not a separate spec), the confirmation that `base_url` rides in the saved credential JSON ("it's just the json that gets saved"), and a read of every touched component, CLI command, and the resolver→runner→nullclaw chain; re-confirm at PLAN.

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` (visual source of truth — the mono typeface, the pulse accent, dark-primary, anti-vibes) + `docs/architecture/direction.md` §UI surfaces & §model-routing. The polish codifies *how* the existing system is applied; the custom-endpoint piece threads one validated field through the existing self-managed routing path — no new architectural concept, no schema migration.

---

## Implementing agent — read these first

1. `ui/packages/app/components/layout/Shell.tsx` + `ui/packages/design-system/src/{design-system/,tokens.css}` — nav, the shared content-width container + ambient glow, and the primitives/tokens to compose (Card, DataTable, LogLine, EmptyState, Tabs, TabNav, Badge, Button); **no arbitrary Tailwind values**.
2. `src/agentsfleetd/state/tenant_provider_resolver.zig` + `lib/contract/execution_policy.zig` + `runner/{child_exec_input.zig,engine/wire.zig,engine/runner_helpers.zig,network/AllowList.zig}` — the self-managed credential JSON (`{provider, api_key, model}`) and the resolve→policy→runner→nullclaw→egress-allowlist chain that `base_url` threads through.
3. `cli/src/commands/{tenant.ts,fleet_credential.ts}` + `cli/src/services/credentials.ts` + `cli/test/acceptance/{tenant-provider-mutation.spec.ts,credential-vault.spec.ts}` (+ `fixtures/{tenant-provider-ops,credential-ops,command-matrix}.ts`) — the CLI provider/credential surface and the acceptance specs this work extends.
4. `dispatch/write_zig.md` + `dispatch/write_ts_adhere_bun.md` — Zig (ZIG/PUB/LIFECYCLE) and TS (FILE SHAPE, primitive substitution, DESIGN TOKEN) discipline.
5. `ui/packages/app/lib/clerkAppearance.ts` + `docs/DESIGN_SYSTEM.md` — the Clerk appearance map (dark-theme contrast fix) and the binding type ramp / pulse-as-currency / anti-vibes rules.

> **Confirm at handshake (precedent exists, so not a `[?]`):** nullclaw's engine `Config` accepts a per-request endpoint/base_url override for an OpenAI-compatible provider — the existing `azure`/`vertex` providers already require endpoint configuration, so the knob exists. If it does not, that is a blocking upstream dependency — STOP and surface to Indy before EXECUTE.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m98): dashboard polish + custom OpenAI-compatible endpoints`
- **Intent (one sentence):** an operator sees one coherent terminal-native dashboard (sans chrome, mono data, one tab style, one width, calm glow) across Billing/Models/Credentials, and can point own-key model setup — from the CLI or the UI — at any OpenAI-compatible URL via a credential that carries its base URL.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm: (a) UI verification targets (`make lint-app`, `make test-unit-app`, `make dry-smoke`) and acceptance targets (`make acceptance-e2e`, `make cli-acceptance`); (b) the design-system tab primitives + their call sites; (c) `/credentials` is the live top-level route and `/settings/models` carries the in-page Credentials tab to remove; (d) nullclaw base_url override (above); (e) `base_url` needs no schema change (vault stores arbitrary credential JSON); (f) `ExecutionPolicy` can gain a nullable field without breaking in-flight leases. A `[?]` blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — an operator opens Billing → Models → Credentials and never thinks "is this the same app?"; the balance meter fills, the usage ledger reads like a terminal log, the model decision is one glance, Light theme stays readable; then they run `agentsfleet` to add an OpenAI-compatible credential with a base URL, point the model at it, trigger a fleet, and the run completes against their gateway — Events shows a normal run.
2. **Preserved user behaviour** — every existing action still works unchanged: platform-default vs named-provider own-key setup, add/delete credential, usage history with load-more, Workspace/API-Keys tabs, theme toggle, the account modal, and every CLI command. A credential without `base_url` behaves exactly as today.
3. **Optimal-way check** — fix the *application* of the existing design system + thread one validated field through the existing self-managed path; the gap to "perfect" (deeper motion; a typed multi-endpoint provider catalog) is deferred deliberately.
4. **Rebuild-vs-iterate** — iterate. Design system, tokens, primitives, and the resolve→runner chain all exist; the defects are inconsistent application and a missing field. A rebuild trades determinism for nothing.
5. **What we build** — a typography/tab/width/header/ink/motion standard in the shared layer; redesigned Billing (balance+meter, terminal usage ledger, no seat grid); the Models/Credentials sidebar split + decluttered two-option Models screen; a Credentials *vault* (model providers · custom secrets · integrations-coming-soon); the Clerk dark-mode fix; and `base_url`-carrying credentials with SSRF-safe validation threaded to the engine + egress allowlist, surfaced in both the CLI and the UI; and one minimal, unified install-fleet experience (template / GitHub source / paste SKILL.md, with its import states) shared by the Dashboard and the Fleets install page.
6. **What we do NOT build** — first-class **GitHub/Zoho/Slack** connectors (Coming soon; custom secrets bridge them) — **the GitHub connector + a config-driven credential broker are decoupled to M99_001** (Jun 23, 2026; see that spec); a typed multi-endpoint provider catalog; non-OpenAI wire formats; per-credential fleet-usage tracking beyond the active model credential; any billing-model change (consumption/prepaid stays); credential rotation automation.
7. **Fit with existing features** — compounds with the design system, the settings IA, the self-managed own-key path, and the credential vault; must not destabilize platform-default routing, the billing data path, or the runner egress allowlist (the SSRF boundary).
8. **Surface order** — UI polish (§1–§5) is UI-only. The custom-endpoint feature is **CLI-first** per repo default (`agentsfleet`): the routing + CLI land the contract, the UI option follows. Justified: the credential JSON shape is the contract both surfaces consume.
9. **Dashboard restraint** — **GitHub/Zoho/Slack** Integrations render as "Coming soon / Planned" with no Connect control (the GitHub connector is M99_001); the custom-secrets "referenced by" shows only the *known* reference (active model credential), never a fabricated usage graph; the `base_url` field appears only under the explicit "Custom — OpenAI-compatible" choice.
10. **Confused-user next step** — a user unsure how to give a fleet a GitHub token reads the inline hint (store `GITHUB_TOKEN` as a custom secret) and self-serves; a user whose endpoint is rejected gets a typed error naming the reason (not-https / blocked-host / unreachable) + a hint, from both the CLI (structured JSON error) and the UI; the account-modal user can now read their email.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE CFG (config-driven over enum-driven: the openai-compatible path and the three credential *kinds* are data, not new hardcoded branches), RULE VLT (api_key stays vault-only; never logged/returned), RULE NTP (narrow `base_url` to a validated type at the parse boundary), RULE PRI (treat the user-supplied URL as hostile — SSRF guard), RULE ECL (invalid/blocked/unreachable endpoint is a typed validation/denied/timeout class, not fatal-silent), RULE EMS (standard error structure), RULE TGU (tagged-union validation result), RULE JCL (CLI JSON contract discipline for the new flags/output), RULE UFS (content-width constant, glow tokens, tab classes, kind labels, the `"openai-compatible"` id + `/chat/completions` suffix → named constants/tokens shared verbatim with tests), RULE NDC/NRC/NLR/ORP/TST-NAM.
- **`dispatch/write_zig.md`** — tagged-union results, multi-step `errdefer`, pg-drain (if any query touches), file ≤350 / fn ≤50, cross-compile both linux targets.
- **`dispatch/write_ts_adhere_bun.md`** — TS FILE SHAPE per new component, primitive substitution, DESIGN TOKEN gate.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — credential create + `PUT /v1/tenants/me/provider` (shape unchanged; the URL rides in the credential).
- **`docs/DESIGN_SYSTEM.md`** — type ramp, pulse-as-currency, dark-primary, anti-vibes (the typography rule is checked against it).
- **`docs/AUTH.md`** — the GitHub connector (an auth flow) is **decoupled to M99_001**; this workstream touches no connect/callback/token-minting surface. (The public-repo bundle import in §9 stays unauthenticated, as today.)
- No schema DDL → `docs/SCHEMA_CONVENTIONS.md` does not apply (`base_url` is vault JSON, not a column).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — resolver, policy, runner, allowlist, base_url guard | tagged-union results; `errdefer`; cross-compile `x86_64-linux` + `aarch64-linux` |
| PUB / Struct-Shape | yes — `ExecutionPolicy` gains a nullable `base_url` | shape verdict: nullable field; backward-compatible deserialize of existing leases |
| File & Function Length (≤350/≤50/≤70) | yes — new screens + the base_url guard | split screens into child components; extract the guard into its own unit |
| UFS (repeated/semantic literals) | yes — width const, glow/ink tokens, tab classes, kind labels, provider id, URL suffix, error strings | named constants/tokens in one module per side; tests import them |
| UI Substitution / DESIGN TOKEN | yes — every `.tsx` edit | design-system primitives; theme tokens only |
| ERROR REGISTRY (`UZ-XXX-NNN`) | yes — invalid/blocked endpoint | register a new code + `hint()` entry |
| LOGGING / OBS | yes — own-key custom selection + a blocked-host rejection are observable | log/event per the LOGGING standard; api_key never in the log |
| SCHEMA | no — no DDL; base_url rides in vault JSON | — |

---

## Overview

**Goal (testable):** the dashboard renders one design language (one underline tab primitive, one ~1200px content container (a readability-cap token `--content-max`; chat/steer + large tables go full-width) + ambient dual-tone glow on every page, `PageHeader` description-below-title, sans chrome / mono data, ink CTA in light), with Billing/Models/Credentials rebuilt to it and the Clerk account modal readable in dark; and a self-managed credential carrying `base_url` is validated (https, SSRF-safe), threaded through `ExecutionPolicy` → runner → nullclaw → egress allowlist, and settable from both the CLI and the UI — with an invalid or SSRF-unsafe URL rejected with a typed error before any run. The install-fleet experience (template / GitHub source / paste SKILL.md, with its import states) is one minimal shared flow across the Dashboard and the Fleets install page.

**Problem:** the design system is applied inconsistently (two tab visuals, varying widths + stranded controls, monospace on chrome, a thrice-restated Models screen, Credentials buried as a tab, an invisible dark-theme email), and own-key model setup resolves only the hardcoded named providers — a customer on self-hosted vLLM / OpenRouter / a gateway has no way to point a fleet at it.

**Solution summary:** codify the design language in the shared layer, rebuild the three worst screens on it, split Models/Credentials into two destinations, fix the Clerk token, add a reduced-motion-gated motion pass; and carry an optional `base_url` in the existing self-managed credential JSON (no migration), validate it at the resolver boundary against an SSRF guard, thread it to the engine + egress allowlist, and surface "Custom — OpenAI-compatible" in both the CLI (credential add + provider set) and the UI.

---

## Prior-Art / Reference Implementations

- **UI** → design-system primitives + `tokens.css`; `LogLine`/`DataTable` for the terminal usage ledger, `Card`/`Badge`/`EmptyState` for the rest. Tab unification adopts the underline visual implied by `docs/DESIGN_SYSTEM.md` (pill style retired; `Tabs`/`TabNav` keep their semantic split but share one style module).
- **CLI** → the "7 Pillars" of CLI developer experience (handler purity, output-as-a-service, structured JSON errors with suggestion/retry, 3-tier test pyramid, auto-JSON when piped). Extend the existing `tenant`/`fleet_credential` commands and the `credential-vault.spec.ts` / `tenant-provider-mutation.spec.ts` acceptance specs rather than inventing a new surface.
- **Backend** → mirror the existing `{provider, api_key, model}` extraction in `tenant_provider_resolver.zig` for `base_url`; mirror how `azure`/`vertex` already pass an endpoint to nullclaw; reuse `AllowList.zig`'s host-derivation.
- **House style** → `docs/v2/done/M92_001_P1_UI_SUPPORT_WEDGE_WEBSITE_REFRESH.md` (token-precise, guard-tested UI workstream).
- Approved visual: the Jun 23 mockup Indy signed off — provenance only; binding rules live here + in `DESIGN_SYSTEM.md`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/design-system/src/design-system/Tabs.tsx` · `TabNav.tsx` | EDIT | underline visual via one shared style module; retire pill style |
| `ui/packages/design-system/src/design-system/PageHeader.tsx` | EDIT | stack title → description below (`PageDescription` slot) |
| `ui/packages/design-system/src/tokens.css` · `design-system/Button.tsx` | EDIT | light-mode ink CTA token; glow tokens read in both themes; primary resolves to ink in light |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | nav: Models + Credentials entries; shared content-width container; ambient glow; mount-motion wrapper |
| `ui/packages/app/lib/clerkAppearance.ts` | EDIT | dark-theme UserProfile secondary-identifier (email) → readable token |
| `ui/packages/app/app/(dashboard)/settings/billing/page.tsx` · `components/BillingBalanceCard.tsx` · `components/BillingUsageTab.tsx` | EDIT | description-below-title; balance + full-width meter + caption + ink CTA; terminal usage ledger + empty state; no seat grid |
| `ui/packages/app/app/(dashboard)/settings/models/page.tsx` · `components/ProviderSelector.tsx` | EDIT | drop in-page Credentials tab; two option-cards; own-key "Custom — OpenAI-compatible" option reveals base-URL field |
| `ui/packages/app/app/(dashboard)/credentials/page.tsx` · `components/CredentialsList.tsx` · `components/AddCredentialForm.tsx` · `components/IntegrationsComingSoon.tsx` (CREATE) | EDIT/CREATE | vault layout (kinds strip + grouped sections); custom-secrets table + best-effort referenced-by; the Custom — OpenAI-compatible credential (base URL + key); Planned integrations + bridge hint |
| `ui/packages/app/lib/api/credentials.ts` | EDIT | type the optional `provider`/`base_url` fields in the credential data shape |
| `ui/packages/app/app/(dashboard)/fleets/new/page.tsx` · `InstallFleet.tsx` · `InstallSourceSelector.tsx` · `TemplateCard.tsx` · `InstallFleetForm.tsx` | EDIT | minimal Install page; inline **state-driven** flow (replaces the review step) for the three paths |
| `ui/packages/app/app/(dashboard)/fleets/new/InstallStates.tsx` | CREATE | live import→create state progression (importing · connect-to-continue · creating · done · error/retry), terminal STATES aesthetic; **post-create steps consume the existing SSE fleet-event stream via `useFleetEventStream`** (no polling) |
| `ui/packages/app/app/(dashboard)/fleets/new/BundlePreview.tsx` | DELETE | the review page is replaced by the inline states; its requirement transparency folds into the connect-to-continue state (RULE NDC/ORP) |
| `ui/packages/app/app/(dashboard)/page.tsx` · `fleets/page.tsx` · `fleets/[id]/page.tsx` | EDIT | Dashboard + Fleets empty-state compose the shared install component (drop the hand-rolled duplicate); Fleets list shows an installing indicator; fleet detail = **full-height steer/chat** that shows install states while provisioning |
| `src/agentsfleetd/http/handlers/fleets/create_stream.zig` + the provisioning path | EDIT | emit typed `install:*` step events to the fleet event stream; flip `core.fleets.status` `installing→active` on ready (named-constant status value; no DDL) |
| `ui/packages/app/app/(dashboard)/credentials/components/IntegrationsComingSoon.tsx` | CREATE | GitHub/Zoho/Slack render Planned (no Connect) + the custom-secret bridge hint; the one-click GitHub connector is M99_001 |
| `src/agentsfleetd/state/tenant_provider_resolver.zig` | EDIT | extract + validate optional `base_url` from the self-managed credential JSON |
| `src/agentsfleetd/state/base_url_guard.zig` | CREATE | validate https + SSRF-safe host (reject loopback/private/link-local/metadata); tagged-union result |
| `src/agentsfleetd/lib/contract/execution_policy.zig` | EDIT | nullable `base_url`; derive `inference_host` from it for custom endpoints; backward-compatible deserialize |
| `src/agentsfleetd/runner/child_exec_input.zig` · `runner/engine/wire.zig` · `runner/engine/runner_helpers.zig` | EDIT | thread `base_url` into the engine config + nullclaw provider init |
| `src/agentsfleetd/runner/network/AllowList.zig` | EDIT | custom host passes; SSRF-unsafe host denied |
| _error registry module_ | EDIT | new `UZ-*` code + `hint()` for invalid/blocked endpoint |
| `cli/src/commands/tenant.ts` · `commands/fleet_credential.ts` · `services/credentials.ts` | EDIT | credential add carries `provider:"openai-compatible"` + `base_url`; provider set selects it (structured-JSON error on invalid) |
| `cli/test/acceptance/tenant-provider-mutation.spec.ts` · `credential-vault.spec.ts` · `fixtures/{tenant-provider-ops,credential-ops}.ts` | EDIT | custom-endpoint acceptance scenarios + fixtures |
| _colocated tests: Zig `test {}` blocks · `*.test.tsx` · `*.unit.test.ts` · `*.integration.test.ts`_ | CREATE/EDIT | one test per Dimension below |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, eight Sections, bundled per Indy's direction ("must be bundled in M98_001"). The shared standard (§1) lands first so the rebuilt screens (§2–§4) compose it; the Clerk fix (§5) is the same readable-in-both-themes defect class; the custom endpoint is CLI-first backend (§6–§7) then surfaced (§8). Suggested commit staging: §1–§5, then §6–§8.
- **Alternatives considered:** (a) two specs (polish + endpoints) — Indy rejected, bundle here; (b) a `base_url` column on `core.tenant_providers` — rejected: the vault already stores arbitrary credential JSON, a column is needless migration + a second source of truth; (c) a typed multi-endpoint provider catalog — rejected as over-build for one OpenAI-compatible path (RULE CFG).
- **Patch-vs-refactor verdict:** **patch** — consistent application of an existing design system + one validated field threaded through an existing routing chain, plus a contained nav reorganization. The deeper motion system, full fleet-usage tracking, and a typed provider catalog are named follow-ups.

---

## Sections (implementation slices)

### §1 — Design-language foundation

One underline tab visual shared by `Tabs`/`TabNav` (pill retired); `PageHeader` stacks title→description; `Shell` wraps page content in one container whose **max-width is a single token** (`--content-max`, ≈1200px; fluid below it — it is a readability cap, not a fixed width, and **chat/steer + large data tables go full-width**, needing no reading cap) with the ambient dual-tone `--pulse` glow behind every page; a light-mode **ink** primary-CTA token (mint reserved for accents/links/active/glow); a restrained motion pass (mount-rise, glow drift, live-status ping, meter fill, micro-interactions) entirely behind `prefers-reduced-motion`. **Implementation default:** typography = sans for titles/body/buttons, mono only for IDs/paths/table-cells/section-labels/log surfaces — chosen via the existing sans/mono token, not new classes.

- **Dimension 1.1** — `Tabs`/`TabNav` render underline; zero pill-style class in `src/` → Test `test_tabs_underline_no_pill`
- **Dimension 1.2** — `PageHeader` renders description below the title → Test `test_pageheader_description_below`
- **Dimension 1.3** — every dashboard page composes the shared width container; none sets its own max-width → Test `test_pages_use_shared_content_width`
- **Dimension 1.4** — primary `Button` resolves to ink under `[data-theme=light]`, pulse under dark → Test `test_primary_button_ink_in_light`
- **Dimension 1.5** — `prefers-reduced-motion` disables all animations → Test `test_motion_respects_reduced_motion`

### §2 — Billing redesign

Balance + full-width consumption meter (caption rides its right end; CTA in the header row — no stranded gap); a terminal-native usage ledger (`date · amount · type · description`) from `core.fleet_execution_telemetry` via the existing endpoints, with an empty state; **no seat-plan grid** (consumption/prepaid) — one "Pay as you go" row + a volume-pricing link. Presentation only; the data path is unchanged.

- **Dimension 2.1** — balance card: amount + full-width meter + caption + header CTA; no stranded right gap → Test `test_billing_balance_layout`
- **Dimension 2.2** — usage ledger renders charge rows mono-on-data; empty state when no charges → Test `test_billing_usage_ledger_and_empty`
- **Dimension 2.3** — no seat/plan grid; "Pay as you go" row + volume link instead → Test `test_billing_no_seat_grid`

### §3 — Models screen + sidebar split

`Shell` nav gains **Models** (`/settings/models`) and **Credentials** (`/credentials`); the in-page Credentials tab on Models is removed (no double-nav). The Models screen collapses the three "platform defaults" restatements into **two option-cards** (active is badged "Current", reads "Active — nothing to do"; the action lives only on the option you'd switch *to*). Own-key uses the existing named-provider flow; the "Custom — OpenAI-compatible" option is wired in §8.

- **Dimension 3.1** — sidebar shows Models + Credentials resolving to the two routes → Test `test_nav_models_credentials_split`
- **Dimension 3.2** — two option-cards; active shows "Current" + no action button; inactive shows the switch action → Test `test_models_two_option_cards`
- **Dimension 3.3** — Models page renders no in-page Credentials tab → Test `test_models_no_inpage_credentials_tab`

### §4 — Credentials vault screen

`/credentials` becomes the secret **vault**: a kinds strip then groups in order Model-providers → Custom-secrets → Integrations. Model-provider credentials are write-only, masked, Replace-not-reveal. Custom secrets = arbitrary `NAME=value` the SKILL reads by name, with Set/Empty status + best-effort referenced-by (active model credential; full fleet-usage is follow-up). Integrations: **GitHub/Zoho/Slack all render "Coming soon / Planned"** with no Connect control and the custom-secret bridge hint — the first-class GitHub connector (Connect → auth flow → minted token) is **decoupled to M99_001** (Connectors + Credential Broker). The three kinds are config-driven rows (RULE CFG).

- **Dimension 4.1** — kinds strip + three groups in order providers→custom→integrations → Test `test_credentials_vault_order`
- **Dimension 4.2** — stored credential masked (suffix only), Replace/Delete, never re-reveals → Test `test_credential_write_only_masked`
- **Dimension 4.3** — custom secret create takes NAME+value; lists with Set/Empty status → Test `test_custom_secret_create_and_status`
- **Dimension 4.4** — GitHub/Zoho/Slack Integrations render "Planned", no Connect, with the custom-secret bridge hint → Test `test_integrations_coming_soon`

### §5 — Clerk dark-theme contrast fix

The account modal's email/secondary-identifier is invisible in dark theme (maps to a too-dim token). Map it to a readable text token (set base theme explicitly if auto-detect is wrong) so it's legible in both themes.

- **Dimension 5.1** — `clerkAppearance` maps the UserProfile secondary-identifier to the readable token in dark → Test `test_clerk_secondary_identifier_contrast`

### §6 — Custom-endpoint: credential `base_url` + resolver SSRF validation

The self-managed credential JSON may carry `provider:"openai-compatible"` + `base_url` alongside `api_key`/`model`. The resolver extracts `base_url` and validates it through a dedicated guard before any run. **Implementation default:** require `https`; reject loopback (`127/8`,`::1`), private (`10/8`,`172.16/12`,`192.168/16`), link-local incl. cloud metadata (`169.254/16`), and unspecified hosts (RULE PRI/NTP). Missing `base_url` resolves exactly as today.

- **Dimension 6.1** — openai-compatible credential + valid base_url → resolves with the URL carried → Test `test_resolver_extracts_base_url`
- **Dimension 6.2** — non-https/malformed → typed invalid-endpoint error; no run → Test `test_resolver_rejects_non_https`
- **Dimension 6.3** — SSRF-unsafe host → blocked before any run → Test `test_resolver_blocks_ssrf_hosts`
- **Dimension 6.4** — credential without base_url → resolves unchanged → Test `test_resolver_named_provider_unchanged`

### §7 — Custom-endpoint: policy + runner threading

`ExecutionPolicy` carries a nullable `base_url`; for custom endpoints the egress-allowlist host derives from it. The runner threads `base_url` through the engine wire into nullclaw's OpenAI-compatible provider init so requests dial the custom URL; the allowlist permits exactly that host. Existing leases without `base_url` deserialize to null and route via the named-provider table.

- **Dimension 7.1** — policy round-trips base_url; a legacy lease without it → null → Test `test_policy_base_url_optional_roundtrip`
- **Dimension 7.2** — engine config built from a base_url policy dials the injected fake endpoint, not the named table → Test `test_runner_injects_base_url`
- **Dimension 7.3** — the custom host is allowed; an off-list host from the same run is denied → Test `test_allowlist_permits_custom_host`

### §8 — Custom-endpoint surfaces: CLI + UI

CLI: `agentsfleet` credential-add carries `provider:"openai-compatible"` + `base_url` (+ key/model); provider-set selects such a credential; invalid input returns a structured-JSON error (RULE JCL). UI: the Credentials vault gains the "Custom — OpenAI-compatible" model-provider row (base URL + optional key); the Models own-key form gains the "Custom — OpenAI-compatible" option revealing the base-URL field and selecting such a credential. The `PUT /provider` payload is unchanged — the URL lives in the referenced credential.

- **Dimension 8.1** — CLI credential-add submits provider="openai-compatible" + base_url; non-https → structured-JSON error, non-zero exit → Test `test_cli_custom_credential_add`
- **Dimension 8.2** — CLI provider-set selects an openai-compatible credential; `--json` output reflects the custom setup → Test `test_cli_provider_set_custom`
- **Dimension 8.3** — UI custom credential form submits provider+base_url via `createCredential` → Test `test_custom_credential_form_payload`
- **Dimension 8.4** — UI own-key "Custom" option reveals the URL field, selects the credential, calls `setTenantProviderSelfManaged` with its ref → Test `test_models_custom_option_select`

### §9 — Unified, minimal, state-driven install-fleet experience (Dashboard + Fleets)

One install experience, three paths — **template**, **GitHub source** (`owner/repo` → Import), **paste SKILL.md** — composed identically by the Dashboard "Start your fleet" card, the Fleets `new` install page, and the Fleets empty-state CTAs. Today the Dashboard hand-rolls its own cards, `fleets/new` has another set, the page reads cluttered, and choosing a template routes to a separate **review page** (`BundlePreview`) before anything happens. This collapses the entry points to **one shared, minimal install component** (clear hierarchy: templates first, `owner/repo` source second, paste-SKILL.md a quiet tertiary link; §1 typography/buttons) and **replaces the review page with an inline, state-driven flow**: clicking Use template / Import / Paste-create proceeds **in place**, rendering the live import→create states in the terminal-native STATES aesthetic (§1) — it **must not navigate to a review/preview page**. States, in order: importing/fetching (`SKILL.md`, `TRIGGER.md`, support files) → either **first-run no-credentials** (the requirement transparency `BundlePreview` used to show — needed credentials/tools/network-hosts — surfaced here as a **connect-to-continue** state) and/or **skill-only bundle** (no TRIGGER.md → a manual / API wake is generated) → creating → done; the error states repo-not-found / no-SKILL.md / rate-limited (404/429) render with a retry. **And an installing fleet always surfaces its install state** — while a create is in flight, and (if the fleet model carries an installing status) until it clears, the states are visible in the flow and the fleet shows an installing indicator in the Fleets list/detail, so progress is never hidden. **Implementation default:** extract the shared experience + an `InstallStates` progression once, compose in all entry points, and remove `BundlePreview` (its requirement transparency folds into the connect-to-continue state). **Live-status transport (resolved — reuse, don't reinvent):** pre-create steps (importing → connect-to-continue) are **client-driven from the import/create request responses** — the page makes the calls, so it knows each result. On **create**, `create_stream.zig` already provisions the fleet's install event stream and returns the fleet with `status:"installing"`; the page then subscribes to the **existing SSE fleet-event stream** (`useFleetEventStream` → `/fleets/{id}/events/stream`, Bearer injected server-side) and advances each step the instant the backend emits it — **no polling, no new WebSocket** (SSE is server→client, exactly what status needs). The backend emits typed `install:*` step events (`install:creating → install:provisioning → install:ready`, plus `install:error`) as the provisioning lease crosses them; the existing `core.fleets.status` column flips **`installing → active`** on ready (a new status value, app-enforced named constant — no DDL). **Install states and the steer/chat are one stream**: the page renders install-mode until `install:ready`, then becomes the chat (§9.6); the Fleets list installing badge reads `fleet.status`. **Friction default (fewer end-user clicks):** create **auto-proceeds** — no confirm beat between *imported* and *creating*; after a clean import it fires immediately, and when a credential was needed the **instant the connect-to-continue gate is satisfied the flow auto-resumes** into creating. One click (Use template / Import / Paste-create) commits; the system does the rest. **Connector decoupling:** the one-click GitHub **Connect** is M99_001; until it lands, the **connect-to-continue** state resolves a needed integration via the **custom-secret bridge** (e.g. store `GITHUB_TOKEN`), not an App connect — the connect-to-continue UX is under active review (Install-page spar).

- **Dimension 9.1** — `fleets/new` renders the three paths (template grid · `owner/repo` import · paste SKILL.md) minimal, in the §1 design language → Test `test_install_three_paths_render`
- **Dimension 9.2** — Dashboard "Start your fleet" and the Fleets empty-state CTAs compose the **same** shared install component (no hand-rolled duplicate) → Test `test_install_experience_shared`
- **Dimension 9.3** — clicking Use template / Import / Paste-create proceeds **inline to the states**, never routing to a review/preview page; create fires with the correct source → Test `test_install_inline_state_driven`
- **Dimension 9.4** — states render in order: importing → (connect-to-continue when credentials needed | skill-only no-TRIGGER) → creating → done; error states (404/no-SKILL.md/rate-limited) show retry → Test `test_install_states_render`
- **Dimension 9.5** — an installing fleet always surfaces its installing state in the Fleets list/detail until it resolves → Test `test_installing_fleet_always_visible`
- **Dimension 9.6** — install done → "**Open fleet**" lands in the fleet's **full-height steer/chat** thread (composer pinned, message list scrolls); a still-provisioning fleet shows the install states there first, then becomes the chat → Test `test_install_lands_in_steer`
- **Dimension 9.7** — live status: post-create, `InstallStates` consumes the existing SSE fleet-event stream (`useFleetEventStream`); each `install:*` event advances the rendered step with no polling; `install:ready` flips `fleet.status` `installing→active` and ends install-mode → Test `test_install_status_stream`

---

## Interfaces

Credential data JSON (self-managed; stored encrypted in `vault.secrets`, never returned):

```
{ "provider": "openai-compatible" | "<named>",
  "api_key":  "<secret>",
  "model":    "<model id>",       // optional
  "base_url": "https://host/v1"   // required iff provider == "openai-compatible"; rejected otherwise
}
```

- `PUT /v1/tenants/me/provider` body **unchanged** (`{ mode:"self_managed", credential_ref, model? }`) — the URL lives in the referenced credential.
- `ExecutionPolicy` gains nullable `base_url` (internal contract); existing serialized leases deserialize with `base_url = null`.
- Validation result is a tagged union: `ok | invalid_scheme | blocked_host | malformed`.
- CLI: credential-add accepts `--provider openai-compatible --base-url <url> --api-key <key> [--model <m>]`; on rejection emits the standard structured-JSON error (code + message + suggestion). Existing design-system primitive props (`PageHeader`, etc.) stay backward-compatible.
- All existing API client signatures consumed unchanged (`getTenantBilling`, `listTenantBillingCharges`, `listCredentials`, `createCredential`, `deleteCredential`, `getTenantProvider`, `setTenantProviderSelfManaged`, `resetTenantProvider`).

Install progression (reuses the existing fleet event stream; consumed via `useFleetEventStream` / SSE — no new transport):

```
on click (Use template / Import / Paste-create) → POST …/fleets/bundles/snapshots
        → "importing" → "imported" + requirements (needs[], tools, hosts); the snapshot is the immutable source
requirements met (or the instant connect-to-continue is satisfied) → POST …/fleets { snapshot_id }   ← AUTO, no confirm beat
        → "creating" → { fleet_id, status:"installing" } → open the SSE stream (below)
post-create: SSE events on /fleets/{id}/events/stream →
             install:creating | install:provisioning | install:ready | install:error
core.fleets.status : "installing" on create → "active" on install:ready   (existing column; new value = named constant, no DDL)
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Reduced motion | `prefers-reduced-motion: reduce` | all animations disabled; final state immediate |
| Narrow viewport | < tablet breakpoint | option-cards/rows/ledger stack; no horizontal page scroll |
| Light theme legibility | theme toggle to light | ink CTA + readable text everywhere incl. the Clerk modal |
| Billing not bootstrapped | `getTenantBilling` null/500 | existing "Billing isn't ready yet" empty state preserved |
| No charges yet | empty charges list | usage ledger empty state, not a void |
| Delete active-model credential | deleting an in-use credential | confirm warns it is the active model credential |
| Non-https base_url | `http://`/other scheme | typed invalid-endpoint error at save/resolve; CLI structured error + non-zero exit; UI inline flag |
| Malformed URL | unparseable | typed malformed error; no run |
| SSRF target | loopback/private/link-local/metadata host | blocked-host error; never dialed; rejection logged (no secret) |
| openai-compatible without base_url (or base_url without it) | mismatched fields | rejected as malformed credential at the boundary |
| Endpoint unreachable / non-OpenAI response | network/timeout / wrong shape at run | run fails with the endpoint error surfaced in Events; ECL timeout/retryable class, not silent |
| api_key leak | logging the credential | api_key never logged or returned (VLT); only the host appears |
| Repo not found / no SKILL.md / rate-limited | bad `owner/repo`, GitHub 404 or 429 | install shows the not-found/rate-limited state with a retry — not a crash or silent blank |
| First-run, no credentials | template `NEEDS:` a credential not present | install shows the requirement + the custom-secret bridge, and gates create until the credential is present (the one-click connector is M99_001) |
| Skill-only bundle | imported source has SKILL.md but no TRIGGER.md | install informs a manual / API wake is generated; create still succeeds |
| Malformed SKILL.md paste | invalid pasted content | inline validation error; create blocked |
| Install stream drops | SSE/EventSource disconnect mid-provision | EventSource auto-reconnects; on resume `fleet.status` + the latest `install:*` event re-render the correct step — no lost progress, never a stuck spinner |

---

## Invariants

1. One tab visual — the retired pill-tab class has **zero** occurrences in `src/` — enforced by `test_tabs_underline_no_pill` + grep.
2. One content width — a single exported constant; pages compose the shared container, set no own max-width — enforced by `test_pages_use_shared_content_width`.
3. No arbitrary Tailwind values; motion is opt-out — DESIGN TOKEN lint + every animation behind `prefers-reduced-motion` (`test_motion_respects_reduced_motion`).
4. Credentials are write-only — UI/CLI never render a stored secret beyond a masked suffix; no reveal — enforced by `test_credential_write_only_masked`.
5. `base_url` is https + SSRF-safe, and `provider=="openai-compatible"` ⇔ valid `base_url` present — enforced by `base_url_guard.zig` (tagged-union) at the parse boundary, not review.
6. Requests reach **only** the validated custom host — egress-allowlist host derives from the same `base_url`; off-list host denied by `AllowList.zig`.
7. The api_key is never logged or returned — VLT (vault-only) + a log audit; existing no-base_url leases/credentials are byte-for-byte unchanged in behaviour.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_tabs_underline_no_pill` | underline-active class present; grep finds zero pill class in `src/` |
| 1.2 | unit | `test_pageheader_description_below` | description node renders after the title in DOM order |
| 1.3 | unit | `test_pages_use_shared_content_width` | each page composes the shared container; none sets own `max-w-*` |
| 1.4 | unit | `test_primary_button_ink_in_light` | primary under light → ink token; under dark → pulse |
| 1.5 | unit | `test_motion_respects_reduced_motion` | reduced-motion mock → computed animation none |
| 2.1 | unit | `test_billing_balance_layout` | amount+meter+caption+header CTA; CTA not stranded |
| 2.2 | unit | `test_billing_usage_ledger_and_empty` | charge rows date/amount/type/description; empty list → empty state |
| 2.3 | unit | `test_billing_no_seat_grid` | no per-seat cards; "Pay as you go" row + volume link present |
| 3.1 | unit | `test_nav_models_credentials_split` | nav renders Models→/settings/models, Credentials→/credentials |
| 3.2 | unit | `test_models_two_option_cards` | active card "Current"+no action; inactive has switch action |
| 3.3 | unit | `test_models_no_inpage_credentials_tab` | Models page renders no Credentials tab trigger |
| 4.1 | unit | `test_credentials_vault_order` | kinds strip + groups order providers→custom→integrations |
| 4.2 | unit | `test_credential_write_only_masked` | masked suffix; no reveal; Replace present |
| 4.3 | unit | `test_custom_secret_create_and_status` | NAME+value → `createCredential`; row shows Set/Empty |
| 4.4 | unit | `test_integrations_coming_soon` | GitHub/Zoho/Slack "Planned", no Connect, bridge hint |
| 5.1 | unit | `test_clerk_secondary_identifier_contrast` | secondary-identifier mapped to the readable token, not the subtle one |
| 6.1 | unit | `test_resolver_extracts_base_url` | valid https base_url → resolved policy carries it |
| 6.2 | unit | `test_resolver_rejects_non_https` | http/garbage → `invalid_scheme`; no policy |
| 6.3 | unit | `test_resolver_blocks_ssrf_hosts` | `127.0.0.1`,`10.x`,`169.254.169.254`,`::1` → `blocked_host` |
| 6.4 | unit | `test_resolver_named_provider_unchanged` | no base_url → identical to current resolution |
| 7.1 | unit | `test_policy_base_url_optional_roundtrip` | policy serializes/deserializes base_url; legacy lease → null |
| 7.2 | integration | `test_runner_injects_base_url` | engine config from a base_url policy dials the injected fake endpoint, not the named table |
| 7.3 | integration | `test_allowlist_permits_custom_host` | custom host allowed; off-list host from same run denied |
| 8.1 | integration | `test_cli_custom_credential_add` | `agentsfleet` credential-add openai-compatible+base_url succeeds; non-https → structured-JSON error + non-zero exit |
| 8.2 | e2e (cli-acceptance) | `test_cli_provider_set_custom` | CLI sets provider to the openai-compatible credential vs live API; `--json` reflects custom setup |
| 8.3 | unit | `test_custom_credential_form_payload` | UI form submit → `createCredential` body has provider+base_url |
| 8.4 | e2e (acceptance) | `test_models_custom_option_select` | UI: add custom credential, pick in own-key, submit → `setTenantProviderSelfManaged` with its ref |
| 9.1 | unit | `test_install_three_paths_render` | `fleets/new` renders template grid + `owner/repo` import + paste SKILL.md affordance |
| 9.2 | unit | `test_install_experience_shared` | Dashboard card + Fleets empty-state render the same shared install component (one source) |
| 9.3 | e2e (acceptance) | `test_install_inline_state_driven` | Use template / Import / Paste-create proceed inline to the states (no review-page route); create fires with the correct source |
| 9.4 | unit | `test_install_states_render` | importing → connect-to-continue / skill-only-no-TRIGGER → creating → done; error (404/no-SKILL.md/rate-limited) shows retry |
| 9.5 | unit | `test_installing_fleet_always_visible` | an installing fleet shows its installing state in the Fleets list/detail until resolved |
| 9.6 | e2e (acceptance) | `test_install_lands_in_steer` | install done → "Open fleet" routes to the fleet's full-height steer/chat |
| 9.7 | integration | `test_install_status_stream` | post-create `install:*` SSE events advance the steps; `install:ready` flips status `installing→active` and ends install-mode |
| all-ui | e2e (acceptance) | dashboard acceptance-e2e | Billing/Models/Credentials/Install render every section dark+light; no console errors; axe green |
| all-cli | e2e (cli-acceptance) | CLI vault + provider acceptance | extended `credential-vault.spec.ts` + `tenant-provider-mutation.spec.ts` cover the openai-compatible path vs live API |

**Regression:** platform-default + named-provider routing and runs; billing pagination; credential create/delete; settings tabs; theme toggle; the egress allowlist for named providers; every existing CLI command — all unchanged except assertions tracking intentional markup. **Idempotency/replay:** re-resolving the same credential yields the same policy; a re-run against the same endpoint is not double-charged beyond existing telemetry semantics. **Integration coverage:** `cli/test/credentials.integration.test.ts` extended for the base_url credential; backend resolver→runner→fake-endpoint + allowlist integration (Failure Modes for SSRF/unreachable injected deterministically).

---

## Acceptance Criteria

- [ ] Polish (tabs/width/ink/motion) + Billing/Models/Credentials rebuilt + Clerk fix — verify: `make test-unit-app`
- [ ] Resolver extracts + SSRF-validates base_url; runner dials it; allowlist permits it — verify: `make test && make test-integration`
- [ ] Memory clean on resolver/runner path — verify: `make memleak`
- [ ] Cross-compile clean — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] CLI carries base_url end-to-end — verify: `make test-unit-cli` (or the CLI unit lane) and `make cli-acceptance`
- [ ] Dashboard acceptance e2e green (all screens, both themes) — verify: `make dry-smoke && make acceptance-e2e`
- [ ] Install experience minimal + unified across Dashboard and Fleets (3 paths + import states); **live status over the existing SSE stream** (`install:*` events + `installing→active` flip) — verify: `make test-unit-app && make test-integration && make acceptance-e2e`
- [ ] `make lint` + `make lint-app` clean · `gitleaks detect` clean · no non-md file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: UI unit + lint
make test-unit-app && make lint-app && echo "PASS" || echo "FAIL"
# E2: backend unit + integration + memleak
make test && make test-integration && make memleak 2>&1 | tail -5
# E3: cross-compile both targets
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo "XC PASS"
# E4: acceptance (dashboard + CLI, live API twins)
make acceptance-e2e && make cli-acceptance
# E5: dry-lane smoke
make dry-smoke
# E6: lint + gitleaks
make lint 2>&1 | grep -E "✓|FAIL"; gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: orphan sweeps — retired pill-tab + api_key-in-log (empty = pass)
grep -rn "data-\[state=active\]:bg-background" ui/ | head
grep -rn "api_key" src/agentsfleetd/runner --include='*.zig' | grep -i "log\|print" | head
```

---

## Dead Code Sweep

**1. Orphaned files** — `BundlePreview.tsx` is **deleted** (replaced by the inline `InstallStates`). The Models in-page Credentials tab content lives at `/credentials` already; the duplicate tab wiring is removed. New files: `base_url_guard.zig`, `InstallStates.tsx`. (The GitHub connector component + backend are M99_001, not this diff.)

**2. Orphaned references** — grep after the changes; non-zero = stale.

| Removed/renamed symbol | Grep | Expected |
|------------------------|------|----------|
| pill-tab active class | `grep -rn "data-\[state=active\]:bg-background" ui/ \| head` | 0 (underline replaces it) |
| Models page Credentials tab | `grep -rn "CREDENTIALS_TAB\|Credentials</TabsTrigger" ui/packages/app/app/\(dashboard\)/settings/models \| head` | 0 |
| `"openai-compatible"` literal | `grep -rn '"openai-compatible"' src/ ui/ cli/ \| grep -v const \| head` | only the named-constant defs + imports (RULE UFS) |

---

## Discovery (consult log)

> **Empty at creation.** Populate as work surfaces consults, skill outcomes, and any Indy-acked deferrals.

- **Authoring-time decisions (Indy, Jun 23, 2026 design-review session):** direction = "terminal-native, clean Replicas-like", branding stays; the right-side dual-tone glow is intentional, on all pages "with jazz"; the content width is a readability-cap token (`--content-max`, ≈1200px — see the post-mock-approval update); light-mode primary CTA = ink; decorative terminal corner-tags are labels, never links; Credentials kinds order = Model providers → Custom secrets → Integrations; GitHub/Zoho/Slack are "Coming soon" (not built), custom secrets bridge them now; **bundle custom endpoints into M98_001** (not a separate spec); **`base_url` rides in the saved credential JSON — no schema migration**; full support (backend + CLI + UI) this round; add integration + acceptance + CLI-acceptance coverage. Also bundle the **install-fleet experience** (§9): the Install page "looks very cluttered" → make it minimal, and unify the three install paths (template / GitHub source / paste SKILL.md) and the import states across the Dashboard and the Fleets install page (one shared component).
- **Update (Indy, Jun 23 — post-mock-approval):** mock approved. Post-install action is "**Open fleet**" — the entity is a fleet, not "teammate" (broader teammate→fleet noun alignment under review in the Install-page spar). The **steer/chat is full-height** (composer pinned, message list scrolls). Content width is a **token** (`--content-max`, ≈1200px), a readability cap not a fixed width; **chat + large data tables go full-width**. Install is **state-driven inline** (the `BundlePreview` review page is removed) and lands in the fleet's steer/chat.
- **Decouple (Indy, Jun 23, 2026):** the **GitHub connector + a config-driven credential broker** move out of this milestone to **M99_001** (Connectors + Credential Broker). Stress-testing the token lifecycle (25h gap before first event, 24h-active fleet, single run > token lifetime, steer-after-6h with no trigger) surfaced that on-demand, daemon-side minting through a broker — not a per-connector minter bolted into agentsfleetd — is required, and that it is its own subsystem. This milestone keeps the dashboard polish (§1–§5), the custom OpenAI-compatible endpoints (§6–§8), and the install UX (§9); GitHub/Zoho/Slack stay "Coming soon," bridged by custom secrets, and the §9 connect-to-continue resolves via the custom-secret bridge until M99 lands.
- **Install-page spar (Indy, Jun 23, 2026):** terminology **confirmed → `fleet`** ("Install fleet" / "creating fleet…" / "Open fleet"; "teammate" is warm prose only). The **connect-to-continue** gate follows the **template's declared needs**. **Install-status transport resolved — reuse the existing SSE fleet-event stream** (`useFleetEventStream`), **no polling and no new WebSocket**: pre-create steps render from the snapshot/create responses (**snapshot on click**, **create after the gate is satisfied**); post-create, typed `install:*` events advance the steps and `core.fleets.status` flips `installing→active` on ready; install + steer share one stream. **Create auto-proceeds — no confirm beat** (after a clean import, and the instant the gate is satisfied). Indy: *"always think about adding less friction to an end user"* — a standing design value, applied here and carried forward.
- **Open dependency to confirm at PLAN:** nullclaw OpenAI-compatible base_url override capability (see read-first note). If absent → blocking upstream item, surface to Indy.
- **Skill chain outcomes** — `/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs`: populate during VERIFY/CHORE(close).
- **Deferrals** — full per-credential fleet-usage tracking (beyond the active model credential) is **Out of Scope** here, not a silently-dropped Dimension.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification (≥50% negative; every Failure Mode covered; integration + acceptance + cli-acceptance present) | Clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `docs/DESIGN_SYSTEM.md`, `dispatch/write_zig.md`, `dispatch/write_ts_adhere_bun.md`, REST guide, Failure Modes, Invariants (esp. the SSRF guard) | Clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR | Comments addressed before human review |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| UI unit | `make test-unit-app` | {paste snippet} | |
| Backend unit | `make test` | {paste snippet} | |
| Integration | `make test-integration` | {paste snippet} | |
| Memleak | `make memleak` | {paste snippet} | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux && -Dtarget=aarch64-linux` | {paste snippet} | |
| CLI acceptance | `make cli-acceptance` | {paste snippet} | |
| Dashboard acceptance | `make acceptance-e2e` | {paste snippet} | |
| Lint | `make lint && make lint-app` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |
| api_key log audit | Eval E8 | {paste snippet} | |

---

## Out of Scope

- **GitHub/Zoho/Slack connectors + the credential broker** — **decoupled to M99_001** (Connectors + Credential Broker, Jun 23, 2026); custom secrets bridge all three until it lands. The public-repo bundle import (§9, unauthenticated) stays in this milestone.
- A typed multi-endpoint provider catalog with per-entry endpoints — future work if more endpoint-carrying providers arrive.
- Non-OpenAI wire formats (Anthropic-native, Gemini-native) for custom endpoints — only the OpenAI-compatible shape this round.
- Per-credential fleet-usage tracking beyond the active model credential — follow-up needing a read-only credential-usage query.
- Any billing-model change (tiers/seats) — consumption/prepaid stays.
- Deeper motion (number count-ups, tab-underline slide, skeleton shimmers) and credential rotation automation — revisit later.
