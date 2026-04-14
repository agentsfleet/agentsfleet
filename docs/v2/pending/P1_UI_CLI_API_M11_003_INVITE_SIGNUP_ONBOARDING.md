# M11_003: Invite Code + Signup Onboarding — CLI / API / UI

**Prototype:** v2
**Milestone:** M11
**Workstream:** 003
**Date:** Apr 13, 2026
**Status:** PENDING
**Priority:** P1 — Blocks user acquisition; no invite = no entry
**Batch:** B5 — builds alongside M12; prerequisite to all post-signup milestones
**Branch:** feat/m11-invite-signup
**Depends on:** M15_001 (credit metering, done), M12_001 (app shell, for web UI path)

---

## Overview

**Goal (testable):** A qualified lead receives an invite code via email or Slack (sent by the Lead Nurturer Zombie or manually by an admin). They use it to complete signup via three equivalent paths: (1) web UI at `app.usezombie.com/signup`, (2) CLI via `zombiectl auth signup --invite-code IVT-xxxx`, (3) API via `POST /v1/auth/signup`. On successful redemption, an account + workspace is created and X free credits are loaded instantly. The invite code is single-use and the credit grant is permanent until exhausted.

**Problem:** UseZombie is invite-only at launch. Without an invite gate, anyone can sign up; without a frictionless redemption path, qualified leads drop off. The current state has no invite system at all — signups are either open (risk) or fully manual (ops bottleneck). The Lead Nurturer Zombie exists to score and warm leads, but it has no mechanism to close the loop by issuing access. The missing link is: zombie qualifies lead → zombie creates invite → lead redeems invite in whatever environment they prefer (browser, terminal, pipeline).

**Solution summary:** Three-layer implementation. (1) Backend: invite code generation API (`POST /v1/invites`), redemption API (`POST /v1/auth/signup`), and credit grant wired into the workspace creation path. (2) Web UI: `/signup` page with invite code entry, account creation form (via Clerk), and a post-signup redirect to the dashboard with credits displayed. (3) CLI + API: `zombiectl auth signup --invite-code` for terminal-first users; raw API endpoint for agent/pipeline consumers who provision workspaces programmatically.

The Lead Nurturer Zombie calls `POST /v1/invites` as an execution target (like any tool call) — it goes through the firewall, is optionally gated by an approval, and the resulting invite code is returned to the zombie which then emails or Slacks it to the lead.

---

## 1.0 Invite Code Generation

**Status:** PENDING

Invite codes are created either by an admin manually, or autonomously by the Lead Nurturer Zombie. Each code carries a credit value, an expiry date, and a single-use flag. The zombie calls this endpoint like any other tool call — via `POST /v1/execute` with `target: api.usezombie.com/v1/invites`.

**Layout:**

```
Admin panel: Settings > Invites
┌──────────────────────────────────────────────────────┐
│ Invite Codes                          [+ Create]     │
├────────────────┬────────────┬──────────┬─────────────┤
│ Code           │ Credits    │ Status   │ Expires     │
├────────────────┼────────────┼──────────┼─────────────┤
│ IVT-ZMB-4F9A   │ 20 runs    │ Sent     │ May 13      │
│ IVT-ZMB-A1C3   │ 20 runs    │ Redeemed │ —           │
│ IVT-ZMB-9E2B   │ 5 runs     │ Expired  │ Apr 10      │
└────────────────┴────────────┴──────────┴─────────────┘
```

**Dimensions:**
- 1.1 PENDING
  - target: `POST /v1/invites`
  - input: `{ credit_value: 20, expiry_days: 30, note: "YC batch W26" }` + admin token
  - expected: `{ invite_code: "IVT-ZMB-xxxx", credits: 20, expires_at: "...", status: "created" }`
  - test_type: integration
- 1.2 PENDING
  - target: `POST /v1/invites` called via zombie (Lead Nurturer Zombie via `/v1/execute`)
  - input: zombie execution with `target: "api.usezombie.com/v1/invites"`, `credential_ref: "usezombie_admin"`, `body: { credit_value: 20 }`
  - expected: invite created; code returned in execute response; zombie can forward it to lead
  - test_type: integration
- 1.3 PENDING
  - target: `GET /v1/invites`
  - input: admin token, optional `?status=pending`
  - expected: paginated list with code, credits, status, redeemed_by, redeemed_at
  - test_type: integration
- 1.4 PENDING
  - target: invite generation with approval gate
  - input: zombie requests invite with `credit_value: 100` (above auto-approve threshold)
  - expected: firewall gate fires; admin receives Slack DM "Lead Nurturer wants to generate a 100-credit invite for jane@acme.com — Approve?"; on approve, code generated
  - test_type: integration

---

## 2.0 Web UI Signup Flow

**Status:** PENDING

Landing path for invite recipients who click a link or visit directly. Invite code pre-populated from URL param if present. On success: Clerk account created, workspace created, credits loaded, redirect to dashboard.

**Flow:**

```
app.usezombie.com/signup?code=IVT-ZMB-4F9A

┌─────────────────────────────────────────────────┐
│          Welcome to UseZombie                   │
│                                                 │
│  You've been invited. Enter your code below.   │
│                                                 │
│  Invite code  [IVT-ZMB-4F9A          ] [Check] │
│  ✓ Valid — 20 free runs included                │
│                                                 │
│  Email        [jane@acme.com         ]          │
│  Password     [••••••••••••          ]          │
│  Workspace    [acme-prod             ]          │
│                                                 │
│  [Create account →]                             │
└─────────────────────────────────────────────────┘

On success → redirect to /dashboard
┌─────────────────────────────────────────────────┐
│  Welcome, Jane. You have 20 free runs.          │
│  [Set up your first zombie →]                   │
└─────────────────────────────────────────────────┘
```

**Dimensions:**
- 2.1 PENDING
  - target: `app.usezombie.com/signup`
  - input: valid invite code in URL param `?code=IVT-ZMB-4F9A`
  - expected: code pre-filled, validity shown ("✓ Valid — 20 free runs"), form ready
  - test_type: e2e
- 2.2 PENDING
  - target: signup form submission
  - input: valid code + email + password + workspace name
  - expected: Clerk account created; workspace created; 20 credits loaded; redirect to /dashboard; credits visible in spend tracker
  - test_type: e2e
- 2.3 PENDING
  - target: invalid / expired / already-used invite code
  - input: `code=IVT-ZMB-DEAD`
  - expected: inline error "Invalid or expired invite code" — no account created
  - test_type: unit (component test)
- 2.4 PENDING
  - target: invite code consumed on first use
  - input: redeem IVT-ZMB-4F9A, then attempt to redeem again with a different email
  - expected: second redemption returns 409 `UZ-INVITE-002` "Invite already redeemed"
  - test_type: integration

---

## 3.0 CLI Signup Path

**Status:** PENDING

For terminal-first operators and pipeline consumers who want to create an account without a browser.

```bash
$ zombiectl auth signup --invite-code IVT-ZMB-4F9A
Email: jane@acme.com
Password: ••••••••
Workspace name: acme-prod

✓ Account created
✓ Workspace: acme-prod (ws_01abc)
✓ Credits: 20 runs loaded
✓ Logged in — token saved to ~/.zombiectl/credentials

Run `zombiectl zombie create --help` to get started.
```

**For agent/pipeline use (non-interactive):**

```bash
zombiectl auth signup \
  --invite-code IVT-ZMB-4F9A \
  --email jane@acme.com \
  --workspace acme-prod \
  --non-interactive \
  --output-token   # prints token to stdout for CI/CD capture
```

**Dimensions:**
- 3.1 PENDING
  - target: `zombiectl auth signup`
  - input: valid invite code + email + workspace via interactive prompts
  - expected: account created; token stored in `~/.zombiectl/credentials`; success summary printed
  - test_type: CLI integration
- 3.2 PENDING
  - target: `zombiectl auth signup --non-interactive --output-token`
  - input: all flags provided non-interactively
  - expected: token printed to stdout; exit 0; no interactive prompts
  - test_type: CLI integration (CI simulation)
- 3.3 PENDING
  - target: `zombiectl auth signup` with invalid code
  - input: `--invite-code IVT-INVALID`
  - expected: `UZ-INVITE-001 Invalid invite code. Contact support or request a new one.`; exit 1
  - test_type: CLI integration

---

## 4.0 API Path (Agent / Pipeline)

**Status:** PENDING

Full programmatic path. An agent or CI pipeline can provision a UseZombie workspace without human intervention once it holds an invite code.

```bash
# Redeem invite, create account, get workspace token
curl -X POST https://api.usezombie.com/v1/auth/signup \
  -H "Content-Type: application/json" \
  -d '{
    "invite_code": "IVT-ZMB-4F9A",
    "email": "ci-bot@acme.com",
    "workspace_name": "acme-ci",
    "password": "..."
  }'

# Response:
{
  "workspace_id": "ws_01abc",
  "token": "wt_...",
  "credits": { "initial": 20, "remaining": 20 },
  "expires_at": null
}

# Use workspace token for all subsequent operations
zombiectl zombie create --name "lead-collector" ...  # or POST /v1/workspaces/ws_01abc/zombies
```

**Dimensions:**
- 4.1 PENDING
  - target: `POST /v1/auth/signup`
  - input: valid invite_code, email, workspace_name, password
  - expected: 201 `{ workspace_id, token, credits }` — account created, credits loaded
  - test_type: integration
- 4.2 PENDING
  - target: `POST /v1/auth/signup` with already-redeemed code
  - expected: 409 `{ code: "UZ-INVITE-002", message: "Invite already redeemed" }`
  - test_type: integration
- 4.3 PENDING
  - target: `POST /v1/auth/signup` with expired code
  - expected: 410 `{ code: "UZ-INVITE-003", message: "Invite expired" }`
  - test_type: integration
- 4.4 PENDING
  - target: credits loaded after signup
  - input: workspace created via API signup with 20-credit invite
  - expected: `GET /v1/workspaces/{ws}/credits` returns `{ total: 20, used: 0, remaining: 20 }`
  - test_type: integration

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 New API Endpoints

```
POST /v1/invites                          — admin/zombie creates invite code
GET  /v1/invites                          — list invites (admin)
GET  /v1/invites/{code}                   — check code validity + credits (public, pre-signup)
POST /v1/auth/signup                      — redeem invite, create account + workspace
GET  /v1/workspaces/{ws}/credits          — credit balance (already in M15_001 metering)
```

### 5.2 Error Contracts

| Error condition | Code | HTTP |
|---|---|---|
| Invite code not found | `UZ-INVITE-001` | 404 |
| Invite already redeemed | `UZ-INVITE-002` | 409 |
| Invite expired | `UZ-INVITE-003` | 410 |
| Workspace name taken | `UZ-INVITE-004` | 409 |
| Below credit threshold for auto-approve | triggers gate, not an error | — |

### 5.3 Invite Code Format

`IVT-ZMB-{8 uppercase alphanumeric chars}` — e.g., `IVT-ZMB-4F9A2C1E`

- Unique, checked at insertion
- Stored hashed in the DB; full code returned only at creation time
- URL-safe (no ambiguous chars O/0/I/l)

---

## 6.0 Implementation Constraints (Enforceable)

| Constraint | How to verify |
|---|---|
| Invite codes single-use | Integration test: second redemption returns 409 |
| Code stored hashed (not plaintext in DB) | grep schema for hash; code review |
| Credit grant is atomic with workspace creation | Integration test: kill after workspace create but before credit write → verify rollback |
| `POST /v1/invites` callable by zombie via `/v1/execute` | Dim 1.2 test |
| CLI non-interactive mode works headless | Dim 3.2 test (CI simulation) |
| Invite check endpoint is public (no auth required) | `GET /v1/invites/{code}` returns validity without token |

---

## 7.0 Execution Plan (Ordered)

| Step | Action | Verify |
|---|---|---|
| 1 | Schema: `invites` table (code_hash, credit_value, status, expires_at, redeemed_by, redeemed_at) | `zig build` |
| 2 | `POST /v1/invites` handler — admin creates invite | dim 1.1 |
| 3 | `GET /v1/invites/{code}` — public validity check | dim 2.1 pre-check call |
| 4 | `POST /v1/auth/signup` — redeem invite, create account+workspace+credits | dims 4.1–4.4 |
| 5 | Wire zombie execute path to `/v1/invites` | dim 1.2 |
| 6 | Approval gate for high-value invites | dim 1.4 |
| 7 | Web UI: `/signup` page | dims 2.1–2.4 |
| 8 | CLI: `zombiectl auth signup` | dims 3.1–3.3 |
| 9 | Admin invite list page in dashboard (Settings > Invites) | dim 1.3 |
| 10 | Full test gate + cross-compile | all dims pass |

---

## 8.0 Acceptance Criteria

- [ ] Lead Nurturer Zombie can generate an invite code via `/v1/execute` — verify: dim 1.2
- [ ] Web signup: valid code → credits loaded → dashboard — verify: dim 2.2 e2e
- [ ] CLI signup: `--non-interactive --output-token` works in CI — verify: dim 3.2
- [ ] API signup: full programmatic path — verify: dim 4.1
- [ ] Second redemption of same code → 409 — verify: dim 2.4
- [ ] High-value invite triggers approval gate — verify: dim 1.4
- [ ] Credits visible in dashboard after signup — verify: M12 spend tracker integration

---

## Applicable Rules

RULE FLL, RULE FLS (drain), RULE XCC (cross-compile Zig), RULE TXN (atomic workspace+credit creation).

---

## Eval Commands

```bash
# E1: Zig build
zig build 2>&1 | head -5; echo "zig_build=$?"

# E2: Tests
make test 2>&1 | tail -5
make test-integration 2>&1 | grep -i invite | tail -10

# E3: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E4: Gitleaks
gitleaks detect 2>&1 | tail -3

# E5: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

---

## Verification Evidence

| Check | Command | Result | Pass? |
|---|---|---|---|
| Zig build | `zig build` | | |
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- Magic link signup (email link instead of code entry) — code entry is explicit and auditable
- Social OAuth signup (GitHub, Google) — defer to V3
- Invite code sharing limits (e.g., one code can be split N ways) — single-use only for V1
- Referral program (user generates invite codes for friends) — admin-generated only for V1
- Invite code expiry notification emails — V2
