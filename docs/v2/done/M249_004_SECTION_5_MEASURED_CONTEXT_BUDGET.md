# M249_004 §5: Measured context budget

**Status:** DEFERRED
**Source:** M249_004, Section 5 and Dimension 5.1

The owner parked this section while retaining the rest of M249_004. Reopen it only when its measurement producer and units are agreed.

## Original section

### §5 — Measured context budget

Identify the runtime measurement producer and applicable model limit before exposing a percentage. Define units, observation time and run/model identity.

Output tokens, cost and configured capacity alone cannot prove remaining context. Model changes, compaction and stale measurements must invalidate incompatible observations.

If no producer exists, keep the unavailable state and record the required producer design. Do not invent a context accounting subsystem silently.

| Dimension | Test | Expected behavior |
|---|---|---|
| 5.1: Budget requires a coherent observation | `context_budget_requires_matching_fresh_measurement` (unit/integration) | Stale, mismatched and absent observations show unavailable; valid zero remains zero |

## Discovery

> Owner: "I think you could move the Dimension 5, in the M249_004 to parked, since the rest of the spec is valid"

Reopen this section when the measurement producer, units and invalidation rules are agreed.

## Parent workstream

[Recorded-data completion](../pending/M249_004_P1_API_DOCS_UI_RECORDED_DATA_COMPLETION.md) retains Usage, key metadata, secret access and recorded-work projections.
