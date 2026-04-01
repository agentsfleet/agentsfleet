# M3_006: Implement Clerk Authentication Contract

**Prototype:** v1.0.0
**Milestone:** M3
**Workstream:** 006
**Date:** Mar 06, 2026
**Status:** DONE
**Priority:** P0 — release blocker
**Batch:** B1 — no upstream blockers
**Depends on:** None

---

## 1.0 Singular Function

**Status:** DONE

Implement one working authentication function: Clerk-backed API and CLI auth path with deterministic fallback semantics.

**Dimensions:**
- 1.1 DONE API verifies Clerk JWT signature and required claims
- 1.2 DONE Expired JWT returns deterministic `token_expired` 401
- 1.3 DONE Server-side auth session endpoints (create/poll/complete) for CLI login flow. CLI commands (`zombiectl login/logout`) tracked in M4_001.
- 1.4 DONE Auth session store + HTTP routes enable callback-to-CLI polling flow. Provider-agnostic JWKS verifier (`jwks.zig`) + Clerk claim extractor (`claims.zig`) abstracted from monolithic `clerk.zig`.

---

## 2.0 Verification Units

**Status:** DONE

**Dimensions:**
- 2.1 DONE Unit test: JWT signature and audience validation
- 2.2 DONE Unit test: expired token rejection path
- 2.3 DONE Unit test: OWASP JWT attacks — `alg:none` (CVE-2015-9235), `alg:HS256` switching (CVE-2016-5431), missing kid
- 2.4 DONE Unit test: missing/malformed claims (sub, iss, exp), type confusion, negative/zero exp
- 2.5 DONE Unit test: audience edge cases — array aud, empty array, wrong type, missing field
- 2.6 DONE Unit test: injection payloads — SQL injection in sub, XSS in sub, null bytes, 10KB DoS subject
- 2.7 DONE Unit test: JWKS parsing — truncated JSON, empty modulus, null/string/missing keys field, duplicate kids
- 2.8 DONE Unit test: RS256 signature — wrong modulus length, wrong sig, empty sig, length mismatch
- 2.9 DONE Unit test: Clerk claims extraction — metadata.tenant_id, top-level tenant_id, missing claims, non-JSON, non-object
- 2.10 DONE Unit test: session store — create+poll, complete+poll, unknown session, double complete
- 2.11 DEFERRED Integration test: CLI login -> API authenticated request (requires M4_001 zombiectl)
- 2.12 DEFERRED Integration test: tenant mismatch rejected end-to-end (requires DB + CLI)

---

## 3.0 Acceptance Criteria

**Status:** IN_PROGRESS

- [x] 3.1 Authentication function works end-to-end from server API (CLI side deferred to M4_001)
- [x] 3.2 Failures return stable machine-readable error codes
- [ ] 3.3 Demo evidence captured for auth flow (`login`, authenticated API action, `logout`) — deferred to M4_001

---

## 4.0 Out of Scope

- Website launch-blocker fixes (tracked in M3_007)
- Full RBAC model beyond tenant match
- CLI login/logout commands (tracked in M4_001)
