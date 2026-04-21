# M13_001: Credential Vault UI — web-based credential management, never shows values

**Prototype:** v1.0.0
**Milestone:** M13
**Workstream:** 001
**Date:** Apr 10, 2026
**Status:** PENDING
**Priority:** P1 — Operator trust surface; proves "agents never see your keys"
**Batch:** B2 — parallel with M19_001, M21_001, M11_005
**Branch:** feat/m13-credential-vault-ui
**Depends on:** M12_001 (app dashboard layout + auth), M5_001 (tool bridge credential flow)
**Supersedes:** M12_001 §4.0 (Credentials Page) — M12's basic credentials list is replaced by this spec in full
**Extended by:** M21_001 (B6) — adds `credential_type = llm_provider` field for BYOK provider keys; same vault, same Add Credential modal with a new "Type" selector; provider credentials are excluded from firewall injection path

---

## Overview

**Goal (testable):** The Credentials page at `app.usezombie.com/credentials` provides MVP vault management: add credentials (name + value → encrypted at rest, value never stored in browser), list credentials (name, scope, which Zombies use it — never the value), and delete credentials (with confirmation showing which Zombies will break).

**Problem:** M12 includes a basic credentials list, but the vault deserves a dedicated experience. The CEO plan's core differentiator is "agents never see your keys" — the Credential Vault UI is the proof. Today, credential management is CLI-only (`zombiectl credential add/list`). The web UI needs to demonstrate the security model visually: values are write-only (submitted once, never retrievable), deletion shows impact.

**Solution summary:** Extend the M12 credentials page into MVP vault management with three views: (1) Credential list with scope metadata, (2) Add credential flow with write-only UX (value field clears on submit, never echoed), (3) Delete credential flow with impact analysis (which Zombies will lose access). No new API endpoint required — uses existing list/add/delete.

---

## 1.0 Credential List View

**Status:** PENDING

List showing operational metadata for each credential. Each row: name, scope (which skills/tools reference it), zombie count (how many Zombies use it).

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `app/credentials/page.tsx`
  - input: `Workspace with 3 credentials: stripe (2 Zombies), slack (1 Zombie), github (0 Zombies)`
  - expected: `Table renders with name, scope, zombie count`
  - test_type: unit (component test)
- 1.2 PENDING
  - target: `app/credentials/page.tsx`
  - input: `Credential value column`
  - expected: `No value column exists in the table. No API endpoint returns credential values. Code review confirms.`
  - test_type: unit (static analysis — grep for value/secret/token in rendered output)
- 1.3 PENDING
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

## 5.0 Interfaces

**Status:** PENDING

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
| Each component file < 400 lines | `wc -l app/credentials/**/*.tsx` |
| Delete dialog shows impact before confirmation | Component test |

---

## 7.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Credential list view (name, scope, zombie count) | Tests 1.1-1.3 pass |
| 2 | Add credential modal (write-only, value cleared) | Tests 2.1-2.4 pass |
| 3 | Delete credential dialog (impact analysis) | Tests 3.1-3.3 pass |
| 4 | Full test suite | `make test && make lint` |

---

## 8.0 Acceptance Criteria

**Status:** PENDING

- [ ] Credential list shows name, scope — never values — verify: component test + code review
- [ ] Add credential: value cleared after submit, not in browser state — verify: test 2.2
- [ ] Delete credential: impact shown before confirm — verify: test 3.1
- [ ] Empty states handled gracefully — verify: test 1.3
- [ ] `make test && make lint` pass

---

## Applicable Rules

RULE FLL (350-line gate), RULE ORP (orphan sweep). Standard set for Next.js components — no Zig handlers in this workstream.

---

## Invariants

N/A — no compile-time guardrails.

---

## Eval Commands

```bash
# E1: Build (Zig backend)
zig build 2>&1 | head -5; echo "zig_build=$?"

# E2: Build (Next.js frontend)
npm run build 2>&1 | head -5; echo "next_build=$?"

# E3: Tests
make test 2>&1 | tail -5; echo "test=$?"

# E4: Lint
make lint 2>&1 | grep -E "✓|FAIL"

# E5: Gitleaks
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E6: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E7: Cross-compile (Zig handler)
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "xc_x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "xc_arm=$?"

# E8: Memory leak check
make check-pg-drain 2>&1 | tail -3; echo "drain=$?"
```

---

## Dead Code Sweep

N/A — no files deleted.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Zig build | `zig build` | | |
| Next.js build | `npm run build` | | |
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| 350L gate | see E6 | | |
| drain check | `make check-pg-drain` | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- Usage audit log — deferred to M13b post-MVP.
- Credential rotation (revoke + re-add for v1)
- Credential sharing across workspaces (workspace-scoped only)
- Credential type detection (Stripe vs Slack vs generic — all treated as opaque strings)
- Credential value editing (write-once; delete + re-add to change)
- Export/import credentials (security risk — not planned)
- Credential expiry notifications (future — depends on provider metadata)
