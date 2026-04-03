# M23_002: Homebrew Tap

**Prototype:** v1.0.0
**Milestone:** M23
**Workstream:** 002
**Date:** Apr 03, 2026
**Status:** PENDING
**Priority:** P0 — Required by quickstart.mdx; macOS users expect `brew install`
**Batch:** B2 — Starts after M23_001 (needs a published npm/GitHub Release artifact to hash)
**Branch:** feat/m23-cli-distribution
**Depends on:** M23_001 (GitHub Release with binary tarballs must exist)

---

## 1.0 Tap Repository

**Status:** PENDING

Create the public GitHub repository `usezombie/homebrew-tap` under the `usezombie` org. Homebrew resolves `brew install usezombie/tap/zombiectl` by cloning `github.com/usezombie/homebrew-tap` and reading `Formula/zombiectl.rb`.

**Dimensions:**
- 1.1 PENDING Repo `usezombie/homebrew-tap` exists, is public, and has a `Formula/` directory
- 1.2 PENDING `Formula/zombiectl.rb` contains a valid Homebrew formula for the current release
- 1.3 PENDING Formula passes `brew audit --new-formula zombiectl` with no errors
- 1.4 PENDING `brew install usezombie/tap/zombiectl` succeeds on macOS aarch64

---

## 2.0 Formula Design

**Status:** PENDING

The formula fetches the macOS aarch64 binary tarball from the GitHub Release (not npm). It uses `bin.install` to place the `zombied-darwin-arm64` binary as `zombied`, and a wrapper shim for `zombiectl` that delegates to `zombied`. Version, URL, and SHA256 are the only fields that change per release.

**Dimensions:**
- 2.1 PENDING Formula `url` points to `https://github.com/usezombie/usezombie/releases/download/v{ver}/zombied-darwin-arm64.tar.gz`
- 2.2 PENDING Formula `sha256` matches the tarball SHA256 from the release
- 2.3 PENDING `brew test zombiectl` passes: `zombiectl --version` exits 0
- 2.4 PENDING Formula only installs `zombied` — no dynamic linker dependencies (binary is static)

---

## 3.0 Release Automation

**Status:** PENDING

After each GitHub Release, a CI job (or the existing `release.yml`) must auto-update the tap formula: bump version, recalculate SHA256 from the new tarball, commit, and push to `usezombie/homebrew-tap`. This must be agent-executable with no human steps after the release tag is pushed.

**Dimensions:**
- 3.1 PENDING `release.yml` has a `homebrew-tap` job that runs after `create-release`
- 3.2 PENDING Job fetches the new tarball, computes SHA256, and patches `Formula/zombiectl.rb`
- 3.3 PENDING Job commits and pushes to `usezombie/homebrew-tap` using a GitHub App token from vault
- 3.4 PENDING Tap update commit message follows pattern: `chore: bump zombiectl to v{ver}`

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 `brew install usezombie/tap/zombiectl` succeeds on macOS aarch64 (Apple Silicon)
- [ ] 4.2 `zombiectl --version` prints correct version after brew install
- [ ] 4.3 `brew upgrade zombiectl` picks up the new formula after a release tag is pushed
- [ ] 4.4 Tap repo `usezombie/homebrew-tap` is public and browsable on GitHub

---

## 5.0 Out of Scope

- Linux Homebrew (Linuxbrew) support — defer until aarch64-linux binary is validated
- Intel (x86_64) macOS formula — `macos-latest` runner is Apple Silicon; Intel bottles deferred
- Submitting to `homebrew-core` — tap approach is sufficient for launch
