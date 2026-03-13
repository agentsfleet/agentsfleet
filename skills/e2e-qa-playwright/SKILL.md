---
name: e2e-qa-playwright
description: Set up and run Playwright CLI-based end-to-end QA with deterministic selectors and CI-friendly output.
---

# E2E QA Playwright

CLI-first browser QA baseline for this organization.

## Policy

- Use Playwright CLI as default.
- Do not use mabl as baseline.
- Do not use Stagehand as baseline.

## Setup

```bash
bun add -d @playwright/test
bunx playwright install --with-deps
```

## Required Targets

- `make qa`        — full Playwright e2e suite, headless
- `make qa-smoke`  — smoke tests only (fast CI gate)

No `make qa-headed`. Agents run headless. Local headed runs use `bunx playwright test --headed` directly — not a shared Make target.

## Core Commands

```bash
bunx playwright test --reporter=line
bunx playwright test tests/e2e/smoke.spec.ts
```

## CI Rules

- Run headless by default.
- Fail pipeline on test failure.
- Store screenshots/videos/traces as artifacts.
- Use `chromium` only in CI (no cross-browser matrix unless explicitly required).
- `BASE_URL` env var selects target: unset → dev server auto-start; set → test against that URL (post-deploy smoke).

## Output Contract

Return:

1. Test files added/updated.
2. Exact command outputs.
3. Flake analysis and stabilization changes.
