# Stale-file audit, Nov 18, 2026

This audit lists tracked files with no commit since Jul 18, 2026, four months before the audit date.
Of 8,049 tracked files, 339 qualify, and 228 of those are finished specs in `docs/v2/done/`.
Dates come from `git log --name-only` over 9,664 commits, and a moved file counts as touched at the move.

## Removed in this change

| File | Why it goes |
|---|---|
| `.amp/instructions.md` | A five-line pointer to `../AGENTS.md`, which is not at the repository root. Orly does not manage it. |
| `public/heartbeat` | Nothing reads it. The daemon serves `/healthz` and `/readyz`, and the website prebuild copies only `openapi.json`. |
| `DropdownMenuPortal`, `DropdownMenuRadioGroup`, `SelectGroup`, `SelectLabel`, `SelectSeparator` | Design-system exports with no user outside their own file or the Select test. Their two prop-type re-exports go with them. |
| `AUTHORING_LEASE`, `ITEMS_MAX`, `OBJECTIVE_MAX_BYTES`, `TITLE_MAX_BYTES` | Constants in `af_wire` that appear once in the repository, at their definition. |
| `.gitleaksignore` | Its 26 fingerprints name 13 commits that are no longer in history. A full-history scan reports the same 13 findings with and without it. |

The `public/AGENTS.md` row for `heartbeat` leaves with the file.
The `public/llms.txt` health line now names `/healthz`, because the old `/heartbeat` path returned 404.
The `$schema` line added to `ui/agentsfleet.dev/dist/vercel.json` helps editors validate the file and changes no serving behavior.

## Kept

| File | Why it stays |
|---|---|
| `deploy/fly/cloudflared-dev` and `cloudflared-prod` | They run the tunnel for `api-dev.agentsfleet.net`, because the daemon has no public service. Both Fly apps have started machines. |
| `ui/agentsfleet.dev/dist/vercel.json` | It routes the installer site, and lint covers the installer. |
| `LICENSE` | The text and the 2026 year are correct. Indy asked for the file to be touched, so it gains one trailing blank line and nothing else. |

## Dead-code candidates left for follow-up

A name-based scan found these, and a type-aware tool has not confirmed them.
Dynamic use would hide from the scan.

| Area | Finding |
|---|---|
| `ui/packages/app` | `hasPluginWriteScope`, `parseAnsi`, `ansiColor`, `usdPerMtokToNanos` and `getWorkspaceConnectionStatus` have no non-test user. Tests still cover them. |
| `cli` and `ui/packages/website` | `platformEtagFlag`, `MARKETING_LEAD_CAPTURE_URL` and `HERO_SECONDARY_LABEL` are declared and never used, even in their own files. They sit on user-surface paths, so removing them needs a docs change or an owner-recorded override. |
| Needless `export` | 117 values in the app and 14 in the command-line interface (CLI) and website are exported but used only inside their own file. |

## Post-deploy acceptance failure

Run 44351961751 on `650bddef6` failed twice in `workspace-private-chat.spec.ts`.
Attempt 1 stayed on `fleets/new` for 60 seconds after Save.
Attempt 2 saved the fleet, then axe reported a `heading-order` violation on an `h3` at phone width.

The `heading-order` failure most likely came from `FleetSetup.tsx`, which renders its `h2` only when the save is not yet recorded.
`SetupQuestions` always rendered an `h3` after it, so a recorded save left the page at `h1` then `h3`.
Dev gained a decision model in today's rebuild, so drafts now carry those questions.
Unverified: the failing page's snapshot, in that run's saved test files, would confirm it.

This change fixes that path. `SetupQuestions` takes a heading level, and `FleetSetup` passes level 2 when no save heading sits above it.
A regression test pins both states, and `docs/DESIGN_SYSTEM.md` now states the rule.

Attempt 1, the save that never left `fleets/new`, is still unexplained.
The next post-deploy run shows whether it repeats.
