# M80_010: Meter agent runs incrementally at renewal — run-time fee + per-token cost

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 010
**Date:** May 30, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — billing correctness: a long platform run is currently billed at a frozen one-shot floor estimate taken at lease issue, so a slow agent leaks margin (run-time + actual tokens go uncharged) while a short estimate-heavy one overcharges. The stage debit must follow the real run.
**Categories:** API
**Batch:** B1
**Branch:** feat/m80-010-incremental-renewal-metering
**Depends on:** M80_006 §3 (the fenced `/renew` verb — this rides its dual-row reclaim/renew CTE and adds a last-metered cursor to it)
**Provenance:** LLM-drafted (Opus 4.8, May 30, 2026 — from the billing brainstorm with Indy)

> **Provenance is load-bearing.** LLM-drafted from a design conversation, not code-verified end-to-end. The implementing agent re-verifies the named symbols on `main` first — especially that M80_006's `/renew` CTE has landed (this spec extends it; it does NOT introduce renewal itself), and that `computeStageCharge`'s signature is still `(posture, model, input_tokens, output_tokens)` before widening it.

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §3 (two debit points), §4 (`computeStageCharge` shape), §13 (refund-on-actual deferral being pulled forward) + `docs/architecture/runner_fleet.md` (per-lease renewal — the heartbeat this meters off).

---

## Implementing agent — read these first

1. `src/zombied/state/tenant_billing.zig` — `computeStageChargeAt` (the time-injected core), `STAGE_PLATFORM_NANOS`, `STAGE_SELF_MANAGED_NANOS`, `EVENT_NANOS`, the free-trial cutoff branch. This is where the per-stage flat constants become a per-second run rate + a cached token tier.
2. `docs/v2/pending/M80_006_P1_API_RUNNER_FLEET_PLANE.md` §3 + `docs/architecture/runner_fleet.md` (S5 FLEET + renewal gap) — the fenced renewal CTE and `lease_expires_at` advance this rides. The metering cursor advances **in that same CTE**, not in a second statement.
3. `src/zombied/state/model_rate_cache.zig` — `ModelRate` (`input_nanos_per_mtok`, `output_nanos_per_mtok`) + `SELECT_RATES`; the cached-input tier is a third column here.
4. `docs/architecture/billing_and_provider_keys.md` §3/§4/§13 — the two-debit model, the `computeStageCharge` shape, and the §13 "refund-on-actual deferred to v3" line this pulls forward to a settle-at-report.
5. `src/lib/contract/protocol.zig` + `execution_policy.zig` — the `/renew` request body is empty today; it gains cumulative token counts. Frozen contract → additive, defaulted, backward-parseable.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** meter agent runs incrementally at renewal with a run-time fee + per-token cost
- **Intent (one sentence):** the stage debit stops being a one-shot floor estimate at lease issue and becomes an elapsed-time run fee plus actual per-token cost, metered as a delta on every `/renew` and settled on the last partial slice at report, so the credit drained equals the real runtime × rate + real tokens.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. Mismatch → STOP. Key assumptions to confirm: (a) M80_006's `/renew` CTE has landed and is the place the Δ is charged + the cursor advanced **atomically**; (b) `RUN_NANOS_PER_SEC` is **one** per-second rate identical for both postures (it replaces `STAGE_PLATFORM_NANOS` as a per-second value; `STAGE_SELF_MANAGED_NANOS` **retires entirely**); (c) the runner reports **cumulative** token counts and the server charges the **diff** since the lease's last-metered cursor, so a retry double-bills ≈0.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (the run-rate + cached-tier identifiers shared verbatim across `tenant_billing.zig` ↔ `rates.ts` ↔ `rates.mdx`; the `/renew`-body token-count field names reuse the contract identifiers verbatim runner↔zombied), **NLR** (retire `STAGE_SELF_MANAGED_NANOS` and the flat `STAGE_PLATFORM_NANOS` semantics **in place** — no `_v2` twin), **NDC** (no dead constant left after the rename — `STAGE_SELF_MANAGED_NANOS` and its pin-test references all go), **NLG** (pre-2.0: no "legacy"/"old-rate" framing in code, constant names, or prose; the one-shot estimate is removed, not renamed `legacy_*`).
- **`docs/ZIG_RULES.md`** — `*.zig` in zombied: pg-drain on the renew/report queries, tagged-union results, multi-step `errdefer`, cross-compile both linux targets, no `i64` overflow on `elapsed_ms × rate` (use the same `@divTrunc(... , 1_000_000)` mtok pattern + ms→s division as today).
- **`docs/SCHEMA_CONVENTIONS.md` §7** — pre-v2.0 is **full teardown-rebuild: no `ALTER TABLE`** (`check-schema-gate` fails it while major < 2). Add `cached_input_nanos_per_mtok` **inline** to `schema/019_model_caps.sql`, and the metering-cursor columns inline to `schema/022_fleet_runner_leases.sql` + `schema/023_fleet_runner_affinity.sql`; the slots already register in `embed.zig` + `canonicalMigrations()`.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the internal `/v1/runners/.../renew` and report bodies gain fields; envelope + route unchanged.
- **`docs/LIFECYCLE_PATTERNS.md`** — the Δ-charge + cursor-advance must be one atomic CTE; no torn read-modify-write of `metered_*`.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — `019_model_caps.sql` + `022`/`023` lease/affinity slots gain columns | **inline** column adds to the existing slot files (no `ALTER TABLE` — `check-schema-gate` fails it pre-2.0); single-concern ≤100 lines; slots already in `embed.zig` + `canonicalMigrations()`; no DROP |
| ZIG GATE | yes — `tenant_billing.zig`, `metering.zig`, renew/report paths | cross-compile x86_64/aarch64-linux; pg-drain audit on the renew/report queries; tagged-union charge result |
| UFS | yes — `RUN_NANOS_PER_SEC` + `cached_input_nanos_per_mtok` + the cumulative-token field names | one identifier each, shared verbatim Zig↔TS↔MDX and runner↔zombied; nanos-per-mtok divisor stays the existing named constant |
| CROSS-TIER RATES pin | yes — run-rate rename + `STAGE_SELF_MANAGED_NANOS` retire | `audit-cross-tier-rates.sh` pins the scalar constants across **4 code files** (`tenant_billing.zig`, `rates.ts`, `ui/packages/app/lib/types.ts`, `zombiectl/src/constants/billing.ts`); land the rename + removal in all four **and** update the script's `NAMES` array (legit rename-tracking, not gate-silencing) same PR; mirror display in `rates.mdx`; fails closed on drift |
| File & Function Length (≤350/≤50/≤70) | yes — `tenant_billing.zig` grows the cached term + `runFee` helper | factor `runFee(elapsed_ms)` + the cached-tier term so `computeStageChargeAt` stays ≤50; split a settle helper if `service_report` nears the cap |
| LIFECYCLE | yes — the Δ-charge + cursor advance is a balance mutation | the charge UPDATE and the `metered_*`/`last_metered_at_ms` advance are one CTE; `errdefer` on partial build; drain before deinit |
| ERROR REGISTRY | yes — a malformed/negative cumulative-token body needs a `UZ-*` code | declare the new code in `src/zombied/errors/*` before use; clamp-to-zero path (never a negative charge) reports it; mirror where the runner observes it |
| LOGGING | yes — renew/report emit on the metering path | logfmt with `lease_id`/`charge_type`/Δ-nanos; token **counts** are not secret (log them as audit), but never log a provider key on this path; add a log-shape assertion |

---

## Overview

**Goal (testable):** the stage debit for a platform run equals `runtime_seconds × RUN_NANOS_PER_SEC + Σ(token deltas × tier rates)` summed across every `/renew` plus a final settle; a self-managed run equals `runtime_seconds × RUN_NANOS_PER_SEC` with tokens recorded but not charged; a re-sent `/renew` double-bills ≈0 — asserted by `test_settle_sums_to_actual`, `test_self_managed_charges_run_only_records_tokens`, and `test_renew_retry_charges_near_zero`.

**Problem:** today the stage charge is a single conservative floor estimate computed once at lease issue (`computeStageCharge` over a worst-case input-token guess), then frozen — §3/§13 say actual tokens are never reconciled in v2.0. With M80_006 renewing leases, a long agent can run for minutes while it is billed as one short stage; a slow quiet model call pays nothing for the wall time it holds a runner; and a short prompt-heavy run overpays its worst-case guess. The drained credit no longer tracks the real run.

**Solution summary:** replace the one-shot stage estimate with incremental metering. On each `/renew`, the runner reports **cumulative** `(input, cached_input, output)` token counts; the server charges, inside M80_006's fenced renewal CTE, the **delta** since the lease's last-metered cursor: a run fee `(now − last_metered_at) × RUN_NANOS_PER_SEC` plus (platform only) the per-token cost of the token delta across three tiers (input / cached-input / output). The cursor (`metered_*`, `last_metered_at_ms`) advances atomically in the same CTE, so a retry charges ≈0. A final settle at `service_report` charges the last partial slice. `STAGE_PLATFORM_NANOS` becomes the per-second `RUN_NANOS_PER_SEC` (same value both postures); `STAGE_SELF_MANAGED_NANOS` retires. Tokens and wall time are recorded under **both** postures for audit; only platform is charged for tokens.

---

## Prior-Art / Reference Implementations

- **Billing math** → `computeStageChargeAt` in `src/zombied/state/tenant_billing.zig` — the existing `@divTrunc(rate × tokens, 1_000_000)` mtok pattern and the time-injected `now_ms` seam (free-trial cutoff) are mirrored exactly; the cached tier is a third `@divTrunc` term, the run fee a ms→s division.
- **Atomic cursor advance** → M80_006's renewal CTE in `src/zombied/fleet/renewal.zig` (the dual-row reclaim/renew statement) — the Δ-charge + `metered_*` advance ride it as additional `SET` columns; no second statement.
- **Token-rate cache** → `src/zombied/state/model_rate_cache.zig` `ModelRate` + `SELECT_RATES` — the cached-input column is appended exactly like the existing input/output rate columns.
- **Cross-tier pin** → `scripts/audit-cross-tier-rates.sh` already pins `STAGE_PLATFORM_NANOS`/`STAGE_SELF_MANAGED_NANOS`/`FREE_TRIAL_STAGE_NANOS` across `tenant_billing.zig` + `rates.ts` + `types.ts` + `billing.ts` — the run-rate rename reuses that identical-identifier discipline (and updates the script's `NAMES`).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/019_model_caps.sql` · `022_fleet_runner_leases.sql` · `023_fleet_runner_affinity.sql` | EDIT | inline column adds (pre-2.0, no `ALTER`): model-caps gains `cached_input_nanos_per_mtok`; lease/affinity gain `metered_input_tokens`, `metered_cached_tokens`, `metered_output_tokens`, `last_metered_at_ms` (SCHEMA GUARD) |
| `schema/0NN_fleet_metering_periods.sql` (+ `embed.zig` + `canonicalMigrations`) | CREATE | NEW per-renewal breakdown table: `(event_id, slice_seq, d_input, d_cached, d_output, run_ms, run_fee_nanos, token_cost_nanos, charged_nanos, created_at)` (SCHEMA GUARD) |
| `src/zombied/state/fleet_metering_store.zig` + `GET /v1/.../metering-periods` handler | CREATE | INSERT a metering-period row per `/renew`+settle; read endpoint returns an event's periods. **Backend only** — the React drill-down is deferred to an M81 spec |
| `src/zombied/state/model_rate_cache.zig` | EDIT | `ModelRate` + `SELECT_RATES` gain `cached_input_nanos_per_mtok` |
| `src/zombied/http/handlers/model_caps.zig` | EDIT | the model-caps response struct + endpoint shape carry `cached_input_nanos_per_mtok` |
| `src/zombied/state/tenant_billing.zig` | EDIT | `computeStageCharge` gains a `cached_input_tokens` param + cached tier; `STAGE_PLATFORM_NANOS` → per-second `RUN_NANOS_PER_SEC`; `STAGE_SELF_MANAGED_NANOS` retires; add a `runFee(elapsed_ms)` helper (ms-precision) |
| `src/zombied/zombie/metering.zig` + `src/zombied/state/tenant_billing_store.zig` | EDIT | the incremental slice debit — a **clamp** variant (`GREATEST(0, balance_nanos − slice)`, never negative) + accumulate the per-event telemetry `stage` row (`credit_deducted_nanos`/`token_count`/`wall_ms` += Δ) |
| `src/zombied/fleet/renewal.zig` + `service_renew.zig` + `src/zombied/http/handlers/runner/renew.zig` | EDIT (files **created by M80_006** — absent on `main` today) | charge run-fee + token-cost on the Δ inside the fenced CTE; advance `last_metered_*` atomically; the `/renew` body now carries cumulative token counts |
| `src/zombied/fleet/service_report.zig` | EDIT | final settle of the last partial slice at report (elapsed since last renew + final token delta) |
| `src/lib/contract/protocol.zig` | EDIT | `RenewRequest` body gains cumulative `input`/`cached_input`/`output` token counts (additive, defaulted) |
| `src/runner/**` (token accounting) | EDIT | the runner tallies cumulative tokens from the NullClaw child and reports them on `/renew` + report |
| `ui/packages/website/src/lib/rates.ts` + `ui/packages/app/lib/types.ts` + `zombiectl/src/constants/billing.ts` | EDIT | the 4-file scalar pin (with `tenant_billing.zig`): `STAGE_PLATFORM_NANOS`→`RUN_NANOS_PER_SEC`, `STAGE_SELF_MANAGED_NANOS` removed |
| `scripts/audit-cross-tier-rates.sh` | EDIT | update the `NAMES` array to the renamed/removed constants (rename-tracking, not gate-silencing) |
| `ui/packages/website/src/lib/rates.test.ts` | EDIT | retire the `STAGE_SELF_MANAGED_NANOS` assertions + the obsolete `=== … * 10n` ratio (both rates now one value) |
| `~/Projects/docs/snippets/rates.mdx` (cross-repo) | EDIT | display mirror: run-rate rename + cached-tier example strings re-derived |
| `docs/architecture/billing_and_provider_keys.md` | EDIT (human applies separately) | §3/§4/§13 rewritten for the one-meter incremental model |
| `src/zombied/state/*_test.zig`, `src/zombied/fleet/*_test.zig`, `src/zombied/http/handlers/*_test.zig` | CREATE | unit + integration per the Test Specification |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four slices — (1) cached-input pricing tier end-to-end, (2) the run-fee model (per-second rate replacing the flat stage constants), (3) incremental Δ-metering at `/renew` (the cursor + idempotency), (4) the final settle + billing-history breakdown. (1)/(2) are pure-function + schema changes testable in isolation; (3) needs M80_006's CTE; (4) closes the sum-to-actual loop.
- **Alternatives considered:** (a) keep the one-shot estimate and add a single reconciliation debit at report only — rejected: a multi-minute run still holds a runner unbilled for wall time until the very end, and a crash mid-run leaves the run fee uncharged; per-renewal metering bills as the run proceeds and is crash-resilient (each settled slice is durable). (b) charge a flat 30s per renewal — rejected: renewals fire ~every 20s (when < `RENEWAL_WINDOW_MS` of `LEASE_TTL_MS` remains), so a flat 30s/renewal overcharges ~1.5×; the elapsed-delta sums to exactly the real runtime. (c) a separate metering table — rejected: the cursor belongs on the lease row so the Δ-charge and advance stay in M80_006's single fenced CTE (no torn write, no extra join).
- **Patch-vs-refactor verdict:** a **focused refactor of the stage-debit path** — it replaces a one-shot estimate with incremental metering and retires a constant. Not a mud-patch (we do not bolt a reconciliation onto the frozen estimate); not a broad refactor (the receive debit, the gate, the credit column, and the telemetry-row shape are unchanged). The receive `EVENT_NANOS` debit is explicitly untouched (shape stability).

---

## Sections (implementation slices)

### §1 — Cached-input pricing tier, end-to-end

The token cost gains a third tier, `cached_input`, alongside input and output (Fireworks-style input / cached-input / output, e.g. retail $1.74 / $0.14 / $3.48 per 1M). `core.model_caps` and `ModelRate` gain `cached_input_nanos_per_mtok`; the model-caps endpoint exposes it; `computeStageCharge` adds the cached term. Why: cached input is materially cheaper and the platform charge must reflect it. **Implementation default:** the cached column is additive with a row-level value from the model-caps catalogue (no synthesized fallback) — a platform model missing the cached rate is a catalogue inconsistency, handled like a missing model today (loud fail at billing, not a silent default).

- **Dimension 1.1** — `computeStageCharge` with nonzero `cached_input_tokens` adds `Δcached × cached_input_nanos_per_mtok / 1e6` → Test `test_cached_tier_in_stage_charge`
- **Dimension 1.2** — the model-caps endpoint response carries `cached_input_nanos_per_mtok` → Test `test_model_caps_exposes_cached_rate`
- **Dimension 1.3** — `RUN_NANOS_PER_SEC` pins across the 4 audit-tracked rate files (`tenant_billing.zig`/`rates.ts`/`types.ts`/`billing.ts`) → Test `test_cross_tier_rate_pin_holds`

### §2 — Run-fee model: per-second rate replacing the flat stage constants

> **Status (Jun 2, 2026): code DONE — unit-verified.** `STAGE_PLATFORM_NANOS`→`RUN_NANOS_PER_SEC=100_000` (per-second) and `STAGE_SELF_MANAGED_NANOS` **retired** across all four pinned files (`tenant_billing.zig`/`rates.ts`/`types.ts`/`billing.ts`) + `audit-cross-tier-rates.sh` `NAMES` — `audit-cross-tier-rates.sh` PASS (3 constants, 4 files). `runFee(elapsed_ms)` helper added; `computeStageCharge` widened with an `elapsed_ms` param (run fee + token cost under platform, run fee only under self_managed) — it is the unit-tested reference the §3 SQL mirrors. At lease **issue** `elapsed_ms=0`, so the run fee is 0 and self_managed carries no issue-time charge; cascading metering tests reworked accordingly (block/exhaust boundaries moved to platform posture, where the token floor is non-zero). Marketing fold-in (Pricing/FAQ/Terms + their tests + Home.test + smoke) reframed to **per-second usage-based** ($0.0001/sec ≈ $0.36/hr, billed only while an agent is actively running). Green: `zig build` + `zig build test`, website 142/142, app 664/664, zombiectl 852/0. **Pending:** `rates.mdx` display mirror lands on the docs `chore/m80-010-*-changelog` branch (cross-repo); the dotfiles `audit-cross-tier-rates.sh` push is **held until merge** (Indy, Jun 2: "wait") so sibling worktrees keep a green rate gate until they rebase.

- **Dimension 2.1 — DONE** — a renewal at ~20s elapsed charges `20s × RUN_NANOS_PER_SEC`, not `30s × …` → Test `test_run_fee_is_elapsed_not_flat` (covered by inline `runFee` + `computeStageChargeAt` unit tests)
- **Dimension 2.2 — DONE** — `RUN_NANOS_PER_SEC` is identical for platform and self_managed → Test `test_run_rate_same_both_postures` (inline run-fee posture test + `rates.test.ts` pin)
- **Dimension 2.3 — DONE** — platform charge = run fee + token cost → Test `test_platform_charges_run_plus_tokens` (`tenant_billing_edge_test` platform run-fee + token-overflow tests)
- **Dimension 2.4 — DONE** — self_managed charge = run fee only; tokens recorded, not charged → Test `test_self_managed_charges_run_only_records_tokens` (inline self_managed run-fee test + `metering_test` telemetry-row test)

### §3 — Incremental Δ-metering at `/renew`: cumulative body, cursor, idempotency

The `/renew` body carries the runner's **cumulative** `(input, cached_input, output)` token counts (empty today). The server charges, inside M80_006's fenced renewal CTE, the Δ since the lease's `metered_input/cached/output` + `last_metered_at_ms` cursor, then advances the cursor in the same CTE. Why: billing follows the real run, and a fail-safe retry double-bills ≈0. **Implementation default:** the cumulative-diff is the idempotency key — a re-sent renewal a few ms later yields Δtokens≈0 and Δt≈0 → ≈0 charge; there is **no** zero-token waive gate (an active token-free slice still pays its run fee; serverless is structural — a dormant agent is never renewed, so it is never charged beyond its last settled slice).

- **Dimension 3.1** — a re-sent renewal (same cumulatives, ~ms later) charges ≈0 → Test `test_renew_retry_charges_near_zero`
- **Dimension 3.2** — a dormant agent that stops emitting is not renewed → not charged beyond its last slice (no waive rule needed) → Test `test_dormant_not_renewed_not_charged`
- **Dimension 3.3** — the `/renew` body parses cumulative token counts; a missing/malformed body defaults safely (no negative charge) → Test `test_renew_body_carries_cumulative_tokens`
- **Dimension 3.4** — a Δ that would compute negative (clock skew / token-count regression) clamps to 0, never credits → Test `test_negative_delta_never_credits`
- **Dimension 3.5** — the lease row initializes `metered_*`=0 + `last_metered_at_ms`=lease-issue time **at issue**, so the first `/renew` meters off a sane (never-NULL) cursor (pre-2.0 teardown ⇒ no in-flight rows survive a schema change) → Test `test_lease_initializes_metering_cursor`

### §4 — Final settle + per-period billing history

`service_report` charges the last partial slice (elapsed since last renew + final token delta) so the per-renewal debits + the settle sum to **exactly** total runtime × rate + total token cost — ms-precision (`floor(Σ elapsed_ms × rate / 1000)`, never per-slice second-truncation). Each renewal/settle does the **three guard-gated writes** (Interfaces): debit the wallet (clamped), accumulate the **per-event** `stage` telemetry row, and INSERT a **per-renewal** `fleet.metering_periods` row. The accumulated stage row is the headline figure the Usage tab already renders; `metering_periods` is the slice-by-slice breakdown (backend + read API this PR; the React drill-down is an M81 spec). Why: this pulls forward the §13 refund-on-actual item to a settle (no over/under after the run ends). **Implementation default:** the headline credit is one number (the accumulated stage row); the per-renewal detail is queryable from `metering_periods`.

- **Dimension 4.1** — per-renewal debits + final settle == total runtime × rate + total token cost, with **non-second-aligned** slice boundaries (ms-precision; no truncation drift) → Test `test_settle_sums_to_actual`
- **Dimension 4.2** — each renewal/settle INSERTs a `fleet.metering_periods` row (per-renewal breakdown) and the per-event `stage` telemetry row accumulates the running total → Test `test_metering_periods_breakdown_and_stage_accumulates`
- **Dimension 4.3** — a credit-exhausted slice charges the **clamped** remainder (`GREATEST(0, balance−slice)` → balance 0) and the **next** `/renew` is refused (`UZ-RUN-012`) → Test `test_credit_exhausted_clamps_then_refuses`

---

## Interfaces

```
computeStageCharge (src/zombied/state/tenant_billing.zig) — widened; params are DELTAS (the
  cumulative→delta subtraction lives in the CTE; this fn never sees cumulatives → no double-count):
  computeStageCharge(posture, model, d_input, d_cached, d_output) i64
    platform     → RUN_FEE + d_input·input/1e6 + d_cached·cached_input/1e6 + d_output·output/1e6
    self_managed → RUN_FEE                       (tokens recorded by caller, not charged here)
  runFee(elapsed_ms) i64 := @divTrunc(elapsed_ms * RUN_NANOS_PER_SEC, 1000)  -- ms-precision; i64-safe (elapsed_ms ≤ MAX_RUNTIME_MS). Per-SLICE truncation (floor(Δt/1000)) would under-bill by up to N seconds across N renewals.

scalar constants pinned by audit-cross-tier-rates.sh across tenant_billing.zig ↔ rates.ts ↔
  types.ts ↔ billing.ts (+ rates.mdx display mirror), identical identifiers:
  RUN_NANOS_PER_SEC            -- single per-second rate, BOTH postures (was STAGE_PLATFORM_NANOS, now per-second)
  FREE_TRIAL_STAGE_NANOS       -- UNCHANGED (trial zero)    EVENT_NANOS -- UNCHANGED (receive debit)
  STAGE_SELF_MANAGED_NANOS     -- REMOVED (+ drop its rates.test.ts "* 10n" ratio assertion)
ModelRate / model-caps row gains: cached_input_nanos_per_mtok (per-model DB rate, not a scalar pin)

RenewRequest body (src/lib/contract/protocol.zig — additive, defaulted to 0):
  input_tokens, cached_input_tokens, output_tokens   -- CUMULATIVE counts (not deltas)

Lease/affinity row (schema) gains the last-metered cursor:
  metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms

NEW table fleet.metering_periods (per-renewal breakdown, tracked + shown):
  event_id, slice_seq, d_input, d_cached, d_output, run_ms, run_fee_nanos,
  token_cost_nanos, charged_nanos, created_at      -- one row per /renew + settle

Per /renew (inside M80_006's fenced renewal CTE; a `guard` CTE arm = fence-held AND status=active
AND now < created_at+MAX_RUNTIME_MS gates EVERY write below — a lost/capped renewal writes none):
  Δt        := max(0, now - last_metered_at_ms(L))
  Δin/c/out := max(0, cumulative_* - metered_*(L))
  slice     := runFee(Δt) + (posture==platform ? tokenCost(Δin, Δc, Δout, model) : 0)
  -- three guard-gated writes + cursor advance, all atomic in the one CTE:
  ① WALLET     balance_nanos := GREATEST(0, balance_nanos - slice)        -- clamp, never negative
  ② LEDGER     telemetry stage row(event_id): credit_deducted_nanos += slice,
               token_count_input/output += Δ, wall_ms += Δt              -- per-EVENT total (Usage tab)
  ③ BREAKDOWN  INSERT fleet.metering_periods(event_id, slice_seq, Δin, Δcached, Δout,
               Δt, run_fee, token_cost, slice, now)                      -- per-RENEWAL detail
  cursor       metered_* := cumulative_*, last_metered_at_ms := now
Credit refusal: when the balance can no longer cover the run the NEXT /renew's gate refuses
  (UZ-RUN-012, run terminates); the consumed slice was already clamp-charged down to 0.
At report: a final settle slice (now - last_metered_at_ms, final Δtokens) runs the same three writes.

No change to: the receive debit, the gate, balance_nanos, the telemetry-row shape (only new charge_type periods).
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Re-sent renewal (fail-safe retry) | network retry resends the same cumulatives ms later | Δtokens≈0, Δt≈0 → ≈0 charge; no double-bill (cumulative-diff idempotency) → `test_renew_retry_charges_near_zero` |
| Malformed / missing token body | runner sends a `/renew` with no/garbled counts | counts default to 0; charge is run-fee only (no negative, no panic); reports the new `UZ-*` code → `test_renew_body_carries_cumulative_tokens` |
| Negative delta | clock skew (Δt<0) or token-count regression (Δtokens<0) | clamp each Δ to 0; never credit the balance → `test_negative_delta_never_credits` |
| Dormant agent | agent stops emitting; lease not renewed | lease expires (M80_006), never renewed → never charged beyond its last settled slice → `test_dormant_not_renewed_not_charged` |
| Active token-free slice | long quiet model call, no new tokens | run fee still applies for the wall time held; no zero-token waive → asserted within `test_run_fee_is_elapsed_not_flat` |
| Credit exhausted mid-run | incremental Δ-debit drains `balance_nanos` across renewals | the slice is **clamp-charged** (`GREATEST(0, balance−slice)` → balance 0, partial); M80_006's `/renew` credit gate refuses the **next** renewal (`UZ-RUN-012`) → lease not renewed → run terminates at expiry → `test_credit_exhausted_clamps_then_refuses` |
| Fenced-out / lost renewal | reclaimed by another holder between load and extend | the `guard` CTE arm fails → none of the three writes fire (no debit, no ledger, no metering_periods row); 409 `UZ-RUN-011` → `test_lost_renewal_charges_nothing` |
| Platform model missing cached rate | catalogue row lacks `cached_input_nanos_per_mtok` | loud fail at billing (as a missing-model today), not a silent default → `test_cached_tier_in_stage_charge` (negative path: absent rate → caught) |
| Crash mid-run after a renewal | `zombied` dies between renewals | each settled slice is durable (charge + cursor committed in the CTE); on reclaim the next holder meters forward from the cursor → `test_settle_sums_to_actual` (crash-resume variant) |
| Old runner sends an empty `/renew` body | mixed-version deploy | additive defaulted fields parse to 0 → run-fee-only metering; backward-parseable → `test_renew_body_carries_cumulative_tokens` |

---

## Invariants

1. The per-renewal debits + the final settle sum to exactly `floor(total_active_runtime_ms × RUN_NANOS_PER_SEC / 1000) + total_token_cost` — **ms-precision** (per-slice second-truncation would under-bill); enforced by elapsed-delta accumulation (`now − last_metered_at`) + `test_settle_sums_to_actual` with non-second-aligned boundaries.
2. `RUN_NANOS_PER_SEC` is one value applied identically to both postures — enforced by a single constant consumed in both switch arms + `test_run_rate_same_both_postures`.
3. A re-sent renewal cannot double-bill — enforced by charging the cumulative **diff** against the lease cursor + advancing the cursor in the same fenced CTE + `test_renew_retry_charges_near_zero`.
4. A charge is never negative and never credits the balance — enforced by `max(0, Δ)` clamps on Δt and each Δtoken + `test_negative_delta_never_credits`.
5. self_managed is never charged for tokens but always records them — enforced by the posture switch gating only the token term while the telemetry write records counts under both arms + `test_self_managed_charges_run_only_records_tokens`.
6. The cached + run-rate constants are identical across the three rate files — enforced by the CROSS-TIER RATES pin test + `test_cross_tier_rate_pin_holds`.
7. The receive `EVENT_NANOS` debit is unchanged; the per-event `stage` telemetry row is **accumulated** (not multiplied — `UNIQUE (event_id, charge_type)` holds) and the per-renewal breakdown lives in the new `fleet.metering_periods` table — enforced by leaving `computeReceiveCharge` untouched + `test_metering_periods_breakdown_and_stage_accumulates` + the M80_002 receive regression staying green.
8. An incremental debit never drives `balance_nanos` below zero — enforced by clamping the debit to the remaining balance + M80_006's `/renew` credit gate refusing the next renewal (`UZ-RUN-012`) + `test_renew_refused_when_credit_exhausted`.
9. The metering cursor is initialized at lease issue and never read NULL — enforced by the lease-issue write setting `metered_*`=0 / `last_metered_at_ms`=issue-time + `test_lease_initializes_metering_cursor` (pre-2.0 teardown ⇒ no cross-schema-change rows to backfill).
10. The wallet debit never drives `balance_nanos` below zero — enforced by the `GREATEST(0, balance_nanos − slice)` clamp + the `/renew` credit gate refusing the next renewal (`UZ-RUN-012`) + `test_credit_exhausted_clamps_then_refuses`.
11. During the free-trial window the metered charge (run fee + tokens) is **zero**, identical to the existing stage short-circuit — enforced by the shared `computeStageChargeAt` `now_ms < FREE_TRIAL_END_MS` branch + metering tests pinned to `POST_TRIAL_NOW_MS` (plus one in-trial assertion that a metered run charges 0).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_cached_tier_in_stage_charge` | platform, model with all three rates, Δcached=500 → charge includes `500 × cached_rate / 1e6`; absent cached rate → loud fail (negative path) |
| 1.2 | integration | `test_model_caps_exposes_cached_rate` | `GET model-caps` → each row carries `cached_input_nanos_per_mtok` |
| 1.3 | unit | `test_cross_tier_rate_pin_holds` | `RUN_NANOS_PER_SEC` identical across the 4 pinned files (`tenant_billing.zig`/`rates.ts`/`types.ts`/`billing.ts`); `audit-cross-tier-rates.sh` green |
| 2.1 | unit | `test_run_fee_is_elapsed_not_flat` | Δt=20_000ms → `20 × RUN_NANOS_PER_SEC`, not `30 × …`; a token-free active slice still pays the run fee |
| 2.2 | unit | `test_run_rate_same_both_postures` | run fee for identical Δt equal under platform and self_managed |
| 2.3 | unit | `test_platform_charges_run_plus_tokens` | platform, Δt + nonzero Δtokens → run fee + token cost across the three tiers |
| 2.4 | unit | `test_self_managed_charges_run_only_records_tokens` | self_managed, Δt + nonzero Δtokens → charge == run fee only; telemetry still records the token counts |
| 3.1 | integration | `test_renew_retry_charges_near_zero` | two `/renew`s with the same cumulatives ms apart → second charges ≈0 |
| 3.2 | integration | `test_dormant_not_renewed_not_charged` | agent stops emitting → no renewal fires → no charge beyond the last settled slice |
| 3.3 | unit | `test_renew_body_carries_cumulative_tokens` | well-formed body → parsed cumulatives; missing/malformed → defaults to 0, run-fee-only, `UZ-*` reported, no negative |
| 3.4 | unit | `test_negative_delta_never_credits` | Δt<0 (skew) or cumulative<metered → each Δ clamps to 0; balance never credited |
| 3.5 | integration | `test_lease_initializes_metering_cursor` | a freshly issued lease → `metered_*`=0, `last_metered_at_ms`=issue time; first `/renew` meters a finite, non-negative Δ |
| 4.1 | integration | `test_settle_sums_to_actual` | a run over N renewals + settle with **non-second-aligned** boundaries → Σ debits == `floor(Σms×rate/1000)` + total token cost (incl. a crash-resume variant) |
| 4.2 | integration | `test_metering_periods_breakdown_and_stage_accumulates` | after a metered run → one `fleet.metering_periods` row per renewal/settle; the per-event `stage` telemetry row == the accumulated total |
| 4.3 | integration | `test_credit_exhausted_clamps_then_refuses` | balance below the next slice → slice clamp-charged (`GREATEST(0,…)` → 0); the **next** `/renew` refused (`UZ-RUN-012`), run terminates |
| FM | integration | `test_lost_renewal_charges_nothing` | a fenced-out/reclaimed renewal → 409 `UZ-RUN-011`; no debit, no ledger accumulate, no `metering_periods` row |
| FM | unit | `test_free_trial_meters_zero` | a metered run with `now_ms < FREE_TRIAL_END_MS` → charge 0 (run fee + tokens both zeroed); other metering tests pinned to `POST_TRIAL_NOW_MS` |

Regression: M80_002's receive-debit + telemetry-row + fencing tests and M80_006's renewal/reclaim/fencing tests stay green (this adds `SET` columns to the CTE; it must not alter fencing or row-equivalence). Idempotency/replay: `test_renew_retry_charges_near_zero` + `test_negative_delta_never_credits` cover the retry + skew paths. Non-self-evident token-count inputs come from a fixture (`samples/fixtures/m80-metering/cumulative-tokens.json`); don't inline.

---

## Acceptance Criteria

- [ ] A platform run's drained credit == runtime × rate + token cost; self_managed == runtime × rate (tokens recorded) — verify: `test_settle_sums_to_actual` + `test_self_managed_charges_run_only_records_tokens`
- [ ] A renewal charges elapsed time, not a flat 30s; the run rate is one value both postures — verify: `test_run_fee_is_elapsed_not_flat` + `test_run_rate_same_both_postures`
- [ ] A re-sent renewal does not double-bill; a negative Δ never credits — verify: `test_renew_retry_charges_near_zero` + `test_negative_delta_never_credits`
- [ ] The cached tier is priced, exposed by model-caps, and pinned across the rate files — verify: `test_cached_tier_in_stage_charge` + `test_model_caps_exposes_cached_rate` + `test_cross_tier_rate_pin_holds`
- [ ] `make lint` clean · `make test` passes · `make test-integration` passes · `make check-pg-drain` clean
- [ ] `make memleak` clean over the renew/report path · Cross-compile both linux targets · `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: metering — make test-integration 2>&1 | grep -E "settle_sums|renew_retry|run_fee|cached_tier|PASS|FAIL"
# E2: Build  — zig build
# E3: Tests  — make test && make test-integration
# E4: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=x86_64-linux 2>&1 | tail -3 && zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: pg-drain — make check-pg-drain 2>&1 | tail -3
# E8: Orphan sweep (empty = pass) — grep -rn "STAGE_SELF_MANAGED_NANOS\|STAGE_PLATFORM_NANOS" src/ ui/ zombiectl/src/ scripts/
```

---

## Dead Code Sweep

**1. Orphaned files** — N/A — no files deleted.

**2. Orphaned references** — `STAGE_SELF_MANAGED_NANOS` is removed; `STAGE_PLATFORM_NANOS` → per-second `RUN_NANOS_PER_SEC` (semantics change, not a twin). Confirmed carriers on `main` (all swept/renamed same PR): `tenant_billing.zig`, `tenant_billing_test.zig`, `metering_test.zig`, `rates.ts`, `rates.test.ts` (incl. the now-obsolete `* 10n` ratio), `types.ts`, `billing.ts`, plus `rates.mdx` + the `audit-cross-tier-rates.sh` `NAMES` array. Do **not** exclude `_test`.

| Deleted/renamed symbol | Grep (do NOT exclude _test) | Expected |
|------------------------|------|----------|
| `STAGE_SELF_MANAGED_NANOS` | `grep -rn STAGE_SELF_MANAGED_NANOS src/ ui/ zombiectl/src/ scripts/ ~/Projects/docs/snippets/rates.mdx` | 0 matches |
| `STAGE_PLATFORM_NANOS` | `grep -rn STAGE_PLATFORM_NANOS src/ ui/ zombiectl/src/ scripts/ ~/Projects/docs/snippets/rates.mdx` | 0 (renamed) |

---

## Discovery (consult log)

> **Empty at creation.** Append consults, skill-chain outcomes, and Indy-acked deferral quotes as work proceeds.

- **Dependency (May 30, 2026):** this spec extends M80_006 §3's fenced `/renew` CTE — it does NOT introduce renewal. If M80_006 has not landed when this starts, that is a blocking dependency, not a thing to re-implement here. The cursor columns ride the same CTE M80_006 owns.
- **Model decision (May 30, 2026):** `STAGE_PLATFORM_NANOS` is reinterpreted as a per-second rate and renamed `RUN_NANOS_PER_SEC`; `STAGE_SELF_MANAGED_NANOS` retires (one run rate, both postures).
- **Rate decisions — Indy (May 31, 2026):** `RUN_NANOS_PER_SEC` = **100_000 nanos** ($0.0001/sec = $0.36/hr) — Indy: "use 0.0001/sec (I will change it later)". The `rates.ts`/`rates.mdx` display reads **$/hr** (`RATE × 3600 / NANOS_PER_USD`), not a unit-less per-second string. `cached_input_nanos_per_mtok` is **per-model, configured in `core.model_caps`** (not a global constant, not in the 4-file scalar pin) — e.g. DeepSeek v4 Pro carries $0.14/1M cached; the platform-default model's row supplies its own. Resolved at billing time via the model-caps cache.
- **Storage decision — Indy (May 31, 2026):** three layers — wallet debit (clamped) + per-event `stage` ledger accumulate + NEW `fleet.metering_periods` per-renewal breakdown. Indy wants the breakdown **tracked + shown**: the **backend** (table + read API) is this spec; the **React drill-down UI is deferred to an M81 spec** (design-shotgun → design-review, alongside e2e). The per-event total remains what the existing Usage tab renders.
- **Adversarial review — PR #354 (May 31, 2026):** the 4-lens review caught and this spec now folds: the telemetry `UNIQUE(event_id,charge_type)` collision (→ `metering_periods` + accumulate, not N stage rows); all-or-nothing debit vs the spec's clamp (→ clamp `GREATEST(0,…)`, Indy); per-slice second-truncation (→ ms-precision `floor(Σms×rate/1000)`); the vacuous free-trial test (→ pin `POST_TRIAL_NOW_MS`); the cross-schema debit arm (→ gate on `FROM guard`).
- **§13 pull-forward (May 30, 2026):** the settle-at-report supersedes `billing_and_provider_keys.md` §13's "refund-on-actual deferred to v3" line; the doc edit (§3/§4/§13) lands alongside this spec on the M80_006 fleet branch.
- **SCOPE EXPANSION — Indy (Jun 01–02, 2026):** while reviewing the §1 `cached_input` schema edit, Indy redirected into a **model-catalogue + provider redesign**, chosen to land **inline in this PR** (not split) after the blast-radius was surfaced twice:
  - **`provider` column + composite `(provider, model_id)` PK** on `core.model_caps` (Indy: "Add provider column"). Forced by real collisions: Pioneer's API serves bare ids (`claude-opus-4-8`, `claude-sonnet-4-6`, …) identical to Anthropic-direct's. Rate cache rekeyed to `(provider, model)`; `provider` threaded end-to-end (model_caps endpoint → `computeStageCharge` → lease schema 022 → renew/report credit gates → tenant-provider validation).
  - **Catalogue refresh** (Indy: "opus-4.8 now", "kimi2.7?", "remove glm-5.1", "MiniMax", "Pioneer"). Researched real pricing (Opus 4.8 **$5/$25**, down from 4-7's $15/$75; Kimi K2.7 not released → kept K2.6 at corrected $0.95/$4.00; MiniMax "2.3" → **M3** standard $0.60/$2.40, 1M ctx; glm-5.1 removed). Pioneer seeded from its `llms.txt` + pricing screenshots: opus-4-8/sonnet-4-6/haiku-4-5 + `moonshotai/Kimi-K2.6`. **Pioneer DeepSeek dropped** (Indy: "pioneer doesnt have deepseek-v4-pro") — catalog (V3.1) and pricing UI (V4) disagreed.
  - **Marketing display fold-in** (Indy: "Fold in, you give copy"): the per-stage→per-hour change makes `Pricing.tsx`/`FAQ`/`Terms` copy a customer-facing lie; those components join Files-Changed. **Self-managed = same run fee, intended** (Indy). Copy strings authored with Indy — pending.
  - **cached default convention:** 10% of input where a provider publishes no cache rate; 0 for self-managed-only (zero-rate) rows.
- **Foundation checkpoint (Jun 02, 2026):** composite-key + provider-threading + cached tier landed as a standalone commit (production + tests compile green; 7 lease-INSERT fixtures + billing/metering test signatures updated). The §2 run-fee reshape (`RUN_NANOS_PER_SEC`) + 4-file pin and §3/§4 metering mechanism build ON this foundation in subsequent commits — the `STAGE_*` constants are deliberately untouched here, so `audit-cross-tier-rates.sh` stays green until §2.
- **§3 cursor-placement decision — Indy (Jun 02, 2026):** the spec listed the metering cursor on **both** `022_fleet_runner_leases.sql` and `023_fleet_runner_affinity.sql`. I first proposed reconciling to `022`-only (lease row + `PriorLease` copy-forward on reclaim, since reclaim issues a fresh lease row). **Indy: "keep cursor on 023 too."** Resolved design — the **affinity slot (`023`) is the durable per-zombie cursor anchor** (it survives a reclaim natively: the slot persists while the dead holder's lease row is marked `expired` and a fresh lease row is issued); the **lease row (`022`) cursor is the working copy** the fenced renewal CTE reads in `probe` and advances. Mechanism: (a) `affinity.claim`'s INSERT seeds the cursor `0`/now for a brand-new zombie, and its `ON CONFLICT` UPDATE **preserves** the cursor (never resets — so a reclaim keeps the prior run's value); (b) `insertLeaseRow` on a **fresh** lease resets the affinity cursor to `0`/now (new event) and seeds the lease cursor `0`/now; on a **reclaim** it seeds the lease cursor **from the surviving affinity cursor** (meter forward, no gap/double-charge); (c) the renewal CTE + report settle advance **both** the lease and affinity cursors atomically. So `023` is load-bearing (the reclaim seed source), not a dead mirror. **Foundation landed (`fe2d47a0`, green):** `RenewRequest` body + 2 parse tests, the four `022` cursor columns + header doc, `insertLeaseRow` fresh-seed, 7 fixtures. **Reworking to add `023` + the affinity-anchored seeding next**, then the Δ-charge CTE + settle + Dimension tests.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | audits coverage vs this Test Specification (esp. the sum-to-actual, retry-idempotency, and negative-Δ invariants) | clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | adversarial review vs the billing-doc model, the money invariants, the M80_006 CTE atomicity, ZIG_RULES | clean OR every finding dispositioned |
| After `gh pr create` | `/review-pr` | review-comments the open PR | comments addressed before merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit (billing math) | `make test` | {paste at VERIFY} | |
| Integration (metering) | `make test-integration` | {paste at VERIFY} | |
| Memleak (renew/report) | `make memleak` | {paste at VERIFY} | |
| pg-drain | `make check-pg-drain` | {paste at VERIFY} | |
| Lint | `make lint` | {paste at VERIFY} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste at VERIFY} | |
| Rate pin | `make test 2>&1 | grep rate_pin` | {paste at VERIFY} | |

---

## Out of Scope

- **The receive (`EVENT_NANOS`) debit** — unchanged; only the stage debit transforms. Any receive-rate change is a separate spec.
- **Stripe Purchase Credits / Auto Top Up** — v2.1 (`billing_and_provider_keys.md` §13), unrelated to how the stage is metered.
- **agentsfleet.net vs usezombie.com surface** — branding only, never a billing axis; no per-surface rate is introduced.
- **Per-workspace soft caps / volume discounts** — v3 (§13), separate gate layer.
- **Introducing lease renewal itself** — M80_006 §3 owns it; this rides it.
