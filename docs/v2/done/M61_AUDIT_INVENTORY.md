# M61 Audit — machine-readable inventory of removal candidates

**Audit date:** May 04, 2026
**Auditor:** automated forensic pass (production-only `@import` closure for Zig; `from "..."` import grep for TS/TSX; `rg`-based name sweep for SQL).
**Inputs:** `cat VERSION` = `0.33.0` (pre-v2.0); prior sweeps M53/M54/M56 already retired the obvious orphans.
**Owning specs:** M61_001 (firewall + otel-export Zig), M61_002 (small Zig orphans), M61_003 (SQL schema), M61_004 (UI).

The previous Zig audit run (background sub-agent, May 04) returned "zero orphans" — that was wrong. It walked the unioned closure (test bridge + production), under which every file imported by `src/main.zig`'s `test {}` block looks reachable. The correct lens for runtime / binary-size questions is the **production-only** closure (entries: `src/main.zig`, `src/executor/main.zig`; `test "..." { ... }` and `test { ... }` blocks stripped before extracting `@import`s). This inventory is the result of that lens plus the SQL/TS audits.

---

## Summary

| Layer | Confirmed orphans (LOC) | Probable orphans (LOC) | Owner spec |
|-------|-------------------------|------------------------|------------|
| Zig (firewall + otel-export) | 8 source files + 5 tests · ~2460 LOC | 0 | M61_001 |
| Zig (small clusters) | 9 source files + 1 conditional test fixture · ~1200 LOC | `webhook_test_signers.zig` (215) — keep iff a test still imports it | M61_002 |
| SQL schema | 1 table + 2 indexes + 2 triggers + 1 fn (003); 1 column (004); 1 table + 1 fn + 1 view (005) · ~130 LOC | 0 | M61_003 |
| UI | 3 source files (`card.tsx`, `AnalyticsPageEvent.tsx`, `TrackedAnchor.tsx`) + 10 named DS re-exports · ~92 LOC + index pruning | ~58 type-alias DS exports (low-value, deferred) | M61_004 |
| **Total confirmed** | **~3880 LOC** | | |

Biggest single lever: M61_001 (firewall + otel-export, ~2460 LOC).
Highest-confidence: M61_003 (schema — every name has zero refs anywhere).
Lowest-risk: M61_004 (UI — the playwright smoke and typecheck catch any missed importer).

Decision points that block auto-execution:
- M61_001 §1 — retiring the M6_001 P0 firewall engine is a Legacy-Design Consult Guard trigger; needs Captain A/B/C answer.
- M61_002 §3 — `webhook_test_signers.zig` keep-or-delete depends on a `_test.zig` import check.
- M61_003 §4 — whether migration 005 becomes empty depends on what else lives in the file.

---

## Inventory — Zig (production-only closure misses)

| File | LOC | Sole `@import` | Spec | Confidence |
|------|-----|----------------|------|------------|
| src/zombie/firewall/firewall.zig | 140 | src/main.zig test{} | M61_001 | HIGH |
| src/zombie/firewall/domain_policy.zig | 66 | src/main.zig test{} | M61_001 | HIGH |
| src/zombie/firewall/endpoint_policy.zig | 243 | src/main.zig test{} | M61_001 | HIGH |
| src/zombie/firewall/injection_detector.zig | 182 | src/main.zig test{} | M61_001 | HIGH |
| src/zombie/firewall/content_scanner.zig | 284 | src/main.zig test{} | M61_001 | HIGH |
| src/zombie/firewall/firewall_test.zig | 110 | src/main.zig test{} | M61_001 | HIGH |
| src/zombie/firewall/firewall_robustness_test.zig | 341 | src/main.zig test{} | M61_001 | HIGH |
| src/zombie/firewall/firewall_greptile_test.zig | 122 | src/main.zig test{} | M61_001 | HIGH |
| src/observability/otel_export.zig | 331 | src/main.zig test{} | M61_001 | HIGH |
| src/observability/otel_histogram.zig | 263 | otel_export.zig only | M61_001 | HIGH |
| src/observability/otel_json.zig | 170 | otel_export.zig + otel_histogram.zig only | M61_001 | HIGH |
| src/observability/otel_export_test.zig | 86 | (sibling test) | M61_001 | HIGH |
| src/observability/otel_histogram_test.zig | 122 | (sibling test) | M61_001 | HIGH |
| src/auth/clerk.zig | 82 | src/auth/tests.zig only | M61_002 | HIGH |
| src/state/workspace_integrations.zig | 114 | src/main.zig test{} | M61_002 | HIGH |
| src/secrets/crypto.zig | 30 | src/main.zig test{} | M61_002 | HIGH |
| src/sys/error.zig | 195 | src/main.zig test{} (and sys/errno.zig) | M61_002 | HIGH (decision §2 needed for error-registry coverage) |
| src/sys/errno.zig | 259 | sys/error.zig only | M61_002 | HIGH |
| src/util/strings/smol_str.zig | 294 | src/main.zig test{} | M61_002 | HIGH |
| src/util/strings/case_insensitive_ascii_map.zig | 124 | src/main.zig test{} | M61_002 | HIGH |
| src/http/route_manifest.zig | 111 | NONE — zero `@import`s | M61_002 | HIGH |
| src/http/handlers/workspaces/mod.zig | 8 | (empty mod.zig shim) | M61_002 | HIGH |
| src/http/webhook_test_signers.zig | 215 | src/main.zig test{} (test fixture?) | M61_002 | DECISION-NEEDED — verify `_test.zig` consumers |

---

## Inventory — SQL schema (zero refs anywhere)

| Object | File | Lines | Refs in src/ | Spec | Confidence |
|--------|------|-------|--------------|------|------------|
| core.prompt_lifecycle_events (table) | schema/003_rls_tenant_isolation.sql | 3-14 | 0 | M61_003 | HIGH |
| idx_prompt_lifecycle_events_workspace_time | 003 | (with table) | 0 | M61_003 | HIGH |
| idx_prompt_lifecycle_events_tenant_time | 003 | (with table) | 0 | M61_003 | HIGH |
| trg_prompt_lifecycle_events_no_update | 003 | (with table) | 0 | M61_003 | HIGH |
| trg_prompt_lifecycle_events_no_delete | 003 | (with table) | 0 | M61_003 | HIGH |
| reject_prompt_lifecycle_event_mutation() | 003 | 20-25 | 0 | M61_003 | HIGH |
| audit.ops_ro_access_events (table) | schema/005_…_context_injection.sql | 4-12 | 0 | M61_003 | HIGH |
| audit.log_ops_ro_access() | 005 | 14 | 0 (only the view body calls it) | M61_003 | HIGH |
| ops_ro.workspace_overview (view) | 005 | 48 | 1 (src/db/pool_test.zig — RLS test) | M61_003 | HIGH (test deletes with view) |
| billing.workspace_entitlements.enable_score_context_injection (column) | schema/004_workspace_entitlements.sql | 12 | 0 | M61_003 | HIGH |

---

## Inventory — TS/TSX

| Item | Location | Why orphan | Spec | Confidence |
|------|----------|------------|------|------------|
| `Card` family (local duplicate) | ui/packages/app/components/ui/card.tsx | Zero importers; duplicates `@usezombie/design-system`'s Card; M26_001b unification leftover | M61_004 | HIGH |
| `AnalyticsPageEvent` component | ui/packages/app/components/analytics/AnalyticsPageEvent.tsx | Imported only by tests, not by any page | M61_004 | HIGH |
| `TrackedAnchor` component | ui/packages/app/components/analytics/TrackedAnchor.tsx | Imported only by tests | M61_004 | HIGH |
| `DialogPortal`, `DialogClose`, `DialogOverlay` (re-exports) | ui/packages/design-system/src/index.ts | Zero importers in app/website | M61_004 | HIGH |
| `SelectGroup`, `SelectLabel`, `SelectSeparator` | ui/packages/design-system/src/index.ts | Zero importers | M61_004 | HIGH |
| `DropdownMenuGroup`, `DropdownMenuPortal`, `DropdownMenuRadioGroup`, `DropdownMenuSub` | ui/packages/design-system/src/index.ts | Zero importers | M61_004 | HIGH |
| ~50 `*Props`/`*Variant` type-alias exports | ui/packages/design-system/src/index.ts | Type-only; zero importers | (deferred) | LOW value — out of scope |

---

## Eval — production-only orphan walk

After all four M61_* specs land, the production-only `@import` closure under `src/` must contain zero un-exempt files. The walk: starting from `src/main.zig` and `src/executor/main.zig`, transitively follow `@import("...")` edges **after stripping every `test "..." { ... }` / `test { ... }` block**, then list any `*.zig` under `src/` outside the closure (excluding `vendor/`, `third_party/`, `.zig-cache/`, `*_test.zig`, `test_*.zig`, `*_harness*.zig`, `*fixture*.zig`, `src/zbench_*.zig`, `src/auth/tests.zig`).

Run the eval by hand or via a one-shot Python invocation matching the algorithm above when a future audit pass is needed. Tested codification of the script as a `make lint` gate is **out of scope for this inventory** — the surviving completion signal is the four M61_001..004 sweeps having actually deleted the files they listed (verified at M42_002-era close).

---

## What we did not chase

- **`*Props`/`*Variant` type-alias DS exports** — 50ish; runtime cost zero; deferred.
- **`@embedFile` paths** — repo grep shows none of these point at deletion candidates; not separately audited.
- **Index optimization** — SQL audit found no redundant indexes on live tables.
- **Backend HTTP endpoints with no client** — quick scan didn't surface any; the dashboard + zombiectl together cover the route table. Worth a deeper pass post-launch if cross-layer drift is suspected.
- **Test-only fixtures** — left alone unless they're orphan AND the fixture's only consumer is also in the deletion set. `src/db/test_fixtures.zig` is live; `webhook_test_signers.zig` is the only borderline (M61_002 §3).
- **`src/util/`, `src/types/`, `src/cmd/`, `src/zombie/event_loop_*`** — densely interconnected; production-closure walk pulls them in. No orphans surfaced.
- **`zombiectl/` commands** — every command in `src/program/command-registry.js` resolves to a wired handler module; no orphans. (TS audit confirmed.)

---

## Suggested execution order

1. M61_003 (SQL) — highest-confidence, smallest diff, no decision points beyond migration 005 emptiness.
2. M61_002 (small Zig clusters) — second-highest confidence; hits the eval script's long tail.
3. M61_001 (firewall + otel-export) — biggest LOC win, gated by Captain A/B/C decision on the firewall engine. OTEL push-export piece can ship in parallel even if firewall decision stalls.
4. M61_004 (UI) — independent of 1-3; can interleave anywhere.
