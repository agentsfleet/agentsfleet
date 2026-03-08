# M6_005: GitHub CI and Release Pipeline

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 005
**Date:** Mar 08, 2026
**Status:** PENDING
**Priority:** P0 — release and quality gate
**Depends on:** M4_001 (Implement `zombiectl` CLI Runtime), M6_003 (Zig API Memleak and Perf Gates), M6_006 (V1 Acceptance E2E Gate)

---

## 1.0 Pull Request CI Contract

**Status:** PENDING

Define deterministic PR validation for all shipped facets so merges cannot bypass core quality gates.

**Dimensions:**
- 1.1 PENDING Zig path in PR CI runs `lint-zig`, `test-zig`, `cross-compile-zig`, `memleak-zig`, and `coverage-zig`
- 1.2 PENDING Website path in PR CI runs `lint-website`, unit `test-website` with coverage upload, and smoke `e2e-website`
- 1.3 PENDING Apps/CLI path in PR CI runs `lint-apps`, unit `test-apps` with coverage upload, and smoke `e2e-apps`
- 1.4 PENDING Coverage uploads use explicit Codecov slug plus component flags (`zombied`, `website`, `apps`) to prevent cross-repo attribution

---

## 2.0 Tag Release Contract

**Status:** PENDING

On version tag push, rerun release-critical gates and publish all required distribution artifacts.

**Dimensions:**
- 2.1 PENDING Release workflow verifies tag/version contract before any publish step
- 2.2 PENDING Release workflow cross-compiles `zombied` binaries and attaches them to GitHub Release artifacts
- 2.3 PENDING Release workflow publishes `zombiectl` CLI package to npm as a first-class release step
- 2.4 PENDING Release workflow builds and pushes stable runtime container images used by production deployments

---

## 3.0 Verification Units

**Status:** PENDING

**Dimensions:**
- 3.1 PENDING CI contract test: PR into default branch executes required jobs for Zig, website, and apps paths
- 3.2 PENDING Coverage contract test: Codecov upload URLs resolve to `usezombie/usezombie` with correct component flags
- 3.3 PENDING Release contract test: tag `vX.Y.Z` produces release notes, binary assets, npm publish, and container push logs
- 3.4 PENDING Operator evidence pack includes command logs and links for CI run, release run, and published artifacts

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 PR merges are blocked unless all required CI jobs pass for touched components
- [ ] 4.2 Release tag flow is human/agent triggerable and deterministic end-to-end
- [ ] 4.3 `zombiectl` npm publish is part of the release workflow and validated post-publish
- [ ] 4.4 GitHub Release contains cross-compiled `zombied` binaries for supported targets
- [ ] 4.5 Coverage reporting is correctly attributed to `usezombie/usezombie` across all component uploads

---

## 5.0 Out of Scope

- Migrating away from GitHub Actions
- Replacing Codecov with another coverage provider
- Non-GitHub package/release registries beyond current npm and container registry targets
