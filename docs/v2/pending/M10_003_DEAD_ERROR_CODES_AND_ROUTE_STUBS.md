---
Milestone: M10
Workstream: M10_003
Name: DEAD_ERROR_CODES_AND_ROUTE_STUBS
Status: PENDING
Priority: P2 — no runtime errors, cosmetic cleanup
Created: Apr 11, 2026
Depends on: M10_001 (pipeline v1 removal)
---

# M10_003 — Dead Error Codes and 410 Route Stub Removal

## Goal

Remove pipeline-era error code constants, hint entries, and 410 stub routes
that have no callers or consumers. Reduces codes.zig noise and router surface.

## Problem

M10_001 stubbed proposal/harness/scores endpoints as 410 Gone for backward
compatibility. The 410 stubs serve no deprecation purpose — no CLI or client
has consumed these endpoints since pipeline v1 was removed. The error codes
and routes inflate the codebase without value.

## Scope

| Item | File | Action |
|------|------|--------|
| ERR_PROPOSAL_* (15 codes) | `codes.zig:49-65` | Remove constants |
| ERR_HARNESS_CHANGE_NOT_FOUND | `codes.zig:66` | Remove constant |
| PROPOSAL hint entries | `codes.zig` hint() | Remove (none exist — just verify) |
| 410 stub handlers (7) | `agents.zig` | Remove functions |
| Proposal/harness/scores Route variants | `router.zig` Route union | Remove variants |
| Route matching for proposals/harness | `router.zig` match() | Remove match arms |
| matchAgentProposalAction | `route_matchers.zig` | Remove function |
| matchAgentHarnessChangeAction | `route_matchers.zig` | Remove function |
| AgentProposalRoute struct | `router.zig` | Remove type |
| AgentHarnessChangeRoute struct | `router.zig` | Remove type |
| Server dispatch for removed routes | `server.zig` | Remove switch arms |
| handler.zig re-exports | `handler.zig` | Remove dead re-exports |
| Router tests for proposal/harness | `router_test.zig` | Remove test cases |

## Out of Scope

- `handleGetAgent` — still active (agent profile viewer)
- `get_agent` Route variant — still routed and useful
- ERR_AGENT_NOT_FOUND, ERR_AGENT_SCORES_UNAVAILABLE — still used by get.zig

## Acceptance Criteria

- [ ] `grep -rn ERR_PROPOSAL src/` returns 0 matches
- [ ] `grep -rn handleListAgentProposals src/` returns 0 matches
- [ ] `grep -rn AgentProposalRoute src/` returns 0 matches
- [ ] `make test` passes
- [ ] `make lint` passes
- [ ] Cross-compiles
