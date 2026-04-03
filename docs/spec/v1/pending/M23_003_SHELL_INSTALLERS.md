# M23_003: Shell Installers (cURL + PowerShell)

**Prototype:** v1.0.0
**Milestone:** M23
**Workstream:** 003
**Date:** Apr 03, 2026
**Status:** PENDING
**Priority:** P1 — Required by quickstart.mdx; provides one-liner install for users without Node or Homebrew
**Batch:** B2 — Starts after M23_001 (depends on GitHub Release artifacts)
**Branch:** feat/m23-cli-distribution
**Depends on:** M23_001 (GitHub Release tarballs must exist at known URLs)

---

## 1.0 cURL Installer (macOS / Linux)

**Status:** PENDING

A POSIX shell script hosted at `https://usezombie.sh/install.sh`. Detects OS and architecture, fetches the correct binary tarball from the latest GitHub Release, verifies SHA256, installs to `~/.local/bin` (or `~/.zombiectl/bin` as fallback), and adds the bin dir to PATH if missing.

**Dimensions:**
- 1.1 PENDING Script detects platform: `darwin/aarch64`, `linux/amd64`, `linux/aarch64`
- 1.2 PENDING Script fetches latest release tag from GitHub API (`/releases/latest`) — no hardcoded version
- 1.3 PENDING SHA256 verification before extracting binary; aborts with clear error on mismatch
- 1.4 PENDING `curl -fsSL https://usezombie.sh/install.sh | bash` succeeds on macOS aarch64 and Ubuntu amd64

---

## 2.0 PowerShell Installer (Windows)

**Status:** PENDING

A PowerShell script hosted at `https://usezombie.com/install.ps1`. Fetches the Windows binary from GitHub Releases (`.exe` or `.zip`), verifies hash, installs to `$env:USERPROFILE\.zombiectl\bin`, and adds to the user's `PATH` via `[Environment]::SetEnvironmentVariable`.

**Dimensions:**
- 2.1 PENDING Script fetches latest release from GitHub API and downloads `zombied-windows-amd64.exe`
- 2.2 PENDING SHA256 verification via `Get-FileHash` before moving binary to install dir
- 2.3 PENDING PATH update is user-scoped (`[EnvironmentVariableTarget]::User`), not machine-scoped
- 2.4 PENDING `irm https://usezombie.com/install.ps1 | iex` is documented; tested in CI via Windows runner matrix

---

## 3.0 Hosting

**Status:** PENDING

`install.sh` is served from `usezombie.sh`. `install.ps1` is served from `usezombie.com/install.ps1`. Both are static files — committed to the website repo or served via Cloudflare Pages. The `usezombie.sh` domain must resolve and serve with TLS.

**Dimensions:**
- 3.1 PENDING `https://usezombie.sh/install.sh` returns 200 with `Content-Type: text/x-shellscript` or `text/plain`
- 3.2 PENDING `https://usezombie.com/install.ps1` returns 200 with appropriate content-type
- 3.3 PENDING Both URLs are added to the smoke-post-deploy check list
- 3.4 PENDING Scripts are pinned in version control; no inline secrets or hardcoded tokens

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 `curl -fsSL https://usezombie.sh/install.sh | bash` installs `zombiectl` on Ubuntu amd64 (CI)
- [ ] 4.2 `curl -fsSL https://usezombie.sh/install.sh | bash` installs `zombiectl` on macOS aarch64 (CI)
- [ ] 4.3 `irm https://usezombie.com/install.ps1 | iex` installs `zombiectl` on Windows Server 2022 (CI)
- [ ] 4.4 `zombiectl --version` exits 0 after each installer path

---

## 5.0 Out of Scope

- Windows binary build (x86_64-windows) — required by this workstream; tracked in M23_004 cross-platform CI
- Package managers beyond brew/npm/curl/ps1 (Chocolatey, Scoop, winget) — post-launch
- Automatic PATH reload in the current shell session — users are instructed to open a new terminal
