# M23_004: Cross-Platform Install CI

**Prototype:** v1.0.0
**Milestone:** M23
**Workstream:** 004
**Date:** Apr 03, 2026
**Status:** PENDING
**Priority:** P0 — No Windows machine available locally; CI is the only validation path for Windows and Linux aarch64
**Batch:** B3 — Runs after M23_001 (npm), M23_002 (brew), M23_003 (shell installers) are implemented
**Branch:** feat/m23-cli-distribution
**Depends on:** M23_001, M23_002, M23_003

---

## 1.0 Install Matrix CI Workflow

**Status:** PENDING

A new GitHub Actions workflow `install-matrix.yml` triggers on release tags and on PRs touching `zombiectl/` or installer scripts. It runs a matrix across all supported platforms and install methods. No local Windows or Linux aarch64 machine is needed — GitHub-hosted runners cover all targets.

Matrix:
| OS | Runner | Install method |
|---|---|---|
| macOS aarch64 | `macos-latest` | npm, brew, cURL |
| Ubuntu amd64 | `ubuntu-latest` | npm, cURL |
| Ubuntu aarch64 | `ubuntu-24.04-arm` | npm, cURL |
| Windows amd64 | `windows-latest` | npm, PowerShell |

**Dimensions:**
- 1.1 PENDING `install-matrix.yml` exists and triggers on `push: tags: v*` and `pull_request: paths: zombiectl/**`
- 1.2 PENDING Matrix covers all 4 OS/runner combinations listed above
- 1.3 PENDING Each matrix cell installs `zombiectl`, runs `zombiectl --version`, and asserts exit code 0
- 1.4 PENDING Workflow fails fast (`fail-fast: true`) so broken platforms surface immediately

---

## 2.0 Windows Binary Build

**Status:** PENDING

The existing `release.yml` only builds Linux and macOS binaries. Add an `x86_64-windows` build job using the `windows-latest` runner and Zig cross-compilation. Output: `zombied-windows-amd64.exe` attached to the GitHub Release.

**Dimensions:**
- 2.1 PENDING `release.yml` has a `binaries-windows-x86` job using `windows-latest` runner
- 2.2 PENDING Job builds `zig build -Dtarget=x86_64-windows -Doptimize=ReleaseSafe`
- 2.3 PENDING `zombied-windows-amd64.zip` is uploaded as release artifact
- 2.4 PENDING Windows binary is attached to the GitHub Release alongside linux/macos tarballs

---

## 3.0 npm Install Smoke Test

**Status:** PENDING

On each platform, install `@usezombie/zombiectl` from the `next` dist-tag using the system Node.js, then run `zombiectl --version` and `zombiectl doctor --json`. Assert both exit 0 and that `--version` output matches the expected semver string.

**Dimensions:**
- 3.1 PENDING `npm install -g @usezombie/zombiectl@next` succeeds on macOS, Ubuntu amd64, Ubuntu aarch64, Windows
- 3.2 PENDING `zombiectl --version` output matches `\d+\.\d+\.\d+` on all platforms
- 3.3 PENDING `zombiectl doctor --json` exits 0 (no auth key expected; checks CLI version + environment only)
- 3.4 PENDING Test output is captured and attached as a workflow summary artifact

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 `install-matrix.yml` is green across all 4 platform/runner combinations for a release tag
- [ ] 4.2 Windows binary `zombied-windows-amd64.zip` appears in the GitHub Release assets
- [ ] 4.3 PR gate: any change to `zombiectl/package.json` or installer scripts triggers the matrix
- [ ] 4.4 No platform-specific shims or workarounds needed in `zombiectl/src/` (JS is cross-platform)

---

## 5.0 Out of Scope

- Code signing of Windows `.exe` (SmartScreen warning acceptable at launch)
- Windows ARM64 binary — deferred; no runner available
- Linux GUI distro testing (Fedora, Arch) — Ubuntu covers the relevant userbase
- macOS Intel (x86_64) — `macos-latest` is Apple Silicon; Intel is effectively deprecated
