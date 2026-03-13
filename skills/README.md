# Skills Catalog

Each skill lives in `skills/<skill-name>/SKILL.md` (agent-scripts compatible layout).

This folder is split into `core` workflows and `optional` workflows.

All skills must declare inputs, outputs, command sequence, verification, and failure handling (see `AGENTS.md`).

## Workflow Skills (ship, review, plan, retro, browse)

From `skills/ship/`, `skills/review/`, `skills/plan-ceo-review/`, `skills/plan-eng-review/`, `skills/retro/`, `skills/browse/`.

| Skill | Mode | Invoke |
|-------|------|--------|
| `ship` | Release engineer — merge, test, bump, push, PR | `/ship` |
| `review` | Staff engineer — paranoid pre-merge diff review | `/review` |
| `plan-ceo-review` | Founder taste — scope challenge, 10-star product | `/plan-ceo-review` |
| `plan-eng-review` | Eng lead — architecture + code quality lock-in | `/plan-eng-review` |
| `retro` | Eng manager — weekly metrics, hotspots, trends | `/retro` |
| `browse` | QA engineer — headless Chromium, screenshots, forms | `/browse` |

Browse requires a one-time build: `cd skills/browse && bun install && bun run build`

## Core Skills (for immediate install/test)

- `preflight-tooling/`
- `create-scaffold/` — unified scaffold for all stacks (`python`, `rust`, `go`, `typescript`, `javascript-cli`, `tauri`)
- `create-cli/`
- `review-pr/`
- `e2e-qa-playwright/`
- `oracle/`
- `update-docs-and-commit/`

## Optional Skills

- `zoho-sprint/`
- `zoho-desk/`
- `frontend-design/`

## Install/Test Order

1. Install profile from `agents/`.
2. Run `preflight-tooling/SKILL.md` (let the LLM detect and install missing tools).
3. Test scaffold: `/create-scaffold python` on a throwaway repo.
4. Run QA skill (`e2e-qa-playwright/SKILL.md`) in a repo with Playwright.
5. Validate document update flow with `update-docs-and-commit/SKILL.md`.
6. Test PR review with `review-pr/SKILL.md` on an open PR/MR.
