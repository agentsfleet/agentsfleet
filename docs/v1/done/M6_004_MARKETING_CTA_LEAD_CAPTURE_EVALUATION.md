# M6_004: Marketing CTA Lead Capture Evaluation

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 004
**Date:** Mar 07, 2026
**Status:** DONE
**Priority:** P0 — Validate low-friction lead capture from website CTAs before broader launch spend
**Batch:** B8 — documentation-aligned launch batch
**Depends on:** M3_007_WEBSITE_LAUNCH_BLOCKERS.md, M3_009_WEBSITE_POSITIONING_AND_BRAND_SYSTEM.md

---

## 1.0 Human CTA Lead Capture Path

**Status:** DONE

Define and implement a low-friction human-only lead capture path on pricing CTAs, while keeping the homepage primary CTA pointed at the app. Lead capture uses one primary action (`Notify me`) and durable storage in an external marketing system.

**Dimensions:**
- 1.1 DONE Replace selected pricing CTAs with lead-capture action and one-field form or hosted form handoff while preserving the homepage primary CTA to the app
- 1.2 DONE Persist leads with source metadata (`page`, `cta_id`, `plan_interest`, `timestamp`)
- 1.3 DONE Provide explicit success state (`You’re in`) with no navigation dead-end
- 1.4 DONE Ensure all human capture links and endpoints are config-driven via environment variables

---

## 2.0 Agent-Safe Interest Routing

**Status:** DONE

Separate autonomous agent traffic from human lead capture so machine clients do not pollute marketing lists.

### 2.1 Agent Surface CTA Contract

Agent page keeps machine-oriented actions (docs/webhooks/API interest) and must not submit to human marketing list endpoints.

**Dimensions:**
- 2.1.1 DONE Remove/avoid human email capture controls from `/agents`
- 2.1.2 DONE Define agent-specific interest endpoint/flow (`callback_url` or webhook registration) if needed
- 2.1.3 DONE Add test coverage that prevents human waitlist form rendering on agent route

---

## 3.0 Evaluation Instrumentation

**Status:** DONE

Track conversion quality and operational value of captured leads to decide whether to keep third-party hosted flow or migrate in-house.

**Dimensions:**
- 3.1 DONE Emit analytics events for CTA click, form open, submit success, and submit failure
- 3.2 DONE Capture campaign attribution fields (`utm_source`, `utm_medium`, `utm_campaign`) when present
- 3.3 DONE Add weekly export/report path to evaluate conversion rate and lead quality
- 3.4 DONE Document retention and consent language for marketing follow-up

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 Human CTA lead capture works end-to-end in local, dev, and production configs
- [x] 4.2 Leads are queryable/exportable from selected system for re-engagement blasts
- [x] 4.3 Agent route does not write into human marketing lead pipeline
- [x] 4.4 E2E coverage validates submit success path and link routing behavior
- [x] 4.5 Docs include operator setup for provider keys, list IDs, and rollback switch

---

## 5.0 Out of Scope

- Building a custom CRM
- Multi-step qualification forms
- Automated outbound campaign sequencing
- Sales pipeline scoring models
