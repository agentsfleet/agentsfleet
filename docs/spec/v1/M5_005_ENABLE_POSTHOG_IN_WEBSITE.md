# M5_005: Enable PostHog Tracking In Website

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 005
**Date:** Mar 06, 2026
**Status:** DONE
**Priority:** P1 — product analytics baseline
**Batch:** B1 — uses PostHog JS SDK, not posthog-zig
**Depends on:** None

---

## 1.0 Singular Function

**Status:** DONE

Implement one working telemetry function: website emits PostHog events for core conversion and navigation actions.

**Dimensions:**
- 1.1 DONE Initialize PostHog client with environment-gated config
- 1.2 DONE Capture core events (`signup_started`, `signup_completed`, `team_pilot_booking_started`)
- 1.3 DONE Enforce event schema naming and required properties
- 1.4 DONE Add privacy-safe guardrails for event payloads

---

## 2.0 Verification Units

**Status:** DONE

**Dimensions:**
- 2.1 DONE Unit test: event helper emits expected schema
- 2.2 DONE Integration test: core CTA clicks produce PostHog requests
- 2.3 DONE Integration test: disabled analytics mode emits no external calls

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 Core website conversion events are emitted from website CTA/navigation actions
- [x] 3.2 Event payloads are schema-consistent and privacy-safe through property allowlisting
- [x] 3.3 Local test evidence captured for website event emission behavior

---

## 4.0 Out of Scope

- Zombied runtime event tracking (tracked in M5_006)
- Complex analytics dashboard design
- `/install` edge-function telemetry and installer delivery are tracked separately from website CTA analytics scope
