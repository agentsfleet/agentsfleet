# M23_004: Cross-Platform Install CI (npm only)

**Prototype:** v1.0.0
**Milestone:** M23
**Workstream:** 004
**Date:** Apr 03, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — No Windows or Linux aarch64 machine locally; CI is the only validation path
**Batch:** B3 — Runs after M23_001 (npm release exists)
**Branch:** feat/m23-shell-installers
**Depends on:** M23_001

**Reference implementation:** `~/Projects/oss/resend-cli/.github/workflows/release.yml` and `post-release.yml`

---

## Design Note

M23_003 (shell installers, standalone binaries) is deferred. This workstream validates the **npm distribution channel only** — verifying that `npm install -g @usezombie/zombiectl` works on all four target platforms after every release, and that PRs touching `zombiectl/` don't break installability.

---

## 1.0 post-release.yml — Installation Verification Workflow

**Status:** PENDING

A separate workflow that triggers after `release.yml` completes (via `workflow_run`) and verifies the npm install path against the real published artifact. This is the authoritative gate — it tests what users actually run, not CI fixtures.

**Dimensions:**
- 1.1 PENDING `post-release.yml` triggers on `workflow_run: workflows: [Release] types: [completed]` and only runs when the release workflow succeeded
- 1.2 PENDING Single verification job: `verify-npm` with a 4-platform matrix
- 1.3 PENDING Each runner: `npm install -g @usezombie/zombiectl@next` → `zombiectl --version` → assert output matches `\d+\.\d+\.\d+` and exit code is 0
- 1.4 PENDING Workflow summary step posts a table of platform results as a GitHub Actions job summary

---

## 2.0 npm Verification Matrix

**Status:** PENDING

Install `@usezombie/zombiectl@next` from the real npm registry on all four supported platforms. Because the package is pure JS, the same tarball runs everywhere — this validates Node.js compatibility and PATH wiring across OSes.

Matrix:
| Runner | Install command |
|---|---|
| `macos-latest` | `npm install -g @usezombie/zombiectl@next` |
| `ubuntu-latest` | `npm install -g @usezombie/zombiectl@next` |
| `ubuntu-24.04-arm` | `npm install -g @usezombie/zombiectl@next` |
| `windows-latest` | `npm install -g @usezombie/zombiectl@next` |

**Dimensions:**
- 2.1 PENDING `npm install -g @usezombie/zombiectl@next` succeeds on all four runners
- 2.2 PENDING `zombiectl --version` output matches the tag version on all four runners
- 2.3 PENDING `zombiectl doctor --json` exits 0 (CLI version check only; no auth required)
- 2.4 PENDING `fail-fast: false` on the matrix so all platform results are visible even if one fails

---

## 3.0 PR Gate: install-check.yml

**Status:** PENDING

A fast PR-scoped workflow that runs when `zombiectl/` changes. Validates that the npm package structure is correct and the CLI entry point is loadable. Keeps the feedback loop tight without waiting for a full release.

**Dimensions:**
- 3.1 PENDING `install-check.yml` triggers on `pull_request: paths: [zombiectl/**]`
- 3.2 PENDING Runs `cd zombiectl && bun install && bun run build && node bin/zombiectl.js --version` on `ubuntu-latest` and `windows-latest`
- 3.3 PENDING `--version` and `--help` both exit 0
- 3.4 PENDING Job uses `cancel-in-progress: true` concurrency so stale PR runs are cancelled immediately

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 `post-release.yml` is green across npm × 4 platforms after a release tag
- [ ] 4.2 `install-check.yml` gates PRs touching `zombiectl/`
- [ ] 4.3 GitHub Actions job summary shows a clear pass/fail table per platform

---

## 5.0 Out of Scope

- cURL/PowerShell installers and their CI — deferred with M23_003
- `bun build --compile` standalone binaries — deferred with M23_003
- cosign binary signing — deferred with M23_003 (npm provenance via Sigstore already in M23_001)
- macOS Intel (x86_64) — `macos-latest` is Apple Silicon; Intel is end-of-life
- Alpine / musl Linux — npm path works fine on Alpine with Node.js installed
