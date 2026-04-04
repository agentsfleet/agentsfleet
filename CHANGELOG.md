# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.3.1] - 2026-04-05

### Added

- `@usezombie/zombiectl` scoped npm package — published to npm registry with public access
- Complete OpenAPI 3.1 spec covering all 43 endpoints
- Post-release npm verification CI job: confirms published package installs and runs correctly
- Install-check PR gate: verifies npm install on every PR touching `zombiectl`
- OIDC secrets wired into CI deploy pipelines

### Changed

- Playbooks `M2_001` and `M4_002` marked done — credential gate and prod worker bootstrap verified
- `zombiectl` README rewritten with install instructions and pre-release caveat

### Fixed

- `smoke-post-deploy` workflow trigger restored with correct Production environment condition
- `zombiectl` glob `**` pattern now matches root-level files (replaced `bun Glob` with node-compatible implementation)
- npm publish job switched to bun runtime — resolves install failures in CI
- Website prebuild path corrected from `../../` to `../../../` for monorepo root layout

## [0.3.0] - 2026-04-05

### Added

- `zombiectl` CLI — warning banner + April 5 launch date display
- Release credential gate: all vault items verified before any deploy step runs
- `verify-runtime-compat` CI job: static binary validated against bookworm, trixie, and alpine before publish
- `PROD_WORKER_READY` guard on `deploy-prod-canary` — bare-metal worker fleet deploy gated until bootstrapped
- Fly machine-state verification step in prod deploy pipeline
- `cross-compile.yml` `workflow_call` trigger with `skip_build` input for caller-controlled build skipping
- `playbooks/M4_002_WORKER_BOOTSTRAP_PROD.md` — prod bare-metal worker bootstrap runbook

### Changed

- `docs/ZIG_STATIC_OPENSSL.md` moved to `docs/contributing/ZIG_STATIC_OPENSSL.md` and reformatted as reference blog post

### Fixed

- Fly machine-state check now only accepts `started` state — `stopped` machines are no longer treated as a successful deployment

## [0.1.0] - 2026-03-04

### Added

- Initial release
- `zombied serve` — HTTP API + worker pipeline
- Agent pipeline: Echo → Scout → Warden
- Spec-to-PR delivery with retry loops
- GitHub App OAuth workspace integration
- OpenAPI 3.1 spec at `/openapi.json`
- Machine-readable agent discovery surfaces
