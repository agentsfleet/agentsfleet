# M13_001: Credential Vault UI — web-based credential management, never shows values

**Prototype:** v1.0.0
**Milestone:** M13
**Workstream:** 001
**Date:** Apr 10, 2026
**Status:** PENDING
**Priority:** P1 — Operator trust surface; proves "agents never see your keys"
**Batch:** B5 — after M12 (app dashboard provides the shell)
**Branch:** feat/m13-credential-vault-ui
**Depends on:** M12_001 (app dashboard layout + auth), M5_001 (tool bridge credential flow)

---

## Overview

**Goal (testable):** The Credentials page at `app.usezombie.com/credentials` provides full vault management: add credentials (name + value → encrypted at rest, value never stored in browser), list credentials (name, scope, which Zombies use it, last injection timestamp — never the value), delete credentials (with confirmation showing which Zombies will break), and view credential usage log (which Zombie, which request, which tool, when — proving the audit trail claim). The usage log is the killer feature: it answers "when was my Stripe key last used and by which agent?"

**Problem:** M12 includes a basic credentials list, but the vault deserves a dedicated deep experience. The CEO plan's core differentiator is "agents never see your keys" — the Credential Vault UI is the proof. Today, credential management is CLI-only (`zombiectl credential add/list`). The web UI needs to demonstrate the security model visually: values are write-only (submitted once, never retrievable), usage is fully audited, and deletion shows impact. Without this, the "credentials hidden" claim is words, not evidence.

**Solution summary:** Extend the M12 credentials page into a full vault management experience with four views: (1) Credential list with scope and last-used metadata, (2) Add credential flow with write-only UX (value field clears on submit, never echoed), (3) Delete credential flow with impact analysis (which Zombies will lose access), (4) Credential usage log (filtered from activity_events where event_type='credential_injected'). Requires one new API endpoint: `GET /v1/workspaces/{ws}/credentials/{name}/usage` (paginated usage history). All other endpoints exist.

---

## 1.0 Credential List View

**Status:** PENDING

Enhanced list showing operational metadata for each credential. Each row: name, scope (which skills/tools reference it), zombie count (how many Zombies use it), last injected (timestamp of most recent firewall credential injection), and health status (active/unused/error).

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `app/credentials/page.tsx`
  - input: `Workspace with 3 credentials: stripe (2 Zombies, last used 10m ago), slack (1 Zombie, last used 1h ago), github (0 Zombies, never used)`
  - expected: `Table renders with name, zombie count, last_injected as relative time, health badge (active/unused)`
  - test_type: unit (component test)
- 1.2 PENDING
  - target: `app/credentials/page.tsx`
  - input: `Credential value column`
  - expected: `No value column exists in the table. No API endpoint returns credential values. Code review confirms.`
  - test_type: unit (static analysis — grep for value/secret/token in rendered output)
- 1.3 PENDING
  - target: `app/credentials/components/CredentialRow.tsx`
  - input: `Click on credential row`
  - expected: `Expands to show usage log preview (last 5 injections) + link to full usage log`
  - test_type: unit (component test)
- 1.4 PENDING
  - target: `app/credentials/page.tsx`
  - input: `Empty workspace with no credentials`
  - expected: `Empty state: "No credentials yet. Add one to get started." + Add button`
  - test_type: unit (component test)

---

## 2.0 Add Credential Flow

**Status:** PENDING

Write-only credential submission. The value field is a password input that clears immediately after submission. The value is sent to the API via HTTPS POST, encrypted at rest server-side, and never returned by any API endpoint. The UI must make the write-only nature obvious: "This value will be encrypted and cannot be retrieved later."

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `app/credentials/components/AddCredentialModal.tsx`
  - input: `User enters name="stripe", value="sk_test_xxx", clicks Submit`
  - expected: `POST /v1/workspaces/{ws}/credentials, success toast, value field cleared, modal closes`
  - test_type: unit (component test)
- 2.2 PENDING
  - target: `app/credentials/components/AddCredentialModal.tsx`
  - input: `After successful submission`
  - expected: `Value is NOT in: React state, localStorage, sessionStorage, URL params, console logs. Field ref cleared.`
  - test_type: unit (verify no browser state retention)
- 2.3 PENDING
  - target: `app/credentials/components/AddCredentialModal.tsx`
  - input: `User enters duplicate credential name`
  - expected: `API returns 409, modal shows: "Credential 'stripe' already exists. Delete it first to replace."`
  - test_type: unit (component test)
- 2.4 PENDING
  - target: `app/credentials/components/AddCredentialModal.tsx`
  - input: `User submits empty value`
  - expected: `Client-side validation: "Credential value cannot be empty"`
  - test_type: unit (component test)

---

## 3.0 Delete Credential Flow

**Status:** PENDING

Deletion with impact analysis. Before confirming deletion, the UI shows which Zombies reference this credential and will break if it's deleted. This prevents accidental deletion of in-use credentials.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `app/credentials/components/DeleteCredentialDialog.tsx`
  - input: `Delete "stripe" credential used by 2 Zombies`
  - expected: `Dialog shows: "Deleting 'stripe' will affect: lead-collector, bug-fixer. These Zombies will fail on next credential injection." + [Cancel] [Delete anyway]`
  - test_type: unit (component test)
- 3.2 PENDING
  - target: `app/credentials/components/DeleteCredentialDialog.tsx`
  - input: `Delete "unused_key" credential used by 0 Zombies`
  - expected: `Dialog shows: "No Zombies use 'unused_key'. Safe to delete." + [Cancel] [Delete]`
  - test_type: unit (component test)
- 3.3 PENDING
  - target: `app/credentials/components/DeleteCredentialDialog.tsx`
  - input: `User confirms deletion`
  - expected: `DELETE /v1/workspaces/{ws}/credentials/{name}, credential removed from list, success toast`
  - test_type: unit (component test)

---

## 4.0 Credential Usage Log

**Status:** PENDING

Per-credential usage history showing every time the credential was injected into an outbound request. Data comes from a new API endpoint that filters `core.activity_events` where `event_type='credential_injected' AND detail->>'credential_name'='{name}'`. Each entry shows: timestamp, zombie name, tool, target domain, action summary.

This is the audit proof: "Your Stripe key was last used by bug-fixer to create a charge at 10:47 AM."

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `src/http/handlers/credential_usage.zig:handleGetUsage`
  - input: `GET /v1/workspaces/{ws}/credentials/stripe/usage?limit=20`
  - expected: `Paginated list of credential injection events for "stripe": timestamp, zombie_name, tool, target_domain`
  - test_type: integration (DB)
- 4.2 PENDING
  - target: `app/credentials/[name]/usage/page.tsx`
  - input: `Usage log with 50 entries`
  - expected: `Table with columns: Time, Zombie, Tool, Target, Action. Cursor-based pagination.`
  - test_type: unit (component test)
- 4.3 PENDING
  - target: `src/http/handlers/credential_usage.zig:handleGetUsage`
  - input: `Credential with no usage history`
  - expected: `Empty array, HTTP 200`
  - test_type: integration (DB)
- 4.4 PENDING
  - target: `app/credentials/[name]/usage/page.tsx`
  - input: `Usage entry: "10:47 AM · lead-collector · agentmail · api.agentmail.com · Sent reply"`
  - expected: `Entry renders with relative timestamp, zombie linked to detail page, target shown`
  - test_type: unit (component test)

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 New API Endpoint

```zig
// src/http/handlers/credential_usage.zig
pub fn handleGetUsage(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void

// GET /v1/workspaces/{ws}/credentials/{name}/usage?cursor=...&limit=20
// Response:
// {
//   "events": [
//     {"timestamp": 1712345678, "zombie_name": "lead-collector", "tool": "agentmail", "target": "api.agentmail.com", "action": "send_reply"},
//     ...
//   ],
//   "next_cursor": "019abc..."
// }
```

### 5.2 Existing API Endpoints Used

```
GET    /v1/workspaces/{ws}/credentials             — list (name, scope, no values)
POST   /v1/workspaces/{ws}/credentials             — add (encrypted at rest)
DELETE /v1/workspaces/{ws}/credentials/{name}       — delete
```

### 5.3 Error Contracts

| Error condition | Code | Developer sees | HTTP |
|----------------|------|---------------|------|
| Credential not found | `UZ-CRED-001` | "Credential '{name}' not found" | 404 |
| Duplicate name | `UZ-CRED-002` | "Credential '{name}' already exists. Delete first." | 409 |

---

## 6.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Credential value NEVER returned by any API | grep all API handlers for credential value in response |
| Credential value NEVER stored in browser state | grep frontend for localStorage/sessionStorage/state containing credential |
| credential_usage.zig < 200 lines | `wc -l` |
| Each component file < 400 lines | `wc -l app/credentials/**/*.tsx` |
| Usage log query uses existing activity_events index | EXPLAIN ANALYZE on query |
| Delete dialog shows impact before confirmation | Component test |
| Cross-compiles (new Zig handler) | both targets |
| drain() before deinit() | `make check-pg-drain` |

---

## 7.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Implement credential_usage.zig (new API endpoint) | Integration tests 4.1, 4.3 pass |
| 2 | Enhanced credential list view (scope, last_used, health) | Tests 1.1-1.4 pass |
| 3 | Add credential modal (write-only, value cleared) | Tests 2.1-2.4 pass |
| 4 | Delete credential dialog (impact analysis) | Tests 3.1-3.3 pass |
| 5 | Credential usage log page | Tests 4.2, 4.4 pass |
| 6 | Cross-compile (Zig handler) | both targets pass |
| 7 | Full test suite | `make test && make lint` |

---

## 8.0 Acceptance Criteria

**Status:** PENDING

- [ ] Credential list shows name, scope, last_used — never values — verify: component test + code review
- [ ] Add credential: value cleared after submit, not in browser state — verify: test 2.2
- [ ] Delete credential: impact shown before confirm — verify: test 3.1
- [ ] Usage log: per-credential injection history — verify: integration test
- [ ] Empty states handled gracefully — verify: tests 1.4, 4.3
- [ ] `make test && make lint` pass
- [ ] Cross-compile passes (Zig handler)
- [ ] `make check-pg-drain` passes

---

## 9.0 Out of Scope

- Credential rotation (revoke + re-add for v1)
- Credential sharing across workspaces (workspace-scoped only)
- Credential type detection (Stripe vs Slack vs generic — all treated as opaque strings)
- Credential value editing (write-once; delete + re-add to change)
- Export/import credentials (security risk — not planned)
- Credential expiry notifications (future — depends on provider metadata)
