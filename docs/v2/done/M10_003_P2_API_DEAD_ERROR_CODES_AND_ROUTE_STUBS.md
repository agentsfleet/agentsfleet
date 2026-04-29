---
Milestone: M10
Workstream: M10_003
Name: DEAD_ERROR_CODES_AND_ROUTE_STUBS
Status: DONE
Branch: feat/m10-003-dead-error-codes
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

## Applicable Rules

- RULE NDC — no dead code (primary driver)
- RULE ORP — cross-layer orphan sweep after deletion
- RULE EP4 — removed endpoints return 410 (these are being fully removed now)
- RULE XCC — cross-compile before commit
- RULE FLL — 350-line gate on touched files

## Invariants

N/A — deletion-only spec, no new compile-time guardrails.

## Eval Commands

```bash
# E1: Zero ERR_PROPOSAL_* constants
grep -rn "ERR_PROPOSAL" src/ --include="*.zig" | head -5
echo "E1: proposal code refs (empty = pass)"

# E2: Zero proposal handler functions
grep -rn "handleListAgentProposals\|handleGetAgentProposal\|handleApproveProposal" src/ --include="*.zig" | head -5
echo "E2: proposal handler refs (empty = pass)"

# E3: Zero proposal route types
grep -rn "AgentProposalRoute\|AgentHarnessChangeRoute" src/ --include="*.zig" | head -5
echo "E3: proposal route refs (empty = pass)"

# E4: Zero route matchers for removed routes
grep -rn "matchAgentProposalAction\|matchAgentHarnessChangeAction" src/ --include="*.zig" | head -5
echo "E4: route matcher refs (empty = pass)"

# E5: Build + test + lint + cross-compile + gitleaks
zig build 2>&1 | head -5; echo "build=$?"
zig build test 2>&1 | tail -5; echo "test=$?"
make lint 2>&1 | grep -E "✓|FAIL"
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E6: Memory leak check
zig build test 2>&1 | grep -i "leak" | head -5
echo "E6: leak check (empty = pass)"

# E7: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

## Dead Code Sweep

| Deleted symbol | Grep command | Expected |
|---------------|--------------|----------|
| `ERR_PROPOSAL_*` | `grep -rn "ERR_PROPOSAL" src/ --include="*.zig"` | 0 matches |
| `ERR_HARNESS_CHANGE_NOT_FOUND` | `grep -rn "ERR_HARNESS_CHANGE" src/ --include="*.zig"` | 0 matches |
| `AgentProposalRoute` | `grep -rn "AgentProposalRoute" src/ --include="*.zig"` | 0 matches |
| `AgentHarnessChangeRoute` | `grep -rn "AgentHarnessChangeRoute" src/ --include="*.zig"` | 0 matches |
| `matchAgentProposalAction` | `grep -rn "matchAgentProposalAction" src/ --include="*.zig"` | 0 matches |
| `handleListAgentProposals` | `grep -rn "handleListAgentProposals" src/ --include="*.zig"` | 0 matches |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `zig build test` | all pass, no failures | ✅ |
| Leak detection | `zig build test \| grep leak` | empty (no leaks) | ✅ |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | exit 0 | ✅ |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | exit 0 | ✅ |
| Lint | `make lint` | zombied lint passed | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found | ✅ |
| 350L gate | `wc -l` (exempts .md) | no file over 350 | ✅ |
| Dead code sweep | grep UZ-PROPOSAL, UZ-HARNESS, UZ-AGENT-002, UZ-WORKER | 0 matches in src/ | ✅ |

## Out of Scope

- `handleGetAgent` — still active (agent profile viewer)
- `get_agent` Route variant — still routed and useful
- ERR_AGENT_NOT_FOUND — still used by get.zig

## Acceptance Criteria

- [ ] `grep -rn ERR_PROPOSAL src/` returns 0 matches
- [ ] `grep -rn handleListAgentProposals src/` returns 0 matches
- [ ] `grep -rn AgentProposalRoute src/` returns 0 matches
- [ ] `make test` passes
- [ ] `make lint` passes
- [ ] Cross-compiles
