---
name: m47-gate-fixture
description: |
  Synthetic fixture used by the approval-inbox integration tests. NOT a public
  sample — lives under samples/fixtures/. Promote to samples/ only if and when
  a real public zombie shape with gated destructive operations ships.
gates:
  - tool: write_repo
    action: '*'
    behavior: approve
---

# Test fixture: gate-on-write

This skill exists solely to exercise the approval-gate path under integration
test. It declares a single gated action — any `write_repo:*` tool call requires
operator approval. The test harness drives a synthetic event whose tool/action
matches this rule, observes a row land in `core.zombie_approval_gates`, and
walks the approve / deny / dual-channel-dedup / sweeper-timeout flows through
the dashboard inbox endpoints.

The skill body is intentionally minimal — there is no real prompt and no real
tool. The integration test never reaches the model; it only needs the gate
state machine to fire so the dashboard surface can be observed end to end.
