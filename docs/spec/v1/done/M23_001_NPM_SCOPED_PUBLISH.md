# M23_001: npm Scoped Package Publish

**Prototype:** v1.0.0
**Milestone:** M23
**Workstream:** 001
**Date:** Apr 03, 2026
**Status:** DONE
**Priority:** P0 — Blocking all install paths; end users cannot install from npm
**Batch:** B1 — Must complete before B2 (downstream install paths depend on a working npm release)
**Branch:** feat/m23-cli-distribution
**Depends on:** None

---

## 1.0 Package Scope Fix

**Status:** DONE

The npm publish token in `ZMB_CD_PROD` vault is scoped to the `@usezombie` org. The current package name `zombiectl` is unscoped and outside that scope, causing a 403 Forbidden on every release. Rename the package to `@usezombie/zombiectl` so the existing token can publish without any vault changes.

**Dimensions:**
- 1.1 ✅ DONE `zombiectl/package.json` `name` field set to `@usezombie/zombiectl`
- 1.2 ✅ DONE `zombiectl/README.md` install snippet updated to `npm install -g @usezombie/zombiectl`
- 1.3 ✅ DONE Root `README.md` quick-start snippet updated to `npm install -g @usezombie/zombiectl`
- 1.4 PENDING `bun run build` passes after rename (no module resolution breakage) — verified after CI runs

---

## 2.0 Release CI — npm Job

**Status:** DONE

The `npm` job in `release.yml` must publish `@usezombie/zombiectl` with provenance to the `next` dist-tag. Verify the job reads the correct vault path and that `--access public` is set (required for scoped packages).

**Dimensions:**
- 2.1 ✅ DONE `release.yml` npm job uses `NODE_AUTH_TOKEN` from `op://ZMB_CD_PROD/npm-publish-token/credential`
- 2.2 ✅ DONE `npm publish --provenance --access public --tag next` is the publish command
- 2.3 ✅ DONE `id-token: write` permission present on the npm job (required for provenance)
- 2.4 PENDING After a release tag, `npm view @usezombie/zombiectl dist-tags` shows `next: 0.3.x` — verified after release

---

## 3.0 Post-Publish Promotion Gate

**Status:** DONE

After the `next` tag is validated in acceptance, promote to `latest` via a manual CI job or npm CLI command. Define the promotion command so it is documented and repeatable.

**Dimensions:**
- 3.1 ✅ DONE Promotion command documented: `npm dist-tag add @usezombie/zombiectl@<ver> latest`
- 3.2 ✅ DONE `release.yml` has a comment block explaining `next` vs `latest` tag lifecycle
- 3.3 ✅ DONE `zombiectl/README.md` install snippet uses `@usezombie/zombiectl` (no tag pin — resolves `latest` by default)

---

## 4.0 Acceptance Criteria

**Status:** PENDING — verified after release tag push

- [ ] 4.1 `npm install -g @usezombie/zombiectl@next` succeeds from a clean environment
- [ ] 4.2 `zombiectl --version` prints the expected version string after install
- [ ] 4.3 `npm view @usezombie/zombiectl` shows provenance attestation
- [ ] 4.4 Release CI `npm` job is green in GitHub Actions run log

---

## 5.0 Out of Scope

- Updating playbooks or docs that reference `npx zombiectl` — binary name is unchanged, those invocations remain correct after global install
- Promoting `next` to `latest` automatically — that is a manual gate until acceptance is confirmed
