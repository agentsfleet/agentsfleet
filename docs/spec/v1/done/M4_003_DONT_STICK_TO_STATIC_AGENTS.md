# M4_003: Dynamic Agent Topology Runtime Baseline (Don’t Stick To Static Agents)

> Superseded by docs/spec/v1/M5_002_HARNESS_CONTROL_PLANE.md

**Prototype:** v1.0.0
**Milestone:** M4
**Workstream:** 003
**Date:** Mar 05, 2026
**Status:** SUPERSEDED (stale spec)
**Priority:** P0 — start before CLI freeze
**Depends on:** M3_001 reliability hardening baseline

---

## 1.0 Topology Core

**Status:** DONE

Replace hard-coded role order in worker control flow with a deterministic, profile-driven stage list.

**Dimensions:**
- 1.1 DONE Role registry lookup API added in `src/pipeline/agents.zig` (`lookupRole`, `runByRole`)
- 1.2 DONE Profile loader added in `src/pipeline/topology.zig` with validation (first stage echo, final gate warden)
- 1.3 DONE Worker execution now reads pipeline profile and iterates build/gate stages from config
- 1.4 DONE Support non-built-in role skills beyond echo/scout/warden

---

## 2.0 Compatibility and Runtime Behavior

**Status:** DONE

Preserve default v1 behavior while enabling extension through config changes.

**Dimensions:**
- 2.1 DONE Default profile file added at `config/pipeline-default.json` and used by runtime (`PIPELINE_PROFILE_PATH`)
- 2.2 DONE Stage-aware run logs/events now include `stage_id` and `role_id`
- 2.3 DONE Integration coverage added for custom profile parsing and default topology role resolution
- 2.4 DONE Stage-level transition graph (`on_pass`/`on_fail`) metadata execution

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 Pipeline definition is config-driven, not hard-coded to 3 role calls
- [x] 3.2 Default profile reproduces current Echo → Scout → Warden flow
- [x] 3.3 Adding a new middle stage in profile does not require worker loop rewrite
- [x] 3.4 Logs/events include stage and role identity for executions
- [x] 3.5 Arbitrary role plugin model beyond built-in skills

---

## Supersession Boundary

This document is stale and archived. Active control-plane commitments live in `M5_002`.

---

## 4.0 Out of Scope

- Arbitrary DAG scheduling with branch joins
- UI stage designer
- Runtime hot-reload of profile graph
