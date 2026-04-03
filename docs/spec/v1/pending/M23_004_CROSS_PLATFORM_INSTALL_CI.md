# M23_004: Cross-Platform Install CI

**Prototype:** v1.0.0
**Milestone:** M23
**Workstream:** 004
**Date:** Apr 03, 2026
**Status:** PENDING
**Priority:** P0 — No Windows or Linux aarch64 machine locally; CI is the only validation path
**Batch:** B3 — Runs after M23_001 (npm) and M23_003 (shell installers) are implemented
**Branch:** feat/m23-cli-distribution
**Depends on:** M23_001, M23_003

**Reference implementation:** `~/Projects/oss/resend-cli/.github/workflows/release.yml` and `post-release.yml`

---

## Design Note: No Native Binary for zombiectl

`zombiectl` is pure JavaScript. There is no per-platform compilation step for the CLI — the same npm package works on all platforms where Node.js runs. The Zig binaries (`zombied`, `zombied-executor`) are server-side only and are not part of the CLI distribution.

This simplifies the matrix significantly compared to resend-cli:
- No `pkg` bundling step
- No platform binary build jobs in `release.yml`
- No binary hash verification
- Windows support is `npm install -g @usezombie/zombiectl` — no `.exe` needed

The Windows Zig binary (`zombied-windows-amd64`) is out of scope for M23 — it is a server daemon, not the CLI.

---

## 1.0 post-release.yml — Installation Verification Workflow

**Status:** PENDING

Following the resend-cli `post-release.yml` pattern: a separate workflow that triggers after `release.yml` completes (via `workflow_run`) and verifies all three install methods against the real published artifacts. This is the authoritative gate — it tests what users actually run, not CI fixtures.

**Dimensions:**
- 1.1 PENDING `post-release.yml` triggers on `workflow_run: workflows: [Release] types: [completed]` and only runs when the release workflow succeeded
- 1.2 PENDING Three verification jobs run in parallel: `verify-npm`, `verify-curl`, `verify-powershell`
- 1.3 PENDING Each job: install `zombiectl` from the published source → `zombiectl --version` → assert output matches `\d+\.\d+\.\d+` and exit code is 0
- 1.4 PENDING Workflow summary step posts a table of platform × method results as a GitHub Actions job summary

---

## 2.0 npm Verification Matrix

**Status:** PENDING

Install `@usezombie/zombiectl@next` from the real npm registry on all four supported platforms. Because the package is pure JS, the same tarball runs everywhere — this validates Node.js compatibility and PATH wiring across OSes.

Matrix:
| Runner | Node version | Install command |
|---|---|---|
| `macos-latest` | system (via `actions/setup-node`) | `npm install -g @usezombie/zombiectl@next` |
| `ubuntu-latest` | system | `npm install -g @usezombie/zombiectl@next` |
| `ubuntu-24.04-arm` | system | `npm install -g @usezombie/zombiectl@next` |
| `windows-latest` | system | `npm install -g @usezombie/zombiectl@next` |

**Dimensions:**
- 2.1 PENDING `npm install -g @usezombie/zombiectl@next` succeeds on all four runners
- 2.2 PENDING `zombiectl --version` output matches the tag version on all four runners
- 2.3 PENDING `zombiectl doctor --json` exits 0 (CLI version check only; no auth required)
- 2.4 PENDING `fail-fast: false` on the matrix so all platform results are visible even if one fails

---

## 3.0 cURL Installer Verification (post-release)

**Status:** PENDING

After `install.sh` is live at `https://usezombie.sh/install.sh`, verify it installs the just-released version on macOS and Ubuntu. Uses the same runner set as resend-cli's `post-release.yml` Unix verification.

**Dimensions:**
- 3.1 PENDING `curl -fsSL https://usezombie.sh/install.sh | bash` succeeds on `macos-latest`
- 3.2 PENDING `curl -fsSL https://usezombie.sh/install.sh | bash` succeeds on `ubuntu-latest`
- 3.3 PENDING `zombiectl --version` after install matches the release tag version (not a stale cached version)
- 3.4 PENDING cURL job runs concurrently with npm and PowerShell jobs in `post-release.yml`

---

## 4.0 PowerShell Installer Verification (post-release)

**Status:** PENDING

After `install.ps1` is live at `https://usezombie.com/install.ps1`, verify it installs on Windows with both `pwsh` (PowerShell 7) and `powershell` (5.1) — matching resend-cli's dual-shell test in `test-install-windows.yml`.

**Dimensions:**
- 4.1 PENDING `irm https://usezombie.com/install.ps1 | iex` succeeds on `windows-latest` using `pwsh`
- 4.2 PENDING Same script succeeds using legacy `powershell` (5.1) on the same runner
- 4.3 PENDING `zombiectl --version` exits 0 after both shell variants
- 4.4 PENDING PATH update is verified: `Get-Command zombiectl` resolves without a new shell session

---

## 5.0 PR Gate: install-check.yml

**Status:** PENDING

A fast PR-scoped workflow (separate from `post-release.yml`) that runs when `zombiectl/` or installer scripts change. Validates that the npm package installs cleanly from the branch's published `next` tag. Keeps the feedback loop tight without waiting for a full release.

**Dimensions:**
- 5.1 PENDING `install-check.yml` triggers on `pull_request: paths: [zombiectl/**, install.sh, install.ps1]`
- 5.2 PENDING Runs `npm install -g @usezombie/zombiectl@next` on `ubuntu-latest` and `windows-latest` only (fast — not full matrix)
- 5.3 PENDING `zombiectl --version` and `zombiectl --help` both exit 0
- 5.4 PENDING Job uses `cancel-in-progress: true` concurrency so stale PR runs are cancelled immediately

---

## 6.0 Acceptance Criteria

**Status:** PENDING

- [ ] 6.1 `post-release.yml` is green across all methods (npm × 4 platforms, cURL × 2, PowerShell × 2 shells) after a release tag
- [ ] 6.2 `install-check.yml` gates PRs touching `zombiectl/` and installer scripts
- [ ] 6.3 No platform-specific code changes required in `zombiectl/src/` — pure JS runs everywhere
- [ ] 6.4 GitHub Actions job summary shows a clear pass/fail table per platform and install method

---

## 7.0 Out of Scope

- Windows Zig binary (`zombied-windows-amd64`) — server daemon, not CLI distribution
- Code signing for npm packages or shell scripts — not required at launch
- Linux aarch64 cURL installer test — Ubuntu ARM runner available but deprioritised; npm path covers it
- macOS Intel (x86_64) — `macos-latest` is Apple Silicon; Intel is end-of-life
- Alpine / musl Linux — rejected by `install.sh` with a message; npm install works if Node.js is present
