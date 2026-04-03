# M23_003: Shell Installers (cURL + PowerShell)

**Prototype:** v1.0.0
**Milestone:** M23
**Workstream:** 003
**Date:** Apr 03, 2026
**Status:** PENDING
**Priority:** P1 — Required by quickstart.mdx; provides one-liner install for users without Node.js
**Batch:** B2 — Starts after M23_001 (npm release must exist to install from)
**Branch:** feat/m23-cli-distribution
**Depends on:** M23_001 (`@usezombie/zombiectl` published to npm; no binary tarballs needed — CLI is pure JS)

**Reference implementation:** `~/Projects/oss/resend-cli/install.sh` (302 lines) and `install.ps1` (147 lines)

---

## Design Note: zombiectl is Pure JS

Unlike resend-cli, `zombiectl` has no native binary. The CLI is a pure Node.js package. Both `install.sh` and `install.ps1` therefore install Node.js (if absent) and then run `npm install -g @usezombie/zombiectl` — they do **not** download or extract platform-specific tarballs. Version pinning, architecture detection, and PATH patching still apply; binary hash verification does not.

---

## 1.0 cURL Installer (macOS / Linux)

**Status:** PENDING

A POSIX shell script at `https://usezombie.sh/install.sh`. Checks for Node.js ≥18, installs it via the system package manager if missing, then runs `npm install -g @usezombie/zombiectl`. Detects OS and architecture for the Node.js install path only. Patches PATH for zsh, bash, and fish if the npm global bin dir is not already on PATH.

Modelled on resend-cli `install.sh`: OS/arch detection via `uname -ms`, Rosetta 2 detection, shell config patching, version-pin support via `ZOMBIECTL_VERSION` env var, musl/Alpine rejection with a helpful message.

**Dimensions:**
- 1.1 PENDING Script detects platform: `darwin/arm64`, `linux/x86_64`, `linux/aarch64`; rejects musl (Alpine) with clear error
- 1.2 PENDING Installs Node.js ≥18 if absent; skips install if already present and version is sufficient
- 1.3 PENDING `ZOMBIECTL_VERSION` env var pins the installed version (e.g. `ZOMBIECTL_VERSION=0.3.0 curl … | bash`); defaults to `latest`
- 1.4 PENDING Patches PATH in `~/.zshrc`, `~/.bashrc`, or `~/.config/fish/config.fish` for the npm global bin dir; prints a manual instruction if shell is unrecognised

---

## 2.0 PowerShell Installer (Windows)

**Status:** PENDING

A PowerShell script at `https://usezombie.com/install.ps1`. Checks for Node.js ≥18 via `winget` or direct MSI download if absent, then runs `npm install -g @usezombie/zombiectl`. Adds npm global bin to user PATH via `[Environment]::SetEnvironmentVariable`. Enforces TLS 1.2 for PowerShell 5.1 compatibility. Disables progress bar for fast downloads.

Modelled on resend-cli `install.ps1`: `$env:ZOMBIECTL_VERSION` pin support, `Get-Command node` check, user-scoped PATH update (`[EnvironmentVariableTarget]::User`), no admin required.

**Dimensions:**
- 2.1 PENDING Script checks for Node.js ≥18; installs via `winget install OpenJS.NodeJS.LTS` if absent
- 2.2 PENDING `$env:ZOMBIECTL_VERSION` pins version; defaults to `latest`
- 2.3 PENDING PATH update is user-scoped (`[EnvironmentVariableTarget]::User`), not machine-scoped; no admin required
- 2.4 PENDING Both `pwsh` (PowerShell 7) and `powershell` (5.1) execution paths are tested in CI

---

## 3.0 Hosting

**Status:** PENDING

`install.sh` served from `https://usezombie.sh/install.sh`. `install.ps1` served from `https://usezombie.com/install.ps1`. Both are static files in version control (website or Cloudflare Pages). The `usezombie.sh` domain must resolve with TLS before this workstream can be accepted.

**Dimensions:**
- 3.1 PENDING `https://usezombie.sh/install.sh` returns HTTP 200 with `Content-Type: text/plain` or `text/x-shellscript`
- 3.2 PENDING `https://usezombie.com/install.ps1` returns HTTP 200 with appropriate content-type
- 3.3 PENDING Both URLs added to `smoke-post-deploy.yml` health check list
- 3.4 PENDING Scripts contain no inline secrets, hardcoded versions, or non-HTTPS download sources

---

## 4.0 PR-Scoped CI Gates

**Status:** PENDING

Following the resend-cli pattern, dedicated lightweight workflows trigger only when installer files change — keeping PRs that touch other code fast. These are separate from the post-release full matrix (M23_004).

**Dimensions:**
- 4.1 PENDING `test-install-unix.yml` triggers on `pull_request: paths: [install.sh]`; runs on `ubuntu-latest` and `macos-latest`
- 4.2 PENDING `test-install-windows.yml` triggers on `pull_request: paths: [install.ps1]`; runs on `windows-latest` with both `pwsh` and `powershell`
- 4.3 PENDING Each test job: run installer → assert `zombiectl --version` exits 0 → assert PATH contains npm global bin
- 4.4 PENDING Error path test: `ZOMBIECTL_VERSION=99.99.99` must fail with a non-zero exit code and a clear error message

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 `curl -fsSL https://usezombie.sh/install.sh | bash` installs `zombiectl` on Ubuntu amd64 (CI)
- [ ] 5.2 `curl -fsSL https://usezombie.sh/install.sh | bash` installs `zombiectl` on macOS aarch64 (CI)
- [ ] 5.3 `irm https://usezombie.com/install.ps1 | iex` installs `zombiectl` on `windows-latest` (CI), both `pwsh` and `powershell`
- [ ] 5.4 `zombiectl --version` exits 0 and prints a valid semver string after each installer path
- [ ] 5.5 Bogus version install (`ZOMBIECTL_VERSION=99.99.99`) fails with exit code 1 and a clear error

---

## 6.0 Out of Scope

- Platform-specific binary download or SHA256 verification — not needed; CLI is pure JS via npm
- Alpine/musl support — rejected by installer with a message pointing to npm install
- Chocolatey, Scoop, winget package submission — post-launch
- macOS Gatekeeper / Windows SmartScreen handling — not applicable for npm-installed JS CLIs
- Automatic PATH reload in the current shell session — users open a new terminal
