# M1_001: Autoforecaster

**Version:** v2
**Milestone:** M1
**Workstream:** 001
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** Grafana Cloud observability pipeline
**Batch:** B1 — can parallel with M1_002, M1_004

---

## Problem

Worker scaling decisions are manual. There is no automated signal for when to add or remove capacity.

## Decision

Build an autoforecaster agent that reads queue depth, run duration trends, and time-of-day patterns from Grafana metrics and emits a scaling recommendation (add/remove N workers).

---

## 1.0 Demand Signal Ingestion

**Status:** PENDING

**Dimensions:**
- 1.1 PENDING Read Redis stream queue depth (pending messages per consumer group)
- 1.2 PENDING Read run duration trends (p50, p95, p99 from Grafana)
- 1.3 PENDING Read time-of-day patterns (historical load curves)
- 1.4 PENDING Read current worker count and health status

---

## 2.0 Scaling Recommendation

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Define recommendation format: `{ action: scale_up | scale_down | hold, count: N, reason: string }`
- 2.2 PENDING Scale-up threshold: queue depth > N for sustained period (configurable)
- 2.3 PENDING Scale-down threshold: idle workers for sustained period (configurable)
- 2.4 PENDING Emit recommendation to autoprocurer (M1_003) via control plane

---

## 3.0 Acceptance Criteria

- [ ] 3.1 Agent reads live Grafana metrics
- [ ] 3.2 Recommendation emitted within configured evaluation interval
- [ ] 3.3 Recommendation logged and auditable
- [ ] 3.4 No false scale-up on transient spikes (debounce)
