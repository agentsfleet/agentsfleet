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

## Design Note: Two Distinct Binary Types

M23 involves two independent sets of binaries with different purposes:

| Binary | Built by | Used by | Platform build needed |
|---|---|---|---|
| `zombiectl-<platform>` | `bun build --compile` (M23_003) | cURL/PowerShell install path | Yes — 4 platforms |
| `zombied`, `zombied-executor` | Zig (existing `release.yml`) | Server daemon | Already built |
| `@usezombie/zombiectl` npm package | `bun run build` (M23_001) | npm install path | No — pure JS |

The CI matrix in this workstream must verify all three distribution channels. The `bun build --compile` step from M23_003 adds a new parallel build job to `release.yml`; this workstream validates its output across platforms.

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

## 2b.0 cURL Binary Verification Matrix

**Status:** PENDING

Verify the `bun build --compile` binaries (from M23_003 §1.0) run correctly on each target platform. This mirrors resend-cli's `test-binary` matrix in `release.yml` — build on the target runner, assert `--version`/`--help` pass before upload.

Matrix:
| Runner | Binary | Verification |
|---|---|---|
| `macos-latest` | `zombiectl-darwin-arm64` | `./zombiectl-darwin-arm64 --version` |
| `ubuntu-latest` | `zombiectl-linux-x64` | `./zombiectl-linux-x64 --version` |
| `ubuntu-24.04-arm` | `zombiectl-linux-arm64` | `./zombiectl-linux-arm64 --version` |
| `windows-latest` | `zombiectl-windows-x64.exe` | `.\zombiectl-windows-x64.exe --version` |

**Dimensions:**
- 2b.1 PENDING Each binary is built and verified in the same `release.yml` job step before artifact upload
- 2b.2 PENDING `--version` and `--help` both exit 0 on all four runners
- 2b.3 PENDING macOS binary passes `xattr -d com.apple.quarantine` + `--version` in the build job
- 2b.4 PENDING SHA256 sidecar (`.sha256` file) generated and uploaded alongside each binary

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

- [ ] 6.1 `post-release.yml` is green across all methods (npm × 4 platforms, cURL binary × 2, PowerShell × 2 shells) after a release tag
- [ ] 6.2 `install-check.yml` gates PRs touching `zombiectl/` and installer scripts
- [ ] 6.3 All four `zombiectl-<platform>` binaries appear in the GitHub Release assets with `.sha256` sidecars
- [ ] 6.4 GitHub Actions job summary shows a clear pass/fail table per platform and install method

---

## 7.0 Supply Chain Integrity

**Status:** PENDING

Supply chain attacks on CLIs are an active threat category. npm provenance (Sigstore) is already required by M23_001. This section adds keyless binary signing using `cosign` with GitHub OIDC — free, no hardware token, no paid certificate.

**Dimensions:**
- 7.1 PENDING `release.yml` `cli-binaries` job runs `cosign sign-blob` on each platform binary using GitHub OIDC (`id-token: write` permission already set); `.sig` and `.pem` certificate files uploaded as release assets alongside each binary
- 7.2 PENDING `install.sh` documents the optional verify step: `cosign verify-blob zombiectl-<target> --signature zombiectl-<target>.sig --certificate zombiectl-<target>.pem --certificate-identity-regexp usezombie --certificate-oidc-issuer https://token.actions.githubusercontent.com`
- 7.3 PENDING `post-release.yml` asserts `.sig` sidecars are present in the release assets for all four platform binaries
- 7.4 PENDING `install.sh` and `install.ps1` enforce HTTPS-only download sources; `GITHUB_BASE` override validated as `https://` prefix before any fetch

---

## 8.0 Out of Scope

- Windows Zig binary (`zombied-windows-amd64`) — server daemon, not CLI distribution
- macOS notarization — not doing this
- Windows Authenticode / Microsoft Trusted Signing — post-launch if SmartScreen friction becomes user-reported issue
- Linux aarch64 cURL installer test — npm path covers it; deprioritised
- macOS Intel (x86_64) — `macos-latest` is Apple Silicon; Intel is end-of-life
- Alpine / musl Linux — rejected by `install.sh` with message pointing to npm
