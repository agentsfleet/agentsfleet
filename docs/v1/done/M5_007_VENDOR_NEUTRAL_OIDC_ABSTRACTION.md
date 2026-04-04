# M5_007: Vendor-Neutral OIDC Abstraction

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 7
**Date:** Mar 10, 2026
**Status:** DONE
**Priority:** P0 — authentication portability and safety
**Batch:** B3 — follows M5_002 tenant isolation baseline
**Depends on:** M5_002 (Harness control plane), M5_003 (entitlement policy contract)

---

## 1.0 Singular Function

**Status:** DONE

Implement one working auth abstraction function: provider-neutral OIDC authentication and claim normalization with deterministic workspace/tenant authorization behavior.

**Dimensions:**
- 1.1 DONE Define provider-neutral verifier interface (`jwt_oidc`) and canonical `IdentityPrincipal` claim contract
- 1.2 DONE Implement provider adapter routing (`clerk`, `custom`) with startup-time invalid-provider fail-fast behavior
- 1.3 DONE Normalize claim mapping (`tenant_id`, `workspace_id`, `subject`, `issuer`, `audience`, `scopes`) across adapters
- 1.4 DONE Enforce authorization only through canonical normalized claims in HTTP and worker entry points

---

## 2.0 Verification Units

**Status:** DONE

**Dimensions:**
- 2.1 DONE Unit test: happy path `jwt_oidc` authentication succeeds for supported provider adapters
- 2.2 DONE Unit test: invalid `jwt_oidc` token (signature/expiry/audience) fails with stable error mapping
- 2.3 DONE Unit test: invalid provider identifier fails startup/config parsing deterministically
- 2.4 DONE Integration test: workspace-scoped token denies cross-workspace access within same tenant

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 HTTP auth path is provider-neutral and does not reference provider-specific types outside adapters
- [x] 3.2 Workspace and tenant isolation behavior is identical across supported OIDC providers
- [x] 3.3 Startup fails closed for unsupported provider config with actionable error output
- [x] 3.4 Demo evidence captured for provider switch (`clerk` -> alternate provider) without handler-level code changes

Verification evidence:
- `make lint`
- `make test`

---

## 4.0 Out of Scope

- UI flow for credential issuance and rotation
- External IdP tenant provisioning automation
- Non-OIDC auth providers
