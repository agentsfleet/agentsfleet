# M6_001 Paid Scale Plan Lifecycle Evidence

**Date:** Mar 13, 2026
**Status:** DONE

## Commands

```bash
zig build test
```

## Covered Lifecycle

1. `POST /v1/workspaces/{workspace_id}/billing/scale`
   Promotes a workspace from `FREE` to `SCALE`.
2. `POST /v1/workspaces/{workspace_id}/billing/events`
   Accepts `PAYMENT_FAILED` and `DOWNGRADE_TO_FREE` billing lifecycle events.
3. Existing reconcile path
   `workspace_billing.applyBillingLifecycleEvent(...)` records the event and immediately drives the deterministic reconcile logic.

## Verified Outcomes

- Upgrade applies `SCALE` entitlements and audit state deterministically.
- `PAYMENT_FAILED` moves a paid workspace into `GRACE` and preserves the subscription until grace expiry.
- Grace expiry downgrades the workspace back to `FREE`.
- `DOWNGRADE_TO_FREE` immediately downgrades the workspace through the same production billing path.
- A downgraded workspace can be upgraded back to `SCALE`.
- Successful runs remain billable; failed and incomplete runs remain non-billable.

## Source References

- Billing state source of truth: `src/state/workspace_billing.zig`
- Lifecycle transition tests: `src/state/workspace_billing_test.zig`
- Pure transition tests: `src/state/workspace_billing_transition.zig`
- Usage metering tests: `src/state/billing.zig`
