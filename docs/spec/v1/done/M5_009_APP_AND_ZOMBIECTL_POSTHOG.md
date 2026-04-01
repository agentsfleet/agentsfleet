# M5_009: App And Zombiectl PostHog

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 009
**Date:** Mar 22, 2026
**Status:** DONE
**Priority:** P1 — product analytics coverage for operator surfaces
**Batch:** B2 — extends existing website and zombied PostHog coverage
**Depends on:** M5_005, M5_006

---

## 1.0 Singular Function

**Status:** DONE

Implement one working telemetry function: the dashboard app and `zombiectl` emit deterministic PostHog events for meaningful operator actions and failures.

**Dimensions:**
- 1.1 DONE Add browser bootstrap, identity, navigation, and runtime error tracking to `ui/packages/app`
- 1.2 DONE Add dashboard page/action tracking for workspace and run flows
- 1.3 DONE Add `zombiectl` command lifecycle, domain event, and error tracking
- 1.4 DONE Improve `zombiectl` human output formatting for key command responses

---

## 2.0 Verification Units

**Status:** DONE

**Dimensions:**
- 2.1 DONE App unit tests pass, including instrumentation and analytics helper coverage
- 2.2 DONE App analytics coverage gate passes at or above the configured 95% threshold
- 2.3 DONE `zombiectl` mixed Node/Bun test suite passes with the new test runner
- 2.4 DONE Website regression suite passes to confirm existing PostHog instrumentation stays green
- 2.5 DONE Cross-package gates pass for `make lint`, `make test`, and `make qa`

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 Dashboard events capture workspace, run, navigation, and runtime error signals with allowlisted properties
- [x] 3.2 `zombiectl` emits command lifecycle, domain, and error events without breaking CLI UX
- [x] 3.3 `zombiectl` human-readable output is structured with minimal section and key/value formatting improvements
- [x] 3.4 Durable event inventory and vault/env mapping are documented in `docs/POSTHOG.md`
- [x] 3.5 Surface-level verification passed for website, app, and `zombiectl`
- [x] 3.6 Repo verification gates passed after CI regression fixes for app, website, and `zombiectl`

---

## 4.0 Out of Scope

- `zombied` server and worker PostHog instrumentation changes
- PostHog dashboard/chart creation
- Cross-surface funnel analysis in PostHog itself
