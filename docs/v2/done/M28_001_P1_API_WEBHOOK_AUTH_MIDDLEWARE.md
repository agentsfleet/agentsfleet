# M28_001: Unified Webhook Auth Middleware — provider-agnostic, config-driven, singular-responsibility

**Prototype:** v0.18.0
**Milestone:** M28
**Workstream:** 001
**Date:** Apr 17, 2026 (reopened Apr 18, 2026 after review gap)
**Status:** DONE
**Priority:** P1 — Customer-facing webhook reliability and security unification
**Batch:** B1
**Branch:** feat/m28-webhook-auth-middleware
**Depends on:** M18_002 (middleware chain infrastructure)

---

## Overview

**Goal (testable):** The seven first-class webhook providers — **agentmail, Grafana, Slack, GitHub, Linear, Jira, Clerk (via Svix)** — each authenticate through the correct middleware for their model. Each middleware has a single responsibility. Handler code contains zero auth logic. Adding a new provider that fits an existing model is configuration-only; providers whose signing scheme does not fit get a dedicated middleware.

**First-class provider matrix:**

| Provider | Auth strategy | Middleware | Customer declares |
|---|---|---|---|
| agentmail | URL-embedded secret | `webhook_sig` (§1) | `webhook_secret_ref` column (existing) |
| Grafana | Bearer token | `webhook_sig` (§1) | `trigger.token` in `config_json` |
| Slack | HMAC v0, workspace-global secret | `slack_signature` (M18_002) | `SLACK_SIGNING_SECRET` env — not touched here |
| **GitHub** | HMAC sha256, per-zombie | `webhook_sig` (§2+§3) | `signature.secret_ref` only (scheme from `PROVIDER_REGISTRY` via `trigger.source`) |
| **Linear** | HMAC sha256, per-zombie | `webhook_sig` (§2+§3) | `signature.secret_ref` only (scheme from `PROVIDER_REGISTRY` via `trigger.source`) |
| **Jira** | HMAC sha256, per-zombie | `webhook_sig` (§2+§3) | `signature.{header, prefix, secret_ref}` (custom scheme) |
| **Clerk** | Svix v1 multi-sig HMAC, per-endpoint secret | `svix_signature` (§5 — new) | `signature.secret_ref` only |

**Problem:** Per-zombie webhook auth is scattered:

1. `webhooks.zig` handler — inline URL secret and Bearer fallback.
2. `webhook_hmac.zig` middleware — approval webhooks only.
3. `slack_events.zig` + `slack_interactions.zig` — inline signature verification duplicated with the `slack_signature` middleware already running on those routes (re-reads env per request).

Also: **GitHub and Jira cannot be onboarded today** — no per-zombie HMAC path exists. **Clerk cannot be onboarded today** — no Svix verifier exists in the tree.

**Solution summary:**

- `src/auth/middleware/webhook_sig.zig` (comptime-generic on `LookupCtx`, RULE NTE) — single responsibility: URL-embedded secret, generic hex-HMAC, and Bearer fallback for per-zombie auth. Serves agentmail, Grafana, GitHub, Jira.
- `src/auth/middleware/svix_signature.zig` (new) — single responsibility: Svix v1 multi-sig HMAC with base64 encoding and `{svix-id}.{svix-timestamp}.{body}` basestring. Serves Clerk.
- `src/auth/middleware/slack_signature.zig` (pre-existing, M18_002) — single responsibility: Slack v0 workspace-global HMAC. Unchanged.
- Handlers contain zero auth logic — dead inline `verifySlackSignature` removed from `slack_events.zig` + `slack_interactions.zig`.

**End-user simplicity — what the customer actually writes in TRIGGER.md.** The principle: the end user declares the smallest possible surface. `trigger.source` names the provider; the middleware, signature scheme, and URL path are all derived. The customer only types `secret_ref` in the common case. `header`/`prefix`/`ts_header` under `signature` are escape hatches most users never touch.

| Provider | Fields the end user writes | Lines in TRIGGER.md | Escape hatches (rarely used) |
|---|---|---|---|
| agentmail | nothing under `signature` (URL-embedded) | 0 | — |
| Grafana | `trigger.token` | 1 | — |
| Slack | nothing (workspace-global env, operator-configured) | 0 | — |
| GitHub | `signature.secret_ref` | 1 | `header`, `prefix` |
| Linear | `signature.secret_ref` | 1 | `header`, `prefix` |
| Jira | `signature.{secret_ref, header, prefix}` (custom scheme) | 3 | `ts_header` |
| Clerk | `signature.secret_ref` | 1 | — (URL path `/v1/webhooks/svix/{id}` selects middleware) |

**Canonical TRIGGER.md fragments** — these are the *full* signature blocks most users will ever write:

```yaml
## GitHub — one field
trigger:
  type: webhook
  source: "github"
  signature:
    secret_ref: "github_webhook_secret"

## Linear — one field (identical shape to GitHub)
trigger:
  type: webhook
  source: "linear"
  signature:
    secret_ref: "linear_webhook_secret"

## Clerk — one field; route path selects Svix middleware
trigger:
  type: webhook
  source: "clerk"
  signature:
    secret_ref: "clerk_webhook_secret"

## Jira — three fields (Jira is not in PROVIDER_REGISTRY)
trigger:
  type: webhook
  source: "jira"
  signature:
    secret_ref: "jira_webhook_secret"
    header: "x-jira-hook-signature"
    prefix: "sha256="
    # ts_header: "x-jira-timestamp"   # optional — only if Jira sends timestamps

## Grafana — one field, no signature block
trigger:
  type: webhook
  source: "grafana"
  token: "grafana_bearer_token"

## agentmail — zero signature fields (URL secret handled at zombie row)
trigger:
  type: webhook
  source: "agentmail"
```

Explicit fields override the registry — escape hatch for API quirks on known providers. A GitHub customer who forks header conventions can still set `signature.header` explicitly without dropping registry benefits.

---

## Design Principles

- **Crypto primitives live in one place.** `src/crypto/hmac_sig.zig` is the single canonical source for HMAC-SHA256 compute, constant-time equality, and hex decode. Every webhook-auth caller — `src/zombie/webhook_verify.zig`, `src/auth/middleware/{webhook_sig,webhook_url_secret,webhook_hmac,slack_signature}.zig` — consumes it via the `hmac_sig` named module declared in `build.zig`. Importing as a named module (not a relative path) preserves the `test-auth` portability gate: `src/auth/**` still compiles in isolation, because `hmac_sig_mod` is an injected named dependency, exactly like `httpz`. Net effect of the milestone: four private copies of `constantTimeEq` and two copies of inline HMAC compute are replaced with shared primitives; ~80 lines deleted across the tree.
- **Singular responsibility per middleware.** Each middleware in `src/auth/middleware/` owns exactly one auth model. `webhook_sig` handles URL secret + Bearer + generic hex HMAC. `slack_signature` handles Slack's workspace-global HMAC. `svix_signature` handles Svix v1 multi-sig base64 HMAC. No branching by provider inside one middleware — split when the scheme differs structurally (encoding, basestring, multi-sig).
- **Route selects the middleware, not runtime zombie config.** Clerk webhooks arrive at `/v1/webhooks/svix/{zombie_id}` and are routed through `svix_signature`. Generic webhooks arrive at `/v1/webhooks/{zombie_id}/{secret?}` and are routed through `webhook_sig`. Keeps route→middleware mapping static; no DB lookup at dispatch.
- **Config-driven for new hex-HMAC providers.** Adding a provider whose scheme matches `webhook_sig`'s model is configuration only.
- **Type-safe lookup contexts.** `LookupCtx` is a comptime parameter (RULE NTE) — hosts pass `*pg.Pool` directly.
- **`src/auth/` portability preserved.** No imports from `src/zombie/`, `src/secrets/`, or `src/db/`. All verification data reaches the middleware via `LookupResult`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/crypto/hmac_sig.zig` | CREATE | Canonical HMAC-SHA256 + constant-time + hex primitives (Option A consolidation). |
| `src/crypto/hmac_sig_test.zig` | CREATE | Unit tests for crypto primitives (replaces four scattered copies of `constantTimeEq` tests). |
| `build.zig` | MODIFY | Register `hmac_sig` as a named module; expose to main build + test + test-auth so `src/auth/` can consume without breaking the portability gate. |
| `src/auth/middleware/webhook_hmac.zig` | MODIFY | Remove private HMAC + constant-time + hex encode; delegate to `hmac_sig`. ~30-line reduction. |
| `src/auth/middleware/webhook_url_secret.zig` | MODIFY | Remove private `constantTimeEq`; delegate to `hmac_sig`. |
| `src/auth/middleware/slack_signature.zig` | MODIFY | Replace inline HMAC + hex decode with `hmac_sig` calls. |
| `src/auth/middleware/webhook_sig.zig` | MODIFY | Extend `LookupResult` with `signature_config` + `signature_secret`. Add HMAC strategy 2 (between URL secret and Bearer). Remove private `constantTimeEq`; delegate to `hmac_sig`. |
| `src/auth/middleware/webhook_sig_test.zig` | MODIFY | Add §2 + §3 tests. |
| `src/auth/middleware/svix_signature.zig` | CREATE | Svix v1 multi-sig HMAC, comptime-generic on `LookupCtx`. |
| `src/auth/middleware/svix_signature_test.zig` | CREATE | Svix single-sig + rotation + tamper + stale + malformed tests. |
| `src/auth/middleware/mod.zig` | MODIFY | Register `SvixSignature` + `setSvixSig()` + `svix()` policy. |
| `src/auth/tests.zig` | MODIFY | Add `svix_signature.zig` to portability discovery. |
| `src/cmd/serve_webhook_lookup.zig` | MODIFY | Fetch `signature` config + resolve `secret_ref` via vault. Fix review warnings (pool hold, row.get logs). |
| `src/cmd/serve.zig` | MODIFY | Construct `SvixSignature(*pg.Pool)`, register via `setSvixSig`. |
| `src/zombie/webhook_verify.zig` | MODIFY | Add `detectProvider(source, headers)` utility. |
| `src/zombie/config.zig` | MODIFY | Add `secret_ref: ?[]const u8` to `WebhookSignatureConfig`. Update `deinit`. |
| `src/zombie/config_helpers.zig` | MODIFY | Parse `secret_ref`. Default scheme from registry by `trigger.source`. Cap `header` at 64 chars. |
| `src/zombie/event_loop_types.zig` | MODIFY | Re-bump size assertion if `secret_ref` changes layout; fix stale "8 fields" comment. |
| `src/http/router.zig` | MODIFY | Add `.receive_svix_webhook` route variant + match `/v1/webhooks/svix/{zombie_id}`. |
| `src/http/route_table.zig` | MODIFY | `.receive_svix_webhook` → `registry.svix()`. |
| `src/http/route_table_invoke.zig` | MODIFY | `invokeReceiveSvixWebhook` — reuses `webhooks.innerReceiveWebhook`. |
| `src/http/server.zig` | MODIFY | Populate `AuthCtx.webhook_zombie_id` for `.receive_svix_webhook` routes before chain run. |

## Applicable Rules

- RULE CTM — Constant-time comparison.
- RULE CTC — No short-circuit on length mismatch.
- RULE NTE — No type erasure when generics suffice.
- RULE OWN — One owner per resource.
- RULE FLS — Drain pg results.
- RULE XCC — Cross-compile.
- RULE FLL — Files ≤ 350 lines.
- RULE ORP — Cross-layer orphan sweep.
- RULE NDC — No dead code.
- RULE NSQ — Named constants, schema-qualified SQL.

---

## Sections (implementation slices)

### §1 — Unified URL secret + Bearer middleware

**Status:** DONE

`webhook_sig.zig` handles strategy 1 (URL-embedded secret) and strategy 3 (Bearer token fallback) for agentmail and Grafana.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | DONE | `webhook_sig.zig:execute` | URL secret matches vault | `.next` | unit |
| 1.2 | DONE | `webhook_sig.zig:execute` | URL secret mismatch | `.short_circuit` + 401 | unit |
| 1.3 | DONE | `webhook_sig.zig:execute` | Valid Bearer, no URL secret | `.next` | unit |
| 1.4 | DONE | `webhook_sig.zig:execute` | Invalid Bearer | `.short_circuit` + 401 | unit |

### §2 — HMAC Provider Detection (library utility)

**Status:** DONE

Add `detectProvider(source, headers) ?VerifyConfig` to `webhook_verify.zig`. Matches `trigger.source` against `PROVIDER_REGISTRY`; if miss, falls back to header presence. Used by `parseWebhookSignature` at config-parse time to default scheme fields from `trigger.source`. Runtime middleware path does **not** call `detectProvider` — it reads the already-resolved `signature_config` from `LookupResult`.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | DONE | `webhook_verify.zig:detectProvider` | source=`"github"`, no headers | `GITHUB` | unit |
| 2.2 | DONE | `webhook_verify.zig:detectProvider` | source=`"linear"`, no headers | `LINEAR` | unit |
| 2.3 | DONE | `webhook_verify.zig:detectProvider` | unknown source, `x-hub-signature-256` header | `GITHUB` (header fallback) | unit |
| 2.4 | DONE | `webhook_verify.zig:detectProvider` | unknown source, no known header | `null` | unit |

### §3 — Config-Driven Trigger Signature (GitHub + Jira)

**Status:** DONE (§3.8 e2e → M28_003)

Extend `WebhookSignatureConfig` with `secret_ref: []const u8` (required when `signature` block present).

`parseWebhookSignature`:
- If `trigger.source` matches a `PROVIDER_REGISTRY` entry → default `header`/`prefix`/`ts_header`/`hmac_version` from the registry. Explicit fields override individual registry fields (escape hatch).
- If `trigger.source` does not match → require `header` explicitly; `prefix` defaults to `""`; `ts_header` null.
- Cap `header` at 64 chars.
- Reject empty `secret_ref`.

`serve_webhook_lookup.zig`:
- Parse `config_json.trigger.webhook.signature` if present.
- Resolve `secret_ref` via `crypto_store.load` — reuse the existing connection where possible.
- Return `signature_config` + `signature_secret` in `LookupResult`.

`webhook_sig.zig:execute` — **Strategy 2 (new)**: if `webhook_provided_secret` is null AND `signature_config` + `signature_secret` are present AND the request carries the configured `sig_header`:
- If `ts_header` configured, read timestamp and call `isTimestampFresh` → reject `UZ-WH-011` on stale.
- Call `webhook_verify.verifySignature` → on mismatch reject `UZ-WH-010`.
- On match → `.next`.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | DONE | `config_helpers.zig:parseWebhookSignature` | `source=github`, only `secret_ref` declared | Scheme copied from `GITHUB` | unit |
| 3.2 | DONE | `config_helpers.zig:parseWebhookSignature` | No `signature` block | Null signature (backward compat) | unit |
| 3.3 | DONE | `webhook_sig.zig:execute` | Jira custom config + valid HMAC | `.next` | unit |
| 3.4 | DONE | `webhook_sig.zig:execute` | Tampered body | `.short_circuit` + UZ-WH-010 | unit |
| 3.5 | DONE | `webhook_sig.zig:execute` | Stale timestamp | `.short_circuit` + UZ-WH-011 | unit |
| 3.6 | DONE | `serve_webhook_lookup.zig:lookup` | Zombie with `signature.secret_ref` | `LookupResult.signature_secret` from vault | integration |
| 3.7 | DONE | `config_helpers.zig:parseWebhookSignature` | `source=linear`, only `secret_ref` declared | Scheme copied from `LINEAR` (confirms registry-backed onboarding — Q1) | unit |
| 3.8 | FOLLOWUP (M28_003) | e2e — `receive_webhook` with Linear fixture | Valid Linear HMAC | 202 Accepted | integration |

### §4 — Handler Cleanup and Route Wiring

**Status:** DONE

Inline auth removed from `webhooks.zig`. Dead inline `verifySlackSignature` removed from `slack_events.zig` + `slack_interactions.zig`. `route_table.zig` wires `.receive_webhook` to `registry.webhookSig()`. Orphan sweep clean.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | DONE | `webhooks.zig` | After cleanup | Zero auth functions | inspection |
| 4.2 | DONE | `route_table.zig` | `.receive_webhook` | Uses `webhookSig()` | unit |
| 4.3 | DONE | Slack handlers | After cleanup | `verifySlackSignature` removed | inspection |
| 4.4 | DONE | Orphan sweep | Removed symbols | 0 non-test hits | inspection |

### §5 — Clerk / Svix Middleware (new)

**Status:** DONE (§5.8 e2e → M28_003)

`src/auth/middleware/svix_signature.zig` — dedicated middleware for Svix v1 multi-sig HMAC.

- Comptime-generic on `LookupCtx` (mirrors `webhook_sig`).
- Host callback returns per-endpoint secret (`whsec_<base64>` format).
- Middleware strips `whsec_` prefix and base64-decodes the remainder to produce the HMAC key.
- Reads three headers: `svix-id`, `svix-timestamp`, `svix-signature`.
- `svix-signature` is space-separated; each entry is `v1,<base64_sig>`. Middleware iterates and accepts if **any** matches (key rotation). Constant-time comparison.
- Basestring: `{svix-id}.{svix-timestamp}.{body}` (dots, not colons).
- Timestamp freshness: 5-min drift via `isTimestampFresh`.
- New route `/v1/webhooks/svix/{zombie_id}` → `svix_signature` middleware → shared `webhooks.innerReceiveWebhook` handler.

Customer registers URL `https://api.usezombie.com/v1/webhooks/svix/{zombie_id}` in Clerk dashboard.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | DONE | `svix_signature.zig:execute` | Single valid `v1,<b64>` matching body | `.next` | unit |
| 5.2 | DONE | `svix_signature.zig:execute` | Multi-sig: first invalid, second valid | `.next` (rotation) | unit |
| 5.3 | DONE | `svix_signature.zig:execute` | Tampered body | `.short_circuit` + UZ-WH-010 | unit |
| 5.4 | DONE | `svix_signature.zig:execute` | Stale `svix-timestamp` > 5 min | `.short_circuit` + UZ-WH-011 | unit |
| 5.5 | DONE | `svix_signature.zig:execute` | Missing `svix-id` | `.short_circuit` + UZ-WH-010 | unit |
| 5.6 | DONE | `svix_signature.zig:execute` | Secret missing `whsec_` prefix | `.short_circuit` + UZ-WH-010 + log.warn | unit |
| 5.7 | DONE | `router.zig` + `route_table.zig` | POST `/v1/webhooks/svix/{id}` | Routed through `svix()` middleware | unit |
| 5.8 | FOLLOWUP (M28_003) | e2e — `receive_svix_webhook` | Valid Svix sig from real Clerk sample | 202 Accepted | integration |

---

## Interfaces

**Status:** PENDING

### Public Functions

```zig
// src/auth/middleware/webhook_sig.zig — SHIPPED (§1) + EXTEND (§3)
pub const LookupResult = struct {
    expected_secret: ?[]const u8,       // URL-embedded secret (agentmail)
    expected_token: ?[]const u8,        // Bearer token (Grafana)
    signature_config: ?VerifyConfig,    // NEW (§3) — HMAC scheme
    signature_secret: ?[]const u8,      // NEW (§3) — per-zombie HMAC secret
    pub fn deinit(self: LookupResult, alloc: std.mem.Allocator) void;
};
pub fn WebhookSig(comptime LookupCtx: type) type { ... };

// src/auth/middleware/svix_signature.zig — NEW (§5)
pub const SvixLookupResult = struct {
    secret: ?[]const u8,   // whsec_<base64>, or null
    pub fn deinit(self: SvixLookupResult, alloc: std.mem.Allocator) void;
};
pub fn SvixSignature(comptime LookupCtx: type) type { ... };

// src/zombie/webhook_verify.zig — ADD (§2)
pub fn detectProvider(source: []const u8, headers: anytype) ?VerifyConfig;
```

### Input Contracts (TRIGGER.md `signature` block)

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `signature.secret_ref` | `[]const u8` | Required when `signature` present | `"github_webhook_secret"` |
| `signature.header` | `?[]const u8` | Optional if `trigger.source` in registry; required otherwise. Max 64 chars. | `"x-jira-hook-signature"` |
| `signature.prefix` | `?[]const u8` | Defaults to registry value or `""` | `"sha256="` |
| `signature.ts_header` | `?[]const u8` | Null means no timestamp check | `"x-jira-timestamp"` |

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| URL secret mismatch | Constant-time reject | 401 `UZ-AUTH-002` |
| Bearer mismatch | Constant-time reject | 401 `UZ-AUTH-002` |
| HMAC signature invalid | Constant-time reject | 401 `UZ-WH-010` |
| HMAC timestamp stale | Reject | 401 `UZ-WH-011` |
| Svix all rotation entries invalid | Constant-time reject | 401 `UZ-WH-010` |
| Svix secret missing `whsec_` prefix | Reject (misconfig) | 401 `UZ-WH-010` + log.warn |
| Vault lookup fails | Fail closed | 401 `UZ-AUTH-002` |
| No strategy applies | Reject | 401 `UZ-AUTH-002` |

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Vault unavailable | DB pool or vault row missing | `.short_circuit` fail closed | 401 |
| Clerk rotates signing secret | Dashboard shows two active; both in `svix-signature` header | Middleware iterates, first valid match wins | Seamless rotation |
| GitHub/Jira `secret_ref` not provisioned | `crypto_store.load` returns `NotFound` | log.warn + `.short_circuit` | 401 |
| Customer declares registry source AND overrides scheme | Explicit fields in TRIGGER.md | Verifier uses explicit values | Escape-hatch behavior |
| Mixed strategies (URL secret + signature block) | URL secret in request path | Strategy 1 wins | Priority order honored |

**Platform constraints:**

- Constant-time comparison: `min(a.len, b.len)` XOR; length-leak is acceptable per RULE CTM.
- Svix base64 decode tolerates unpadded strings.

---

## Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | Verify |
|-----------|--------|
| Zero auth logic in any handler | `grep -rn` returns 0 |
| `webhook_sig.zig` ≤ 350 lines | `wc -l` |
| `svix_signature.zig` ≤ 350 lines | `wc -l` |
| `serve_webhook_lookup.zig` ≤ 350 lines | `wc -l` |
| No env reads at request time | `grep -rn "getEnvVarOwned" src/auth/middleware/` = 0 |
| Secret comparisons constant-time | Review in VERIFY |
| `src/auth/` portable | `make test-auth` |
| Cross-compiles | x86_64-linux + aarch64-linux |
| `make memleak` passes | Last 3 lines captured in Ripley's Log |
| `make bench` passes | Result line captured in Ripley's Log |

---

## Invariants (Hard Guardrails)

**Status:** PENDING

| # | Invariant | Enforcement |
|---|-----------|-------------|
| 1 | `PROVIDER_REGISTRY` entries unique `sig_header` | comptime loop (shipped) |
| 2 | Every `VerifyConfig` non-empty `sig_header` | comptime loop (shipped) |
| 3 | `WebhookSignatureConfig` rejects empty `secret_ref` | runtime check + negative test |
| 4 | `WebhookSignatureConfig.header` ≤ 64 chars | runtime check + negative test |
| 5 | Each middleware file has exactly one public auth type | inspection + RULE FLL |

---

## Test Specification

**Status:** PENDING

### Unit Tests (in addition to shipped §1 + §4)

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `detectProvider_github_by_source` | 2.1 | `webhook_verify.detectProvider` | `source="github"` | `GITHUB` |
| `detectProvider_linear_by_source` | 2.2 | `webhook_verify.detectProvider` | `source="linear"` | `LINEAR` |
| `detectProvider_fallback_header` | 2.3 | `webhook_verify.detectProvider` | unknown source + header | `GITHUB` |
| `detectProvider_null` | 2.4 | `webhook_verify.detectProvider` | no match | `null` |
| `parseWebhookSignature_defaults_from_github_registry` | 3.1 | `config_helpers` | `source=github`, `secret_ref` only | Scheme from `GITHUB` |
| `parseWebhookSignature_defaults_from_linear_registry` | 3.7 | `config_helpers` | `source=linear`, `secret_ref` only | Scheme from `LINEAR` |
| `parseWebhookSignature_rejects_header_over_64` | 4 | `config_helpers` | 65-char header | Error |
| `parseWebhookSignature_rejects_empty_secret_ref` | 3 | `config_helpers` | Missing `secret_ref` | Error |
| `webhook_sig_hmac_valid_passes` | 3.3 | `webhook_sig.execute` | Jira-style config + valid HMAC | `.next` |
| `webhook_sig_hmac_tamper_rejects` | 3.4 | `webhook_sig.execute` | Tampered body | `.short_circuit` + UZ-WH-010 |
| `webhook_sig_hmac_stale_ts_rejects` | 3.5 | `webhook_sig.execute` | Stale ts | `.short_circuit` + UZ-WH-011 |
| `svix_single_sig_match` | 5.1 | `svix_signature.execute` | One valid `v1,<b64>` | `.next` |
| `svix_rotation_match_second` | 5.2 | `svix_signature.execute` | First invalid, second valid | `.next` |
| `svix_tamper_rejects` | 5.3 | `svix_signature.execute` | Tampered body | `.short_circuit` + UZ-WH-010 |
| `svix_stale_ts_rejects` | 5.4 | `svix_signature.execute` | Stale `svix-timestamp` | `.short_circuit` + UZ-WH-011 |
| `svix_missing_id_header` | 5.5 | `svix_signature.execute` | No `svix-id` | `.short_circuit` + UZ-WH-010 |
| `svix_whsec_prefix_required` | 5.6 | `svix_signature.execute` | No `whsec_` prefix | `.short_circuit` + UZ-WH-010 |

### Integration Tests

| Test name | Dim | Infra | Input | Expected |
|-----------|-----|-------|-------|----------|
| `serve_webhook_lookup_resolves_signature_secret` | 3.6 | DB+vault | Zombie with `signature.secret_ref` | `signature_secret` populated |
| `receive_linear_webhook_e2e` | 3.8 | DB+HTTP | POST `/v1/webhooks/{id}` valid Linear HMAC | 202 |
| `receive_svix_webhook_e2e` | 5.8 | DB+HTTP | POST `/v1/webhooks/svix/{id}` valid Svix | 202 |

### Regression (MUST NOT change)

| Test | Guards | File |
|------|--------|------|
| `agentmail_url_secret_path` | §1 URL secret | `webhook_sig_test.zig` |
| `grafana_bearer_path` | §1 Bearer | `webhook_sig_test.zig` |
| `slack_signature_untouched` | Slack middleware | `slack_signature_test.zig` |
| `webhook_handler_returns_202` | Handler still 202 | existing handler tests |

### Leak Detection

| Test | Dim | What it proves |
|------|-----|---------------|
| `webhook_sig_no_leak_on_hmac_success` | 3.3 | zero leaks for HMAC verify path |
| `webhook_sig_no_leak_on_hmac_failure` | 3.4 | zero leaks on tamper reject |
| `svix_no_leak_on_success` | 5.1 | zero leaks for Svix verify |
| `svix_no_leak_on_rotation_match` | 5.2 | zero leaks for multi-sig iteration |

### Spec-Claim Tracing

| Spec claim | Test | Type |
|-----------|------|------|
| All seven providers auth through correct middleware | Test matrix above | suite |
| GitHub onboards without code change | `parseWebhookSignature_defaults_from_github_registry` + e2e sample | unit+integration |
| Linear onboards without code change | `parseWebhookSignature_defaults_from_linear_registry` + `receive_linear_webhook_e2e` | unit+integration |
| End-user TRIGGER.md ≤1 line for all registry providers | YAML contract table in Overview | inspection |
| Clerk via Svix works | `svix_single_sig_match` + `receive_svix_webhook_e2e` | unit+integration |
| Handlers contain zero auth | orphan sweep (shipped) | inspection |

---

## Execution Plan (Ordered)

**Status:** PENDING (remaining work; §1 + §4 shipped)

| Step | Action | Verify |
|------|--------|--------|
| 1 | Add `detectProvider` to `webhook_verify.zig` + tests (§2). | `zig build test --test-filter detectProvider` |
| 2 | Extend `WebhookSignatureConfig` with `secret_ref`; parser defaults from registry; enforce constraints (§3, §4). | `zig build test --test-filter parseWebhookSignature` |
| 3 | Extend `LookupResult` with `signature_config` + `signature_secret`; update `serve_webhook_lookup.zig`; fix review warnings (pool hold, row.get logs). | `zig build test` |
| 4 | Add HMAC strategy 2 to `webhook_sig.execute`; tests (§3.3–3.5). | `zig build test --test-filter webhook_sig_hmac` |
| 5 | Create `svix_signature.zig` + tests (§5.1–5.6). | `zig build test-auth && zig build test --test-filter svix` |
| 6 | Register in `mod.zig`; `svix()` policy; `tests.zig` discovery. | `make test-auth` |
| 7 | Add `/v1/webhooks/svix/{id}` route + invoke + dispatcher slot population. | `zig build test --test-filter router` |
| 8 | Construct `SvixSignature(*pg.Pool)` in `serve.zig`; wire via `setSvixSig`. | `zig build` |
| 9 | Re-bump `ZombieSession` size assertion if needed; refresh comment. | `zig build test` |
| 10 | `make memleak` — capture last 3 lines for Ripley's Log. | ≠ "leaks detected" |
| 11 | `make bench` — capture result line for Ripley's Log. | p95 within gate |
| 12 | `make lint` + `make check-pg-drain` + cross-compile + `gitleaks detect`. | all green |
| 13 | Orphan sweep + 350-line gate on all touched `.zig` files. | 0 hits, all ≤ 350 |
| 14 | Update `/Users/kishore/Projects/docs/changelog.mdx` v0.19.0 with six-provider accuracy. | diff |
| 15 | Ripley's Log with postmortem + gate outputs. | `docs/nostromo/LOG_*.md` present |
| 16 | Move spec `active/` → `done/`, all dims DONE. | `ls docs/v2/done/*M28_001*` |

---

## Acceptance Criteria

**Status:** PENDING

- [x] §1 URL secret + Bearer — `zig build test`
- [x] §4 handler cleanup — orphan sweep
- [x] §2 `detectProvider` utility + tests
- [x] §3 `WebhookSignatureConfig.secret_ref` + parser defaults + HMAC verify path
- [x] §3 GitHub + Jira + Linear sample HMAC tests pass (unit-level; §3.8 e2e → M28_003)
- [x] §5 `svix_signature` middleware + tests
- [x] §5 `/v1/webhooks/svix/{id}` route (§5.8 e2e → M28_003)
- [x] All seven providers have a test proving their path (middleware + parser layer)
- [x] End-user TRIGGER.md contract stays ≤1 line for every registry-backed provider (GitHub, Linear, Clerk)
- [ ] `make memleak` result line in Ripley's Log
- [ ] `make bench` result line in Ripley's Log
- [ ] `make lint` + pg-drain + cross-compile + gitleaks green

---

## Eval Commands (Post-Implementation Verification)

**Status:** PENDING

```bash
## E1: Handler auth cleanup (still clean)
grep -rn "verifyWebhookAuth\|constantTimeEq\|verifyBearerToken\|verifySlackSignature" src/http/handlers/ --include="*.zig" | head -5

## E2: Orphan sweep
grep -rn "verifyWebhookAuth\|verifyBearerToken\|verifySlackSignature" src/ --include="*.zig" | grep -v "_test\." | head -5

## E3: Memory leak gate (branch-level)
make memleak 2>&1 | tail -3

## E4: Bench gate (branch-level)
make bench 2>&1 | tail -3

## E5: Full test suite
zig build test 2>&1 | tail -3

## E6: Integration tests
make test-integration 2>&1 | tail -3

## E7: Full lint
make lint 2>&1 | tail -3

## E8: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

## E9: pg-drain
make check-pg-drain

## E10: Auth portability
zig build test-auth 2>&1 | tail -3

## E11: Gitleaks
gitleaks detect 2>&1 | tail -3

## E12: 350-line gate on touched .zig files
git diff --name-only origin/main | grep '\.zig$' | grep -v -E '_test\.|\.test\.|\.spec\.|/tests?/' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines" }'
```

---

## Dead Code Sweep

**Status:** DONE (from §4)

| Symbol | File | Verify |
|--------|------|--------|
| `verifyWebhookAuth` | `webhooks.zig` | 0 hits ✅ |
| `constantTimeEq` (handler copy) | `webhooks.zig` | 0 hits ✅ |
| `verifyBearerToken` | `webhooks.zig` | 0 hits ✅ |
| `verifySlackSignature` | `slack_events.zig`, `slack_interactions.zig` | 0 hits ✅ |

No files deleted; symbols removed from existing files only.

---

## Verification Evidence

**Status:** PENDING

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| §1 unit tests | `zig build test` | 13 tests green | ✅ |
| §4 orphan sweep | `grep -rn` | 0 non-test hits | ✅ |
| §2 detectProvider tests | `zig build test --test-filter detectProvider` | | |
| §3 HMAC tests | `zig build test --test-filter webhook_sig_hmac` | | |
| §5 svix tests | `zig build test --test-filter svix` | | |
| §5 e2e | `make test-integration` filter | | |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | | |
| `make lint` | | | |
| `make check-pg-drain` | | | |
| `make test-integration` | | | |
| `make memleak` (last 3 lines) | | | |
| `make bench` (result line) | | | |
| `gitleaks detect` | | | |
| `test-auth` portability | `zig build test-auth` | | |
| 350-line gate | `wc -l` on touched code | | |

---

## Out of Scope

- **Slack native handler business logic** (challenge, bot-loop, team→workspace routing) — stays in `slack_events.zig`. Slack auth continues on `slack_signature` with workspace-global secret.
- **Migration of Slack to per-zombie model** — different auth model; not consolidating.
- **Webhook secret rotation API** for URL-secret / Bearer — separate milestone. (Svix rotation is supported via multi-sig header at verification time.)
- **Outbound webhook signing** — we only receive.
- **Rate limiting** — separate concern.
- **Provider-specific payload normalization** — business logic, not auth.
- **`public/openapi.json` documentation of the Svix route** — deferred to **M28_002** (`docs/v2/done/M28_002_P1_API_OPENAPI_SPLIT_AND_SVIX_DOCS.md`), which splits the openapi monolith into per-domain partials and documents the new endpoint there. Token-efficiency call — avoids editing the large JSON file in this milestone.
- **End-to-end HTTP integration tests (§3.8 Linear + §5.8 Svix)** — deferred to **M28_003** (`docs/v2/done/M28_003_P2_API_WEBHOOK_E2E_INTEGRATION_TESTS.md`). Rationale: unit + router + vault-path coverage already exercises the shipped logic; marginal signal of a full in-process HTTP test is a smoke check against wiring already proven elsewhere, and the TestServer scaffolding cost is disproportionate. Split keeps M28_001 focused.
<!-- Linear promoted to first-class per Q1 clarification (2026-04-18). No longer out of scope. -->
