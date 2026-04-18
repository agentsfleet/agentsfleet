# M28_001: Unified Webhook Auth Middleware — provider-agnostic, config-driven, zero-code scaling

**Prototype:** v0.18.0
**Milestone:** M28
**Workstream:** 001
**Date:** Apr 17, 2026
**Status:** DONE
**Priority:** P1 — Customer-facing webhook reliability and security unification
**Batch:** B1
**Branch:** feat/m28-webhook-auth-middleware
**Depends on:** M18_002 (middleware chain infrastructure)

---

## Overview

**Goal (testable):** All webhook auth (URL secret, Bearer token, HMAC signature) runs through a single middleware that resolves the provider from the zombie's `config_json` trigger config — no per-provider code for HMAC-SHA256 providers, and `webhooks.zig` handler contains zero auth logic.

**Problem:** Webhook authentication is scattered across three locations:
1. `webhooks.zig` handler — `verifyWebhookAuth()` does URL secret (vault-backed) and Bearer token fallback inline.
2. `webhook_hmac.zig` middleware — handles `x-signature` HMAC for approval webhooks only.
3. `slack_events.zig` handler — calls `webhook_verify.verifySignature()` inline, reads `SLACK_SIGNING_SECRET` from env on every request.

This means: (a) three copies of constant-time comparison logic, (b) Slack reads env per-request instead of at boot, (c) adding a new HMAC provider (Jira, Asana, Stripe) requires touching handler code, and (d) no single place to audit webhook auth.

**Solution summary:** Create a unified `WebhookAuth` middleware in `src/auth/middleware/` that:
1. For `/v1/webhooks/{zombie_id}/{secret}` routes: resolves URL secret from vault via `webhook_secret_ref`, falls back to Bearer token — replaces inline auth in `webhooks.zig`.
2. For HMAC-signed webhooks: detects the provider by probing signature headers against a `[]const VerifyConfig` registry (existing `webhook_verify.zig` consts), performs HMAC verification + timestamp freshness.
3. For future providers: users declare `signature.header` and `signature.prefix` in TRIGGER.md frontmatter → stored in `config_json` → middleware builds `VerifyConfig` at request time. Zero code changes for any new HMAC-SHA256 provider.

Slack's native handler stays separate (it needs challenge handshake, bot-loop prevention, team→workspace routing — business logic, not auth) but its signature verification calls the same `webhook_verify` path through the middleware instead of inline.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/auth/middleware/webhook_sig.zig` | CREATE | Unified webhook auth middleware — URL secret + Bearer + HMAC. Comptime-generic on `LookupCtx` (RULE NTE — no `*anyopaque`). |
| `src/auth/middleware/webhook_sig_test.zig` | CREATE | 13 unit tests covering URL secret, Bearer, HMAC negative paths, edge cases. |
| `src/cmd/serve_webhook_lookup.zig` | CREATE | Real pg+vault `WebhookLookupFn` implementation, extracted from `serve.zig` for RULE FLL. |
| `src/http/handlers/webhooks.zig` | MODIFY | Remove `verifyWebhookAuth`, `constantTimeEq`, `verifyBearerToken` — delegate to middleware |
| `src/http/handlers/slack_events.zig` | MODIFY | Remove inline `verifySlackSignature` — delegate to middleware |
| `src/http/route_table.zig` | MODIFY | Wire `receive_webhook` to use new middleware instead of `none` |
| `src/zombie/webhook_verify.zig` | MODIFY | Add `PROVIDER_REGISTRY` array and `detectProvider` helper |
| `src/zombie/config.zig` | MODIFY | Add optional `signature` fields to `ZombieTrigger.webhook` |
| `src/zombie/config_helpers.zig` | MODIFY | Parse new `signature` fields from trigger config |

## Applicable Rules

- RULE CTM — Constant-time comparison for secrets (core to this spec)
- RULE OWN — One owner per resource (vault secret lifecycle)
- RULE FLS — Flush all layers / drain pg results
- RULE XCC — Cross-compile before commit
- RULE FLL — Files ≤ 350 lines
- RULE ORP — Cross-layer orphan sweep (removing `verifyWebhookAuth` et al.)
- RULE NDC — No dead code (removed inline auth functions)
- RULE NSQ — Named constants, schema-qualified SQL

---

## Sections (implementation slices)

### §1 — Unified WebhookAuth Middleware

**Status:** DONE

Create `src/auth/middleware/webhook_sig.zig` implementing `chain.Middleware(AuthCtx)`. The middleware handles three auth strategies in priority order: (1) URL-embedded secret (vault-backed), (2) HMAC signature header detection against provider registry, (3) Bearer token fallback. All comparisons use constant-time XOR (RULE CTM).

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | DONE | `webhook_sig.zig:execute` | URL secret matches vault secret | `.next` (auth passes) | unit |
| 1.2 | DONE | `webhook_sig.zig:execute` | URL secret does NOT match vault | `.short_circuit` + 401 | unit |
| 1.3 | DONE | `webhook_sig.zig:execute` | No URL secret, valid Bearer token | `.next` (fallback passes) | unit |
| 1.4 | DONE | `webhook_sig.zig:execute` | No URL secret, invalid Bearer token | `.short_circuit` + 401 | unit |

### §2 — HMAC Provider Detection and Verification

**Status:** DONE

Add `PROVIDER_REGISTRY: []const VerifyConfig` to `webhook_verify.zig` — a comptime array of all known providers. Add `detectProvider(headers) → ?VerifyConfig` that probes headers in priority order. The middleware calls this to auto-detect the provider from request headers when no URL secret or Bearer token is present.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | DONE | `webhook_verify.zig:detectProvider` | Request with `x-slack-signature` header | Returns `SLACK` config | unit |
| 2.2 | DONE | `webhook_verify.zig:detectProvider` | Request with `x-hub-signature-256` header | Returns `GITHUB` config | unit |
| 2.3 | DONE | `webhook_verify.zig:detectProvider` | Request with `linear-signature` header | Returns `LINEAR` config | unit |
| 2.4 | DONE | `webhook_verify.zig:detectProvider` | Request with no known signature header | Returns `null` | unit |

### §3 — Config-Driven Trigger Signature (Zero-Code Scaling)

**Status:** DONE

Extend `ZombieTrigger.webhook` to accept optional `signature` fields: `sig_header`, `sig_prefix`, `ts_header`. When present, the middleware builds a `VerifyConfig` from these fields at request time. This allows any new HMAC-SHA256 provider (Jira, Asana, Stripe, etc.) to work without code changes — the user declares the signature scheme in TRIGGER.md.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | DONE | `config_helpers.zig:parseZombieTrigger` | Trigger with `signature.header: "x-jira-hook"` | Parses into webhook with signature config | unit |
| 3.2 | DONE | `config_helpers.zig:parseZombieTrigger` | Trigger without `signature` block | Parses with null signature (backward compat) | unit |
| 3.3 | DONE | `webhook_sig.zig:execute` | Custom `VerifyConfig` from config + valid HMAC | `.next` (auth passes) | unit |
| 3.4 | DONE | `webhook_sig.zig:execute` | Custom `VerifyConfig` from config + invalid HMAC | `.short_circuit` + 401 | unit |

### §4 — Handler Cleanup and Route Wiring

**Status:** DONE

Remove inline auth from `webhooks.zig` (delete `verifyWebhookAuth`, `constantTimeEq`, `verifyBearerToken`). Remove inline `verifySlackSignature` from `slack_events.zig`. Update `route_table.zig` to wire `receive_webhook` through the new middleware. Verify no orphaned references (RULE ORP).

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | DONE | `webhooks.zig` | After cleanup | Zero auth functions remain in handler | inspection |
| 4.2 | DONE | `route_table.zig` | `receive_webhook` route | Uses `WebhookSig(*pg.Pool)` middleware via registry, not `none` | unit |
| 4.3 | DONE | `slack_events.zig` / `slack_interactions.zig` | After cleanup | `verifySlackSignature` removed from both handlers; sig check routes through middleware | inspection |
| 4.4 | DONE | orphan sweep | `grep -rn "verifyWebhookAuth\|verifyBearerToken\|verifySlackSignature" src/` | 0 matches in non-test code | inspection |

---

## Interfaces

**Status:** DONE

### Public Functions

```zig
// src/auth/middleware/webhook_sig.zig — comptime-generic on LookupCtx (RULE NTE)
pub fn WebhookSig(comptime LookupCtx: type) type {
    return struct {
        const Self = @This();
        lookup_ctx: LookupCtx,
        lookup_fn: *const fn (ctx: LookupCtx, allocator: std.mem.Allocator, zombie_id: []const u8) anyerror!?WebhookSecret,
        config: WebhookSignatureConfig,

        pub fn middleware(self: *Self) chain.Middleware(AuthCtx);
        pub fn execute(self: *Self, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome;
    };
}

// src/zombie/webhook_verify.zig (additions)
pub const PROVIDER_REGISTRY: []const VerifyConfig = &.{ SLACK, GITHUB, LINEAR };
pub fn detectProvider(headers: anytype) ?VerifyConfig;
// comptime invariants: unique sig_header, non-empty sig_header across all entries.
```

**Implementation note (deviation from initial spec):** Filename renamed `webhook_sig.zig` → `webhook_sig.zig` to reflect that it is signature-driven auth. The `*anyopaque` lookup context was refactored to a comptime generic parameter per RULE NTE (added to `docs/greptile-learnings/RULES.md` in this branch) — the host passes `*pg.Pool` directly with compile-time type safety.

### Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `webhook_zombie_id` | `?[]const u8` | Set by dispatcher from URL path | `"019abcde-..."` |
| `webhook_provided_secret` | `?[]const u8` | Set by dispatcher from URL path segment | `"s3cr3t-hex-..."` |
| `signature.header` | `?[]const u8` | Optional in TRIGGER.md, max 64 chars | `"x-jira-hook"` |
| `signature.prefix` | `?[]const u8` | Optional, defaults to `""` | `"sha256="` |
| `signature.ts_header` | `?[]const u8` | Optional, null if no timestamp | `"x-jira-timestamp"` |

### Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| Outcome | `.next` | Auth passes any strategy | Middleware passes to handler |
| Outcome | `.short_circuit` | All strategies fail | 401 + error code written |

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| URL secret mismatch | Constant-time reject | 401 `UZ-AUTH-001` |
| Bearer token mismatch | Constant-time reject | 401 `UZ-AUTH-001` |
| HMAC signature invalid | Constant-time reject | 401 `UZ-WH-010` |
| Timestamp stale (>5min) | Reject | 401 `UZ-WH-011` |
| No auth strategy matches | Reject | 401 `UZ-AUTH-001` |
| Vault lookup fails | Reject (fail closed) | 401 `UZ-AUTH-001` |

---

## Failure Modes

**Status:** DONE

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Vault unavailable | DB pool exhausted or vault row missing | Middleware returns `.short_circuit` | 401 — fail closed |
| HMAC secret not loaded at boot | Missing env var | Middleware skips HMAC strategy, falls to next | 401 if no other strategy matches |
| Provider header collision | Request has both `x-slack-signature` and `x-hub-signature-256` | First match in registry wins (Slack) | Consistent behavior |
| Custom signature config malformed | User sets `signature.header` but empty string | Config validation rejects at upload time | 400 from zombiectl |

**Platform constraints:**
- Constant-time comparison runs `min(a.len, b.len)` XOR iterations — leaks shorter length but not value (acceptable for fixed-length secrets per RULE CTM).

---

## Implementation Constraints (Enforceable)

**Status:** DONE

| Constraint | How to verify |
|-----------|---------------|
| Zero auth logic in `webhooks.zig` handler | `grep -rn "verifyWebhookAuth\|constantTimeEq\|verifyBearerToken" src/http/handlers/webhooks.zig` returns 0 |
| All secret comparisons use constant-time XOR | `grep -rn "std.mem.eql.*secret\|==.*token" src/auth/middleware/webhook_sig.zig` returns 0 |
| `webhook_sig.zig` ≤ 350 lines | `wc -l < 350` |
| `webhook_sig_test.zig` covers all 4 auth strategies | `zig build test` — all dim tests pass |
| Cross-compiles | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| No env var reads at request time | `grep -rn "getEnvVarOwned" src/auth/middleware/webhook_sig.zig` returns 0 |

---

## Invariants (Hard Guardrails)

**Status:** DONE

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | `PROVIDER_REGISTRY` entries have unique `sig_header` values | comptime loop checks pair-wise `sig_header` inequality |
| 2 | Every `VerifyConfig` has non-empty `sig_header` | comptime loop asserts `sig_header.len > 0` |

---

## Test Specification

**Status:** DONE

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `url_secret_match_returns_next` | 1.1 | `webhook_sig.zig:execute` | Provided secret = vault secret | `.next` |
| `url_secret_mismatch_returns_401` | 1.2 | `webhook_sig.zig:execute` | Provided secret ≠ vault secret | `.short_circuit` + 401 |
| `bearer_fallback_valid_returns_next` | 1.3 | `webhook_sig.zig:execute` | No URL secret, valid Bearer | `.next` |
| `bearer_fallback_invalid_returns_401` | 1.4 | `webhook_sig.zig:execute` | No URL secret, wrong Bearer | `.short_circuit` + 401 |
| `detect_slack_by_header` | 2.1 | `webhook_verify.zig:detectProvider` | `x-slack-signature` present | `SLACK` |
| `detect_github_by_header` | 2.2 | `webhook_verify.zig:detectProvider` | `x-hub-signature-256` present | `GITHUB` |
| `detect_linear_by_header` | 2.3 | `webhook_verify.zig:detectProvider` | `linear-signature` present | `LINEAR` |
| `detect_unknown_returns_null` | 2.4 | `webhook_verify.zig:detectProvider` | No known headers | `null` |
| `custom_signature_config_parsed` | 3.1 | `config_helpers.zig` | TRIGGER with `signature` block | Signature fields populated |
| `missing_signature_config_is_null` | 3.2 | `config_helpers.zig` | TRIGGER without `signature` | Null signature (backward compat) |
| `custom_hmac_valid_passes` | 3.3 | `webhook_sig.zig:execute` | Custom config + valid HMAC | `.next` |
| `custom_hmac_invalid_rejects` | 3.4 | `webhook_sig.zig:execute` | Custom config + bad HMAC | `.short_circuit` + 401 |

### Negative Tests (error paths that MUST fail)

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| `no_auth_at_all_returns_401` | 1.4 | No URL secret, no Bearer, no HMAC | `.short_circuit` + 401 |
| `vault_lookup_error_returns_401` | 1.2 | Vault returns error | `.short_circuit` + 401 |
| `stale_timestamp_rejected` | 2.1 | Slack header with ts > 5min old | `.short_circuit` + 401 |

### Edge Case Tests (boundary values)

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| `empty_secret_from_vault_rejected` | 1.2 | Vault returns `""` | `.short_circuit` + 401 |
| `zero_length_bearer_rejected` | 1.4 | `Authorization: Bearer ` (empty) | `.short_circuit` + 401 |

### Regression Tests (pre-existing behavior that MUST NOT change)

| Test name | What it guards | File |
|-----------|---------------|------|
| `existing_webhook_handler_202_path` | Webhook handler still returns 202 after auth delegated | `webhooks.zig` existing tests |
| `slack_challenge_still_works` | Slack `url_verification` challenge echo unaffected | `slack_events.zig` |

### Leak Detection Tests

| Test name | Dim | What it proves |
|-----------|-----|---------------|
| `webhook_auth_no_leak_on_vault_success` | 1.1 | std.testing.allocator detects zero leaks for vault lookup + free path |
| `webhook_auth_no_leak_on_vault_failure` | 1.2 | std.testing.allocator detects zero leaks when vault errors |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| All webhook auth through single middleware | `url_secret_match_returns_next` + `bearer_fallback_valid_returns_next` | unit |
| Zero auth logic in handler | `grep verifyWebhookAuth src/http/handlers/webhooks.zig` = 0 | inspection |
| Zero-code scaling for new providers | `custom_hmac_valid_passes` with arbitrary `VerifyConfig` | unit |

---

## Execution Plan (Ordered)

**Status:** DONE

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | Add `PROVIDER_REGISTRY` + `detectProvider` to `webhook_verify.zig` with comptime invariants | `zig build` compiles |
| 2 | Add optional `signature` fields to `ZombieTrigger.webhook` in `config.zig` / `config_helpers.zig` | `zig build && zig build test` |
| 3 | Create `webhook_sig.zig` middleware with URL secret + Bearer + HMAC strategies | `zig build` compiles |
| 4 | Create `webhook_sig_test.zig` covering all dims | `zig build test` (all pass) |
| 5 | Wire `receive_webhook` in `route_table.zig` to use new middleware | `zig build test` |
| 6 | Remove inline auth from `webhooks.zig` (verifyWebhookAuth, constantTimeEq, verifyBearerToken) | `zig build test` |
| 7 | Remove inline `verifySlackSignature` from `slack_events.zig` — route through middleware or direct `webhook_verify` call | `zig build test` |
| 8 | Orphan sweep + cross-compile + lint + gitleaks | All eval commands pass |

---

## Acceptance Criteria

**Status:** DONE

- [x] `webhook_sig.zig` middleware handles URL secret, Bearer, and HMAC auth — verified: `zig build test` (13 tests green)
- [x] `webhooks.zig` contains zero auth functions — verified: grep returns 0
- [x] `detectProvider` returns correct config for Slack/GitHub/Linear headers — verified: `zig build test`
- [x] Custom signature config in TRIGGER.md parses into `VerifyConfig` — verified: `zig build test`
- [x] All existing webhook tests still pass — verified: `make test` + `make test-integration`
- [x] Cross-compile succeeds — verified: x86_64-linux + aarch64-linux
- [x] No leaked secrets — verified: `gitleaks detect` (922 commits scanned, no leaks)
- [x] All new/modified files ≤ 350 lines — verified: webhook_sig.zig=143, webhook_sig_test.zig=232 (test exempt), serve_webhook_lookup.zig=53

---

## Eval Commands (Post-Implementation Verification)

**Status:** DONE

```bash
# E1: No auth logic in webhook handler
grep -rn "verifyWebhookAuth\|constantTimeEq\|verifyBearerToken" src/http/handlers/webhooks.zig | head -5
echo "E1: handler auth cleanup (empty = pass)"

# E2: Dead code sweep — zero orphaned references
grep -rn "verifyWebhookAuth\|verifySlackSignature\|verifyBearerToken" src/ --include="*.zig" | grep -v "_test\." | head -5
echo "E2: orphan sweep (empty = pass)"

# E3: Memory leak test
zig build test 2>&1 | grep -i "leak" | head -5
echo "E3: leak check (empty = pass)"

# E4: Build
zig build 2>&1 | head -5; echo "build=$?"

# E5: Tests
zig build test 2>&1 | tail -5; echo "test=$?"

# E6: Lint
make lint 2>&1 | grep -E "✓|FAIL"

# E7: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E8: Gitleaks
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E9: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'

# E10: check-pg-drain (touching zig files)
make check-pg-drain
```

---

## Dead Code Sweep

**Status:** DONE

**1. Orphaned symbols — must be removed from handlers.**

| Symbol to remove | File | Verify removed |
|-----------------|------|----------------|
| `verifyWebhookAuth` | `src/http/handlers/webhooks.zig` | `grep -rn "verifyWebhookAuth" src/ --include="*.zig"` = 0 |
| `constantTimeEq` (handler copy) | `src/http/handlers/webhooks.zig` | Only copy in `src/auth/middleware/` remains |
| `verifyBearerToken` | `src/http/handlers/webhooks.zig` | `grep -rn "verifyBearerToken" src/ --include="*.zig"` = 0 |
| `verifySlackSignature` | `src/http/handlers/slack_events.zig` | `grep -rn "verifySlackSignature" src/ --include="*.zig"` = 0 |

**2. No files deleted** — symbols removed from existing files only.

**3. main.zig test discovery** — add `_ = @import("auth/middleware/webhook_sig_test.zig");` if test discovery uses explicit imports.

---

## Verification Evidence

**Status:** DONE

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | exit 0 | ✅ |
| Integration tests | `make test-integration` | full suite green | ✅ |
| Leak detection | `zig build test` under `std.testing.allocator` | zero leaks reported | ✅ |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | exit 0 | ✅ |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | exit 0 | ✅ |
| Full lint | `make lint` | All lint checks passed | ✅ |
| pg-drain | `make check-pg-drain` | 268 files scanned, pass | ✅ |
| Gitleaks | `gitleaks detect` | 922 commits, no leaks | ✅ |
| 350L gate | `wc -l` on touched code files | all under limit | ✅ |
| Dead code sweep | `grep -rn "verifyWebhookAuth\|verifyBearerToken\|verifySlackSignature" src/ --include="*.zig"` | 0 non-test hits | ✅ |

---

## Out of Scope

- Slack native handler business logic (challenge, bot-loop, team→workspace routing) — stays in `slack_events.zig`
- Webhook secret rotation API — separate milestone
- Outbound webhook signing (we only receive, not send)
- Rate limiting on webhook endpoints — separate concern
- Provider-specific payload normalization (Jira→generic, Asana→generic) — business logic, not auth
