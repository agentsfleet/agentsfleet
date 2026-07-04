## Handoff — PR #477 pushed and mergeable; Indy's product-verification punch list still open

### Scope/Status

M112_001 and M113_002 are both `Status: DONE`, moved to `docs/v2/done/`. The full CHORE(close) skill chain ran (`/write-unit-test`, adversarial `/review`, `kishore-babysit-prs`) and PR #477 is pushed, description updated with session notes, mergeable. **This is not the end of the work** — Indy came back with a punch list of product-verification questions after the push that this handoff exists to carry forward.

- ✅ M112_001 (Fleet library rename) — DONE, `docs/v2/done/`.
- ✅ M113_002 (error-message friendliness) — DONE, `docs/v2/done/`. `make test-integration` re-verified clean against a fully fresh Postgres.
- ✅ M113_001, M113_003 — already DONE from earlier in the session.
- ✅ Docs-repo PR: [agentsfleet/docs#124](https://github.com/agentsfleet/docs/pull/124) — pushed, greptile findings triaged and replied (3 fixed, 1 false-positive with re-rank requested).
- 🔶 **Indy's punch list below — not started or partially started.**

### Working Tree

Both repos fully pushed, clean working trees:
```
agentsfleet:  ## feat/m108-connector-platform...origin/feat/m108-connector-platform  (nothing ahead/behind)
docs:         ## chore/m112-fleet-library-rename-docs...origin/chore/m112-fleet-library-rename-docs  (nothing ahead/behind)
```

### Branch/PR

- `agentsfleet/agentsfleet` — branch `feat/m108-connector-platform`, PR [#477](https://github.com/agentsfleet/agentsfleet/pull/477), OPEN, not draft, MERGEABLE. Latest pushed commit `421556fd`.
- `agentsfleet/docs` — branch `chore/m112-fleet-library-rename-docs`, PR [#124](https://github.com/agentsfleet/docs/pull/124), OPEN, not draft, MERGEABLE. Latest pushed commit `66174af`.

### Running Processes

- A background Explore agent (id `a2b5abc44a104366b`, output at `/private/tmp/claude-501/-Users-kishore-Projects-agentsfleet/0f93997a-e565-4478-9b1a-7aad61b6ca51/tasks/a2b5abc44a104366b.output`) was mid-run on punch-list item 2 (the grep sweep) when this session was interrupted. **Check whether it's still running / already produced a result before re-launching the same investigation** — do not duplicate it.
- No dev servers, no docker containers of note — Indy tore down the local docker Postgres, `api-dev.agentsfleet.net`, and `api.agentsfleet.net` databases this session (see Risks below); `agentsfleet-postgres`/`agentsfleet-redis` docker containers may still be up locally from the last `make test-integration` run — check `docker ps` before assuming state.

### Tests/Checks

All green as of `421556fd` / docs `66174af`:
- ✅ `make test-unit-app` — 1191/1191
- ✅ `make test-unit-cli` — 1261/1261, 100% coverage
- ✅ `zig build test` — same 2 pre-existing unrelated failures (webhook-sig `UZ-WH-010`, worker-pool `.worker_started`/`.server_started`)
- ✅ `make test-integration` — re-run against a fully fresh, torn-down Postgres; verified directly via `docker exec ... psql` that all 26 migrations applied and `core.fleets` exists; **zero failures this run** (even the 2 usually-flaky ones didn't trigger)
- ✅ `make lint-zig`, `make check-openapi`, `npx mintlify validate`/`broken-links` (docs repo) all clean
- ❌ **`make memleak` was NOT run this session** — punch-list item, see below.

### Next Steps — Indy's punch list (verbatim intent, Jul 05 2026)

1. **Confirm `docs/architecture/*.md` + docs-repo are fully updated for the milestone, not just the changelog.** Done this session: `docs/architecture/fleet_bundles.md` (fixed a stale `platform_template_id`/`tenant_template_id` install invariant) and `docs/architecture/billing_and_provider_keys.md` (fixed `credential_ref`→`secret_ref`, `--credential`→`--secret`). **Not yet checked**: whether any OTHER architecture doc references M113_001 (Models page unification) or M113_003 (Secrets & ENVs split) content that's now stale — only the two files above were audited, not a full `docs/architecture/` sweep.
2. **Final grep sweep** for leftover `template`/`templates`/`credential`/`credentials`/`Fleet Library` (capital L) across CLI source, UI source, `docs/architecture/*.md`, and `public/openapi/**`/`public/openapi.json` (NOT `docs/v2/done/*.md` — historical specs, intentionally untouched; NOT test files). **A background agent was already running this exact check when the session ended — check its output first.**
3. **Investigate**: on the dashboard Models page, when a user adds their own provider key, is the flow the old PlatformHero-card pattern, or the same row/dropdown pattern as the "Other provider" form? (M113_001's spec says the hero card was removed and everything collapses to one row list — verify this actually shipped as described, in the live component tree, not just per the spec's own claim.)
4. **Investigate**: does that add-own-key flow's Provider **and** Model selection come from real dropdowns sourced from the model-caps catalogue (avoiding the exact "model not in cached caps catalogue" bug this session's M113_002 work made friendlier but didn't structurally prevent), or is the model field still free-typed? Check `ui/packages/app/app/(dashboard)/settings/models/` components directly.
5. **Investigate**: how does `core.model_caps` actually get seeded with supported models — a migration-time `INSERT`, or an admin-only API/CLI path (`POST/PATCH/DELETE /v1/admin/models` was mentioned in `api-reference/error-codes.mdx`'s `UZ-PROVIDER-006/007/008` rows)? Check `schema/003_model_caps.sql` and `src/agentsfleetd/http/handlers/admin/` model-catalogue handlers.
6. **Re-verify** nav → Secrets & ENVs is still its own sidebar entry (confirmed once already this session via `ui/packages/app/components/layout/Shell.tsx:70` — `{ label: "Secrets & ENVs", href: "/secrets", icon: KeyRoundIcon }` — a quick re-check is still asked for, do it).
7. **Investigate**: do all dashboard tables (API Keys, Secrets & ENVs, Fleets list, etc.) use the design-system's shared `DataTable` component consistently, or are some hand-rolled? M113_003's own spec mentioned reviving "an already-built-but-unused `DataTable`-based `CredentialsList.tsx`" for Secrets specifically — check whether API Keys and any other list page match that same pattern or diverge.
8. **Run `make memleak`** on this branch's Zig diff (not run this session) — confirm zero leaks per the VERIFY tier requirements, paste the result into PR Session Notes.
9. **Do NOT build** — Indy wants a future sidebar toggle "like tryreplicas." Explicitly not spec'd. Ask him for a reference screenshot before scoping anything; this needs a `kishore-spec-new` pass first if he wants to pursue it, same treatment as the earlier-deferred "integrations/results tracking" idea.

### Risks/Gotchas

- **Migration version-number reuse is a known, deliberately-deferred defect, not silently missed.** `9fecf7d0`'s renumbering (sparse → contiguous 1-26) reuses version integers for different migration content (e.g. old v5 = `model_caps`, new v5 = `core_fleets`). `audit.schema_migrations` tracks applied state by integer only — any database already migrated under the old numbering would silently skip creating `core.fleets` (and everything from v5 on). **Indy's call, quoted in PR #477's Session Notes**: "No fix needed here. I have did a teardown of api-dev, and api.agentsfleet.net db since we are preprod till v2.0. The Docker postgres that was run was killed as well." Nothing is exposed today, but the underlying defect is still in the code — flag again if anyone brings up a persistent staging/prod database that's been migrated more than once.
- **`VERSION` was bumped 0.13.0 → 0.14.0 this session** (Indy-confirmed) — `make sync-version`/`make check-version` clean. Don't re-bump.
- **Do not re-litigate the casing decision** — "Fleet library" (lowercase `l`) is final, applied across ~90 occurrences this session, reversing the codebase's own prior 85/5 majority convention. This was Indy's explicit, repeated call.
- **The `?template=` UI deep-link query param was renamed to `?library=`** this session — a second reversal of what was originally a deliberately-kept exception (both the original M112_001 spec and opencode's `9fecf7d0` commit message called out keeping `?template=`). If you see `?template=` anywhere still, it's stale, not intentional.
- **`TRIGGER.md`'s `credentials:` YAML frontmatter key is NOT renamed** — this is the one structural exception, confirmed unchanged in the actual parser (`event_lifecycle_integration_test.zig`). Don't "fix" it.
- **Docs-repo redirects were deliberately removed**, not just left out — Indy explicitly said "I don't need old legacy urls" / "remove the legacy urls." The changelog's historical `/agents/templates`/`/agents/credentials` links (predating this session) now 404 on purpose; changelog prose is append-only, don't patch it.
- **`kishore-babysit-prs` should be re-invoked** after any new commits land from this punch list — greptile posts asynchronously and per the skill's own cadence table, a fresh push needs its own poll cycle.
