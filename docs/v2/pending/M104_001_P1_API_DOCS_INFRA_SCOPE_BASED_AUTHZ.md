<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M104_001: Replace role-based authorization with explicit scopes

**Prototype:** v2.0.0
**Milestone:** M104
**Workstream:** 001
**Date:** Jun 29, 2026
**Status:** PENDING
**Priority:** P1 — foundational security-boundary refactor; M103_001 and future per-capability grants depend on it.
**Categories:** API, DOCS, INFRA
**Batch:** B1 — authorization foundation.
**Depends on:** none (foundation). Blocks M103_001 (template catalog consumes `template:write`).
**Provenance:** agent-generated (Indy design chat, Jun 29, 2026) — gate enumeration by Explore sweep, 65+ decision points.

> **Provenance is load-bearing.** LLM-drafted against a full gate enumeration; re-verify every gate-to-scope mapping against the live route table before EXECUTE. This touches the security boundary on every authenticated route.

**Canonical architecture:** `docs/AUTH.md` (the authorization model this rewrites) and `docs/architecture/roadmap.md §v2.1 — authorization` (scope-based authz, designed-now/enforced-here).

This spec uses Role-Based Access Control (RBAC), JSON Web Token (JWT), Identity Provider (IdP), Insecure Direct Object Reference (IDOR), Pull Request (PR), and Command-Line Interface (CLI) below.

---

## Implementing agent — read these first

1. `src/agentsfleetd/auth/{rbac.zig,principal.zig,claims.zig}` — `AuthRole` ladder, the `platform_admin` bool, and the `scopes` claim that is parsed-but-discarded today (the rail this lights up).
2. `src/agentsfleetd/auth/middleware/{require_role.zig,platform_admin.zig,bearer_or_api_key.zig,runner_bearer.zig}` — the gates being replaced and the principal-construction path.
3. `src/agentsfleetd/http/handlers/common_authz.zig` and `workspace_guards.zig` — the resource/ownership axis (`authorizeWorkspace`, tenant-id isolation) that **stays unchanged**.
4. `docs/AUTH.md` and `docs/architecture/roadmap.md §v2.1` — the model to rewrite and the documented target naming (`fleet:write` colon convention).
5. `~/Projects/oss/auth.md` — reference OAuth-scoped-credential design the roadmap aligns to.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Replace role-based authorization with explicit scopes
- **Intent:** Make every capability a user or credential holds explicit, enumerable, and individually grantable/revocable — so "what can this principal do?" is read off the token, not reconstructed from a role's undocumented meaning.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`; reconcile any mismatch before edits.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — An operator grants a teammate exactly `model:manage` **without** `runner:enroll` (the capability that exposes every tenant's secrets to a trusted-fleet runner); the teammate curates model caps and cannot enroll a host. Least privilege on the one dangerous capability, impossible under today's all-or-nothing `platform_admin`.
2. **Preserved user behaviour** — Every route that works today keeps working for a correctly-scoped principal; tenant isolation and workspace ownership are unchanged; runners keep their self-scoped access.
3. **Optimal-way check** — The direct shape: a principal carries an explicit scope set; one `requireScope` gate per capability; resource ownership stays a separate, independent check. The gap (no per-resource scope syntax, no scope UI) is acceptable now: provisioning bundles cover the common grants.
4. **Rebuild-vs-iterate** — A rebuild of the authorization layer is justified: the role ladder is barely load-bearing (most tenant gates are ownership-only) and `platform_admin` is an opaque capability bundle. Pre-2.0, not in production — the cutover is one milestone, no dual-run.
5. **What we build** — A documented scope catalog, `principal.scopes`, a `requireScope` gate, all 65+ gates migrated, `AuthRole`/`platform_admin` deleted, Clerk emitting explicit scopes, default provisioning bundles, and a rewritten `docs/AUTH.md`.
6. **What we do NOT build** — Per-resource scope strings (`fleet:write:{id}` — ownership stays separate); a scope-management UI; v3 capability tokens; fleet-key principal revamp (roadmap v2.1, separate).
7. **Fit with existing features** — Underpins M103_001 (`template:write`) and every future capability. Must not destabilize the resource/ownership axis (`authorizeWorkspace`, IDOR guards) — those are orthogonal and untouched.
8. **Surface order** — API/backend first; CLI and dashboard inherit the new token claim with no behaviour change for correctly-provisioned principals.
9. **Dashboard restraint** — No scope-editing UI this milestone; scopes are provisioned via Clerk metadata + documented bundles.
10. **Confused-user next step** — A `403` names the missing scope (`requires scope template:write`), and `docs/AUTH.md` lists every scope and what it grants — the enumerable answer that did not exist for roles.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — `NDC` (no dead code), `NLR` (touch-it-fix-it: delete the role ladder fully), `NLG` (no legacy framing), `UFS` (scope strings + claim names as named constants, shared verbatim cross-runtime), `ORP` (orphan sweep on `AuthRole`/`platform_admin`), `FLL` (file/function length), `ECL` (distinct error classes), `ERR` (error registry for scope-denied), `LOG` (auth log discipline, no token leak), `PRI`, `TST-NAM`.
- **`dispatch/write_auth.md`** + **`docs/AUTH.md`** — authoritative; this milestone rewrites the model. Every gate fails closed; no capability widened by accident.
- **`dispatch/write_zig.md`** — middleware, principal, handlers, catalog.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — `403` error shape naming the missing scope; no route-signature drift.
- **`docs/SCHEMA_CONVENTIONS.md`** — only if any scope/grant is persisted (default: scopes ride the token; no schema change).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | Read `dispatch/write_zig.md`; split catalog / gate / migration; cross-compile both Linux targets. |
| PUB / Struct-Shape | yes | Shape verdict on `principal.scopes`, the `Scope` type, and `requireScope`; minimise pub surface. |
| File & Function Length | yes | Scope catalog and gate are their own files; the route-table migration stays per-group. |
| UFS | yes | Scope strings, claim names, and bundle names are named constants — the JWT claim value must match verbatim across Clerk config and Zig. |
| LOGGING / ERROR REGISTRY | yes | New `UZ-AUTH-*` code for scope-denied; auth logs never carry the token or full scope list at info. |
| LIFECYCLE / SCHEMA | conditional | SCHEMA only if a grant table is added (default: none). |

---

## Overview

**Goal (testable):** Every authenticated route authorizes via `requireScope(principal, <scope>)` reading an explicit `principal.scopes`; `AuthRole` and `platform_admin` no longer exist in the codebase; resource-ownership checks are unchanged; and `docs/AUTH.md` lists every scope with the capability it grants.

**Problem:** Authorization is role-based — `AuthRole = user < operator < admin` plus an orthogonal `platform_admin` bool. A role is an undocumented bundle of capabilities: "what can `platform_admin` do?" has no enumerable answer (it is 7 distinct capabilities, never written down). You cannot grant or revoke a single ability, do least-privilege, or separate duties — e.g. you cannot give `model:manage` without also granting `runner:enroll`, which exposes every tenant's secrets.

**Solution summary:** Introduce an explicit scope catalog; carry an explicit scope set on the principal (lighting up the already-parsed `scopes` claim); replace `RequireRole`/`platformAdmin()` with one `requireScope` gate at every one of the 65+ enumerated decision points; delete the role ladder and `platform_admin`; keep the resource/ownership axis untouched; provision scopes in Clerk with documented default bundles. One big-bang cutover, pre-2.0.

---

## Prior-Art / Reference Implementations

- **Target model** — `~/Projects/oss/auth.md` (OAuth-scoped credentials, `api.read`/`api.write` style) and `docs/architecture/roadmap.md §v2.1` (the `fleet:write` colon convention, "grant a capability without a whole role").
- **Claim parsing** — `src/agentsfleetd/auth/claims.zig` already parses `scope`/`scopes`/`scp` (space-delimited or array); surface the result on the principal instead of freeing it.
- **Gate shape** — mirror `auth/middleware/platform_admin.zig` (fail-closed boolean gate) for `requireScope` (fail-closed set membership).
- **Ownership axis** — mirror and preserve `common_authz.zig::authorizeWorkspace` and `workspace_guards.zig` verbatim; scopes do not replace them.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M104_001_*.md` | CREATE | This spec. |
| `src/agentsfleetd/auth/scopes.zig` | CREATE | Scope catalog: every capability as a named constant + default provisioning bundles. |
| `src/agentsfleetd/auth/principal.zig` | EDIT | Add `scopes`; remove `role` and `platform_admin`. |
| `src/agentsfleetd/auth/claims.zig` | EDIT | Surface parsed scopes onto the principal (stop discarding). |
| `src/agentsfleetd/auth/rbac.zig` | DELETE | `AuthRole` ladder removed (legacy waded out). |
| `src/agentsfleetd/auth/middleware/require_scope.zig` | CREATE | The single capability gate; replaces `require_role.zig`. |
| `src/agentsfleetd/auth/middleware/require_role.zig`, `platform_admin.zig` | DELETE | Replaced by `requireScope`. |
| `src/agentsfleetd/auth/middleware/{bearer_or_api_key,runner_bearer,mod}.zig` | EDIT | Construct `scopes` on the principal; runner principal gets `runner:self`. |
| `src/agentsfleetd/http/route_table.zig` | EDIT | Every route's gate becomes a `requireScope`. |
| `src/agentsfleetd/http/handlers/**` | EDIT | The 65+ enumerated gates → `requireScope`; ownership checks unchanged. |
| `src/agentsfleetd/auth/middleware/errors.zig` (+ error registry) | EDIT | `UZ-AUTH-*` scope-denied code naming the missing scope. |
| `docs/AUTH.md`, `docs/architecture/roadmap.md` | EDIT | Rewrite the authorization model; the scope catalog becomes the source of truth; mark v2.1 scope item delivered. |
| `playbooks/founding/03_priming_infra/001_playbook.md` | EDIT | Clerk session-token customization: emit explicit `scopes`; document default bundles. |
| `src/agentsfleetd/**/*test.zig` | EDIT/CREATE | Per-gate scope enforcement, fail-closed, ownership-still-enforced, legacy-deletion grep. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** A foundation rebuild of the capability axis only: scopes replace roles; the resource axis is preserved verbatim. The gate enumeration is the lossless-cutover checklist.
- **Alternatives considered:** Tier→scope expansion server-side (token carries a tier label) — rejected: it keeps a role word and blocks single-capability revoke, the product's key need (separation of duties, incident revoke). Staged dual-run — rejected by Indy: pre-2.0, not in production, so the safety margin isn't worth the prolonged two-model complexity.
- **Patch-vs-refactor verdict:** a **refactor**, scoped to the capability axis. The ownership/IDOR axis is explicitly out of the blast radius. Fleet-key first-class principal and v3 capability tokens are named follow-ups, not bundled.

---

## Sections (implementation slices)

### §1 — Scope catalog and principal scopes

The named capability vocabulary and carrying it on the principal. **Implementation default:** colon convention (`fleet:write`); scope strings are `UFS` constants shared verbatim with Clerk config.

- **Dimension 1.1** — A scope catalog enumerates every capability from the gate sweep (platform: `runner:enroll`/`runner:operate`/`stream:operate`/`platform-key:manage`/`model:manage`; tenant: `fleet:read`/`fleet:write`/`fleet:delete`/`credential:manage`/`apikey:manage`/`fleetkey:manage`/`grant:manage`/`connector:manage`/`approval:resolve`/`billing:read`/`workspace:manage`/`template:write`; credential: `runner:self`) → Test `test_scope_catalog_covers_every_enumerated_gate`
- **Dimension 1.2** — `principal.scopes` is populated from the verified token's parsed scope claim; absent claim yields the empty set → Test `test_principal_scopes_populated_from_claim`
- **Dimension 1.3** — Documented default bundles (`platform_operator`, `tenant_admin`, `tenant_member`, `runner`) expand to explicit scope lists → Test `test_default_bundles_expand_to_documented_scopes`

### §2 — The `requireScope` gate; ownership unchanged

One fail-closed capability gate replaces both role gates; the resource axis is preserved.

- **Dimension 2.1** — `requireScope(p, s)` allows iff `s ∈ p.scopes`, else `403` naming the missing scope; empty scope set is denied → Test `test_require_scope_allows_only_on_membership`
- **Dimension 2.2** — `authorizeWorkspace` / tenant-id isolation / lease ownership / fleet-key identity behave exactly as before, independent of scopes → Test `test_ownership_axis_unchanged`
- **Dimension 2.3** — A capability gate and an ownership gate compose: a principal with `fleet:write` but not owning workspace W is denied W → Test `test_scope_and_ownership_compose`

### §3 — Migrate every gate to scopes

All 65+ decision points cut over, fail-closed, no capability dropped.

- **Dimension 3.1** — Each former `platform_admin` route requires its mapped scope; an api-key principal (no platform scopes) is rejected as today → Test `test_platform_routes_require_platform_scopes`
- **Dimension 3.2** — Each former role/bearer route requires its mapped tenant scope; a principal lacking it gets `403` → Test `test_tenant_routes_require_tenant_scopes`
- **Dimension 3.3** — Runner routes require `runner:self` and nothing tenant/platform satisfies them → Test `test_runner_routes_require_runner_self`

### §4 — Delete the role layer (legacy waded out)

`AuthRole`, `platform_admin`, and their gates removed entirely.

- **Dimension 4.1** — `AuthRole`, `.allows()`/`.atLeast()`, `require_role.zig`, `platform_admin.zig` no longer exist → Test `test_role_layer_fully_removed`
- **Dimension 4.2** — No production reference to `platform_admin` or `AuthRole` remains outside historical specs → Test `test_no_legacy_role_references`

### §5 — Clerk provisioning and documentation

The IdP emits explicit scopes; `docs/AUTH.md` becomes the scope source of truth.

- **Dimension 5.1** — `docs/AUTH.md` lists every scope with the capability it grants and the default bundles → Test `test_authdoc_documents_every_scope`
- **Dimension 5.2** — The Clerk session-token customization and bundle provisioning steps are documented in the priming playbook → Test `test_clerk_scope_provisioning_documented`

---

## Interfaces

```
Principal:  scopes: Set<Scope>    (role + platform_admin REMOVED)
Gate:       requireScope(principal, scope) → allow | 403 "requires scope <name>"
            (capability ONLY; resource ownership remains a separate, unchanged check)
Scope strings (colon convention, UFS constants, verbatim-matched in Clerk):
  platform: runner:enroll runner:operate stream:operate platform-key:manage model:manage
  tenant:   fleet:read fleet:write fleet:delete credential:manage apikey:manage
            fleetkey:manage grant:manage connector:manage approval:resolve billing:read
            workspace:manage template:write
  runner:   runner:self
Token claim: explicit scopes in `scopes` (array) — already parsed by claims.zig.
Default bundles (provisioning convenience, expand to explicit scopes; NOT a runtime role):
  platform_operator · tenant_admin · tenant_member · runner
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Missing scope | Principal lacks the route's scope | `403` naming the required scope; nothing mutated. |
| No scopes claim | Token carries no scopes | Empty scope set → every capability gate fails closed. |
| Api-key reaches platform route | Machine credential without platform scopes | `403`, as `platform_admin` rejected api-keys before. |
| Runner reaches tenant route | `runner:self`-only principal | `403`; no tenant/platform scope present. |
| Cross-tenant resource | Correct scope, wrong workspace owner | Ownership check denies independently of scope. |
| Unknown scope string in token | Malformed/typo claim value | Unknown strings ignored; they grant nothing (deny by absence). |
| Legacy role check survives migration | A gate not cut over | Deletion test + grep fail the build; no role symbol compiles. |
| Scope string drift Clerk vs Zig | Claim value mismatch | UFS constant + a documented exact-match list; mismatch denies (fail closed), caught in integration test. |

---

## Invariants

1. Every authenticated route authorizes via `requireScope` — enforced by route-table test and a grep that no `RequireRole`/`platformAdmin` remains.
2. `AuthRole` and `platform_admin` do not exist in the codebase — enforced by compile (symbols deleted) + orphan-sweep test.
3. The resource/ownership axis is unchanged and independent of scopes — enforced by ownership regression tests and scope-plus-ownership composition tests.
4. Absent or unknown scopes grant nothing (fail closed) — enforced by empty-set and unknown-string tests.
5. Every capability in the gate enumeration maps to exactly one catalog scope; none is dropped — enforced by the catalog-coverage test against the enumeration checklist.
6. The runner credential carries only `runner:self` — enforced by runner-principal construction tests.
7. Scope strings are identical across Clerk config and Zig — enforced by UFS constants and an exact-match integration test.
8. Auth logs never carry the token or be usable to reconstruct a secret — enforced by redaction tests.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_scope_catalog_covers_every_enumerated_gate` | Catalog scope set ⊇ every capability in the enumeration checklist. |
| 1.2 | unit | `test_principal_scopes_populated_from_claim` | Token with `scopes:[fleet:read]` → principal holds exactly that; no claim → empty set. |
| 1.3 | unit | `test_default_bundles_expand_to_documented_scopes` | Each bundle expands to its documented explicit scope list. |
| 2.1 | unit | `test_require_scope_allows_only_on_membership` | `s∈scopes` allows; absent → `403` naming `s`; empty set denies. |
| 2.2 | integration | `test_ownership_axis_unchanged` | `authorizeWorkspace` accepts/denies exactly as before for the same inputs. |
| 2.3 | integration | `test_scope_and_ownership_compose` | `fleet:write` + non-owned W → denied on ownership. |
| 3.1 | integration | `test_platform_routes_require_platform_scopes` | `POST /v1/runners` needs `runner:enroll`; api-key/tenant principal → `403`. |
| 3.2 | integration | `test_tenant_routes_require_tenant_scopes` | Fleet create needs `fleet:write`; principal without it → `403`. |
| 3.3 | integration | `test_runner_routes_require_runner_self` | `/v1/runners/me/*` needs `runner:self`; tenant/platform scopes do not satisfy. |
| 4.1 | unit | `test_role_layer_fully_removed` | `AuthRole`/`require_role.zig`/`platform_admin.zig` symbols absent (build proves it). |
| 4.2 | unit | `test_no_legacy_role_references` | Grep finds no `platform_admin`/`AuthRole` in production outside historical specs. |
| 5.1 | unit | `test_authdoc_documents_every_scope` | `docs/AUTH.md` contains each catalog scope + its capability line + bundles. |
| 5.2 | unit | `test_clerk_scope_provisioning_documented` | Priming playbook documents the session-token scope claim + bundle steps. |

Regression: every existing route's correctly-scoped happy path stays green; tenant isolation tests unchanged. Idempotency/replay: N/A — no new persisted state (scopes ride the token).

---

## Acceptance Criteria

- [ ] Every gate authorizes via `requireScope`; no role gate remains — verify: `make test-integration` and `rg -n "RequireRole|platformAdmin|platform_admin|AuthRole" src` (production-empty).
- [ ] `AuthRole`/`platform_admin` deleted; build green — verify: `make test`.
- [ ] Ownership axis unchanged — verify: ownership regression suite in `make test-integration`.
- [ ] Catalog covers every enumerated capability; fail-closed on absent scope — verify: `make test-unit-agentsfleetd`.
- [ ] `docs/AUTH.md` lists every scope + bundles; playbook documents Clerk provisioning — verify: doc tests + review.
- [ ] Repository gates pass — verify: `make lint && make test && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && gitleaks detect`.

---

## Eval Commands (post-implementation)

```bash
make test-unit-agentsfleetd && make test-integration && make lint && make test
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && gitleaks detect 2>&1 | tail -3
# Legacy sweep (production-empty):
rg -n "RequireRole|platformAdmin|platform_admin|AuthRole|\.atLeast\(|\.allows\(" src | grep -v test
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `src/agentsfleetd/auth/rbac.zig` | `test ! -f src/agentsfleetd/auth/rbac.zig` |
| `src/agentsfleetd/auth/middleware/require_role.zig` | `test ! -f …/require_role.zig` |
| `src/agentsfleetd/auth/middleware/platform_admin.zig` | `test ! -f …/platform_admin.zig` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `AuthRole` | `rg -n "AuthRole" src` | 0 in production. |
| `platform_admin` | `rg -n "platform_admin" src` | 0 in production. |
| `RequireRole` / `platformAdmin()` | `rg -n "RequireRole\|platformAdmin" src` | 0 in production. |

---

## Discovery (consult log)

- Gate enumeration, Jun 29, 2026 (Explore sweep): 65+ decision points — 7 `platform_admin`, 2 operator-role, 21 bearer+ownership, 9 runner self-scoped, plus ownership/IDOR and no-auth/webhook routes. This list is the lossless-cutover checklist (Invariant 5).
- Design decision (Indy, Jun 29, 2026): explicit scopes in the token (not tier→scope expansion) so a single capability can be granted/revoked — required for separation of duties (e.g. `model:manage` without `runner:enroll`), the approver persona, finance read-only, and incident revoke. Default bundles are a provisioning convenience only.
- Design decision (Indy, Jun 29, 2026): big-bang cutover, no staged dual-run — pre-2.0, not in production.
- Two-axis clarification: scopes replace the capability axis (roles); the resource/ownership axis (`authorizeWorkspace`, tenant isolation) is independent and unchanged.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification, especially per-gate fail-closed and ownership composition. | Clean; coverage note in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `docs/AUTH.md`, Failure Modes, Invariants — focus on any gate not cut over. | Clean or every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Reviews the open PR for a missed gate, widened capability, or scope-string drift. | Comments addressed before human review. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-agentsfleetd` | pending | |
| Integration tests | `make test-integration` | pending | |
| Legacy sweep | `rg -n "AuthRole\|platform_admin" src \| grep -v test` | pending | |
| Lint | `make lint` | pending | |
| Test suite | `make test` | pending | |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | pending | |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | pending | |

---

## Out of Scope

- Per-resource scope syntax (`fleet:write:{id}`) — ownership stays a separate axis.
- Scope-management dashboard UI.
- Fleet-key first-class principal revamp (roadmap v2.1, separate).
- v3 agentsfleet-issued capability tokens (separate trajectory).
- Any change to tenant isolation / IDOR guards beyond preserving them.
