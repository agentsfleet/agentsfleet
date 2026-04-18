# M28_003: End-to-end HTTP integration tests for per-zombie webhook auth (Linear + Svix)

**Prototype:** v0.19.0
**Milestone:** M28
**Workstream:** 003
**Date:** Apr 18, 2026
**Status:** PENDING
**Priority:** P2 — smoke coverage for shipped behaviour; no user-visible change
**Batch:** B1
**Branch:** _unassigned_
**Depends on:** M28_001 (middleware + routes), M18_002 (middleware chain)

---

## Overview

**Goal (testable):** A valid Linear-signed webhook POST to `/v1/webhooks/{zombie_id}` returns 202. A valid Svix-signed webhook POST to `/v1/webhooks/svix/{zombie_id}` returns 202. Both tests run against real Postgres + the in-process HTTP server (mirroring the pattern used by `byok_http_integration_test.zig` and `rbac_http_integration_test.zig`).

**Why deferred from M28_001.** M28_001 shipped the middleware, route, wiring, and vault lookup. Coverage already in place at close:

- 7 unit tests for `svix_signature` covering §5.1–5.6 + freshness boundary.
- 3 unit tests for `webhook_sig` HMAC strategy 2 covering §3.3–3.5.
- Parser unit tests for GitHub + Linear registry defaults (§3.1 + §3.7).
- Router tests proving `/v1/webhooks/svix/{id}` dispatches to `svix()` policy.
- `specFor` table test covers every Route variant including `.receive_svix_webhook`.
- `lookupSvix` reuses the exact `parseSignature` + `crypto_store.load` path already under integration-test coverage via §3.6 (GitHub).
- `test-auth` portability gate green; full build + cross-compile green.

The marginal signal of a full HTTP integration test at this layer is "the in-process HTTP server still returns 202 on the happy path" — largely a smoke test against wiring that is already exercised end-to-end by unit + router + lookup tests. The scaffolding cost (~300 lines of TestServer setup + workspace/zombie/vault fixtures for each case) is disproportionate to the marginal signal, and any bug uncovered would show as a lookup or handler regression in tests that already exist. Splitting this into a dedicated P2 workstream keeps M28_001's diff focused.

---

## Scope

| # | Dim (from M28_001) | Test name | Infra |
|---|---|---|---|
| 1 | 3.8 | `receive_linear_webhook_e2e` | DB + in-process HTTP |
| 2 | 5.8 | `receive_svix_webhook_e2e` | DB + in-process HTTP |

Both tests live in a new `src/http/webhook_http_integration_test.zig`. Names milestone-free per RULE TST-NAM.

---

## Execution Plan

1. Stand up a minimal `WebhookTestServer` helper that wires `MiddlewareRegistry`, `pg.Pool`, `crypto_store`, and the `serve_webhook_lookup` callbacks — likely a ~200-line sibling to `rbac_http_integration_test.zig`'s `TestServer`, scoped to the webhook path so it stays small.
2. `receive_linear_webhook_e2e`:
   - Insert a workspace + zombie with `config_json.trigger = { type: "webhook", source: "linear", signature: { secret_ref: "linear_test_secret" } }`.
   - Store a test secret in the vault under that ref.
   - Sign the canonical Linear payload with HMAC-SHA256 using the raw secret.
   - POST with `linear-signature: sha256=<hex>` header.
   - Assert 202.
3. `receive_svix_webhook_e2e`:
   - Insert a workspace + zombie with `config_json.trigger = { type: "webhook", source: "clerk", signature: { secret_ref: "clerk_test_secret" } }`.
   - Store `whsec_<base64(raw_key)>` in the vault under that ref.
   - Build a valid Svix signature: `v1,base64(HmacSha256(raw_key, "{svix-id}.{svix-timestamp}.{body}"))`.
   - POST to `/v1/webhooks/svix/{zombie_id}` with `svix-id`, `svix-timestamp`, `svix-signature` headers.
   - Assert 202.
4. Negative cases (tamper, stale, missing header) are optional — unit tests already cover these at the middleware layer. Ship happy-path first.

---

## Acceptance Criteria

- [ ] Both e2e tests pass under `make test-integration`.
- [ ] Tests run as part of the standard integration suite (no separate command).
- [ ] Test names and filenames milestone-free per RULE TST-NAM.
- [ ] Touch footprint ≤ 1 new file ≤ 350 lines; no changes to production code.

---

## Applicable Rules

- RULE TST-NAM — Test identifiers are milestone-free.
- RULE FLL — Files ≤ 350 lines; functions ≤ 50 lines.

---

## Out of Scope

- Rewriting `byok_http_integration_test.zig` scaffolding into a shared helper. That refactor is worthwhile on its own but should not block this P2.
- Negative HTTP integration paths (tamper, stale ts, missing headers) — already covered by middleware unit tests.
- Clerk-authenticated webhook flows (user registration + session events end-to-end) — belongs in a Clerk-onboarding workstream, not here.
