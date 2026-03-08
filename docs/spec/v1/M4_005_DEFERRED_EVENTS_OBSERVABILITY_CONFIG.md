# M4_005: Harden Deferred Events, Observability, And Config Hygiene

**Prototype:** v1.0.0
**Milestone:** M4
**Workstream:** 005
**Date:** Mar 06, 2026
**Status:** PENDING
**Priority:** P2 — deferred hardening
**Batch:** B3 — needs M4_007
**Depends on:** M4_007 (Define Runtime, Observability, And Config Contracts)

---

## 1.0 Singular Function

**Status:** PENDING

Implement one working hardening function for deferred D4/D8/D19/D20 runtime concerns.

**Dimensions:**
- 1.1 PENDING Add durable event persistence/replay boundary
- 1.2 PENDING Add canonical trace context model
- 1.3 PENDING Add OTEL-friendly export path without Prometheus regression
- 1.4 PENDING Add key-versioned config/secret envelope and rotation verification

---

## 2.0 Verification Units

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Unit test: replay model survives restart without duplicate side effects
- 2.2 PENDING Unit test: trace fields are present across HTTP/worker paths
- 2.3 PENDING Unit test: key rotation path preserves decryptability during transition

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 Deferred hardening dimensions are implemented and test-backed
- [ ] 3.2 Runtime observability and config hygiene stay deterministic under failure
- [ ] 3.3 Demo evidence captured for replay + trace + rotation checks

---

## 4.0 Out of Scope

- Full distributed tracing backend operations runbook
- Dashboard/UI observability features
