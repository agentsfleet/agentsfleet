# M23_003: Shell Installers (cURL + PowerShell)

**Prototype:** v1.0.0
**Milestone:** M23
**Workstream:** 003
**Date:** Apr 03, 2026
**Status:** PENDING
**Priority:** P1 — Required by quickstart.mdx and install.mdx; cURL path explicitly promises "no Node.js required"
**Batch:** B2 — Starts after M23_001 (npm release exists); binary bundling runs in parallel with npm
**Branch:** feat/m23-cli-distribution
**Depends on:** M23_001 (GitHub Release must exist with platform binaries attached)

**Reference implementation:** `~/Projects/oss/resend-cli/install.sh` (302 lines), `install.ps1` (147 lines)

---

## Design: Two Distinct Install Paths

The docs (`cli/install.mdx`) describe two separate user paths:

| Method | User experience | Node.js required? | Source |
|---|---|---|---|
| cURL | Downloads a prebuilt binary from GitHub releases | **No** | GitHub Release assets |
| npm | `npm install -g @usezombie/zombiectl` | Yes (≥18) | npm registry |

These are not interchangeable. The cURL path must work on a machine with zero Node.js. This requires `zombiectl` to be bundled into a standalone binary using `bun build --compile` (Bun's native binary bundler — no `pkg` dependency needed since the project already uses Bun). Output: a single self-contained executable per platform with the Bun runtime embedded.

---

## 1.0 Standalone Binary Build (Bun compile)

**Status:** PENDING

Use `bun build --compile` to produce platform-native single-file executables. These are attached to the GitHub Release alongside the Zig daemon binaries. The cURL installer downloads the correct one based on detected OS and architecture.

Build targets:
| Platform | Output filename | Runner |
|---|---|---|
| macOS aarch64 | `zombiectl-darwin-arm64` | `macos-latest` |
| Linux x86_64 | `zombiectl-linux-x64` | `ubuntu-latest` |
| Linux aarch64 | `zombiectl-linux-arm64` | `ubuntu-24.04-arm` |
| Windows x64 | `zombiectl-windows-x64.exe` | `windows-latest` |

**Dimensions:**
- 1.1 PENDING `release.yml` has a `cli-binaries` job that runs `bun build --compile --target=<platform> --outfile=dist/zombiectl-<target> src/cli.js` for each platform
- 1.2 PENDING Each binary is verified post-build: `./zombiectl-<target> --version` exits 0 on the target runner
- 1.3 PENDING All four platform binaries are attached to the GitHub Release as assets (alongside the Zig daemon tarballs)
- 1.4 PENDING `bun build --compile` output is a single file with no external runtime dependency

---

## 2.0 cURL Installer (macOS / Linux)

**Status:** PENDING

`install.sh` is hosted at `https://usezombie.sh/install.sh`. It detects OS and architecture, resolves the latest release tag from the GitHub API, downloads the correct `zombiectl-<target>` binary from GitHub releases, verifies SHA256, installs to `~/.zombiectl/bin`, strips macOS quarantine, and patches PATH.

Modelled directly on `resend-cli/install.sh`: wrapped in `main()` to guard against partial downloads, `GITHUB_BASE` override for enterprise/mirror use, `ZOMBIECTL_INSTALL` env override for install dir, Rosetta 2 detection, musl/Alpine rejection, zsh/bash/fish PATH patching.

**Dimensions:**
- 2.1 PENDING Script detects `darwin/arm64`, `linux/x86_64`, `linux/aarch64`; Rosetta 2 → prefers arm64; musl → error with npm fallback hint
- 2.2 PENDING Version pin: `curl … | bash -s v0.3.0` installs exact version; default fetches `/releases/latest/download/`
- 2.3 PENDING SHA256 verification against a `.sha256` sidecar file published alongside each release binary
- 2.4 PENDING `xattr -d com.apple.quarantine` applied on macOS; `chmod +x` applied on Linux; `--version` asserted before PATH patching

---

## 3.0 PowerShell Installer (Windows)

**Status:** PENDING

`install.ps1` is hosted at `https://usezombie.com/install.ps1`. Downloads `zombiectl-windows-x64.exe` from the GitHub Release, verifies SHA256 via `Get-FileHash`, installs to `$HOME\.zombiectl\bin`, adds to user PATH. No Node.js, no admin required.

Modelled on `resend-cli/install.ps1`: TLS 1.2 enforcement for PowerShell 5.1, progress bar disabled for fast downloads, `$env:ZOMBIECTL_VERSION` pin, user-scoped PATH update.

**Dimensions:**
- 3.1 PENDING Downloads `zombiectl-windows-x64.exe` from GitHub Release (latest or pinned via `$env:ZOMBIECTL_VERSION`)
- 3.2 PENDING SHA256 verified via `Get-FileHash` against `.sha256` sidecar; aborts on mismatch
- 3.3 PENDING PATH updated via `[Environment]::SetEnvironmentVariable(..., [EnvironmentVariableTarget]::User)` — no admin required
- 3.4 PENDING Both `pwsh` (7) and `powershell` (5.1) execution paths tested in CI; TLS 1.2 enforced for 5.1

---

## 4.0 Cloudflare Hosting for usezombie.sh

**Status:** PENDING

`install.sh` must be served from `https://usezombie.sh/install.sh`. The `usezombie.sh` domain is already registered and configured in Cloudflare. The fix is a Cloudflare Pages deployment that serves `install.sh` as a static file, or a Cloudflare Worker that redirects to the raw GitHub URL.

**Recommended approach:** Cloudflare Pages redirect rule — `usezombie.sh/install.sh` → `https://raw.githubusercontent.com/usezombie/usezombie/main/install.sh` (301 permanent). This avoids a separate Pages deployment and keeps the canonical file in the main repo. `install.ps1` is served similarly from `usezombie.com/install.ps1` via Cloudflare Pages redirect.

**Human steps (one-time):**
1. In Cloudflare dashboard → `usezombie.sh` → Rules → Redirect Rules: add rule `URI path equals /install.sh` → Static redirect → `https://raw.githubusercontent.com/usezombie/usezombie/main/install.sh` (301)
2. In Cloudflare dashboard → `usezombie.com` → Rules → Redirect Rules: add rule `URI path equals /install.ps1` → Static redirect → `https://raw.githubusercontent.com/usezombie/usezombie/main/install.ps1` (301)

**Dimensions:**
- 4.1 PENDING `https://usezombie.sh/install.sh` returns HTTP 200 (after redirect) with `Content-Type: text/plain`
- 4.2 PENDING `https://usezombie.com/install.ps1` returns HTTP 200 (after redirect) with appropriate content-type
- 4.3 PENDING Both URLs are added to `smoke-post-deploy.yml` health check list
- 4.4 PENDING Cloudflare redirect rules are documented in `playbooks/M23_003_SHELL_INSTALLERS.md` for future operators

---

## 5.0 PR-Scoped CI Gates

**Status:** PENDING

Dedicated lightweight workflows that trigger only when installer files change, keeping unrelated PRs fast. Modelled on resend-cli's `test-install-unix.yml` and `test-install-windows.yml`.

**Dimensions:**
- 5.1 PENDING `test-install-unix.yml` triggers on `pull_request: paths: [install.sh]`; runs on `ubuntu-latest` and `macos-latest`
- 5.2 PENDING `test-install-windows.yml` triggers on `pull_request: paths: [install.ps1]`; runs on `windows-latest` with both `pwsh` and `powershell`
- 5.3 PENDING Each test: run installer → `zombiectl --version` exits 0 → PATH contains install dir
- 5.4 PENDING Error path: `ZOMBIECTL_VERSION=99.99.99` / `$env:ZOMBIECTL_VERSION="99.99.99"` must exit non-zero with a clear error message

---

## 6.0 Acceptance Criteria

**Status:** PENDING

- [ ] 6.1 `curl https://usezombie.sh/install.sh | bash` installs `zombiectl` on macOS aarch64 with no Node.js present (CI)
- [ ] 6.2 `curl https://usezombie.sh/install.sh | bash` installs `zombiectl` on Ubuntu amd64 with no Node.js present (CI)
- [ ] 6.3 `irm https://usezombie.com/install.ps1 | iex` installs `zombiectl` on `windows-latest` with no Node.js, both `pwsh` and `powershell`
- [ ] 6.4 `zombiectl --version` exits 0 and prints a valid semver string after each path
- [ ] 6.5 `https://usezombie.sh/install.sh` resolves and returns 200 (Cloudflare redirect active)
- [ ] 6.6 Bogus version install fails with exit code 1 and a clear error

---

## 7.0 Out of Scope

- Homebrew — deferred post-launch
- Chocolatey, Scoop, winget submission — post-launch
- Alpine/musl support for cURL path — rejected with npm fallback hint
- macOS Intel (x86_64) binary — `macos-latest` is Apple Silicon; Intel deferred
- Automatic PATH reload in the current shell — users open a new terminal or `source` their config
