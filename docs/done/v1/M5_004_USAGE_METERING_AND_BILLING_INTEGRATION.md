# M5_004: Integrate Usage Metering And Billing Adapter Contract

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 004
**Date:** Mar 06, 2026
**Status:** DONE
**Priority:** P1 — commercial automation
**Batch:** B3 — needs M5_003
**Depends on:** M5_003 (Enforce Workspace Entitlements And Plan Limits)

---

## 1.0 Singular Function

**Status:** DONE

Implement one working commercial function: replay-safe usage ledger and provider-agnostic billing adapter contract.

**Dimensions:**
- 1.1 DONE Define billable units and immutable usage ledger schema
- 1.2 DONE Emit and aggregate deterministic usage events from runtime lifecycle
- 1.3 DONE Define adapter interface (`Noop`, `Manual`, provider-specific)
- 1.4 DONE Define adapter outage behavior, retry/idempotency, and secure credential handling

---

## 2.0 Verification Units

**Status:** DONE

**Dimensions:**
- 2.1 DONE Unit test: ledger replay yields identical totals
- 2.2 DONE Unit test: duplicate events do not double-charge
- 2.3 DONE Integration test: adapter outage preserves accounting state and retries safely

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 Usage ledger is deterministic and replay-safe
- [x] 3.2 Entitlement sync can consume billing state without manual edits
- [x] 3.3 Adapter outages do not corrupt usage or enforcement decisions
- [x] 3.4 Demo evidence captured for metering/replay and adapter-failure path

---

## 4.0 Out of Scope

- Customer-facing invoice dashboard
- Vendor-specific billing implementation lock-in

---

## 5.0 Implementation Notes (Mar 13, 2026)

- Added migration `schema/013_usage_metering_billing.sql` to extend `usage_ledger` with deterministic `event_key`, billable metadata, and workspace linkage.
- Added `billing_delivery_outbox` for idempotent adapter delivery with retry metadata (`status`, `delivery_attempts`, `next_retry_at`, `last_error`).
- Added runtime usage/billing module in `src/state/billing.zig`:
  - deterministic stage event writes (`ON CONFLICT (run_id, event_key) DO NOTHING`)
  - replay-safe aggregation and finalize path
  - idempotent outbox queueing per run/attempt/unit
- Added adapter contract in `src/state/billing_adapter.zig` with `noop`, `manual`, and `provider_stub` modes and secure provider key requirement.
- Added billing reconciler in `src/state/billing_reconciler.zig` with adapter delivery, outage retry behavior, and dead-letter handling for hard failures.
- Wired worker run lifecycle in `src/pipeline/worker_stage_executor.zig`:
  - per-stage deterministic usage event emission
  - `completed` attempt finalization as billable
  - failed/incomplete attempt finalization as non-billable
- Extended `zombied reconcile` path to process billing delivery via adapter contract.

---

## 6.0 Verification Evidence (Mar 13, 2026)

- `zig build test` — passed.
- `make lint` — passed (zombied + website).
- `make test` — passed (zombied, zombiectl, website, app, backend e2e lane).
- `make build` — failed due Zig 0.15.2 build-runner panic in Docker build stage (`std/Random.zig` index out of bounds), not due repository test failures.
