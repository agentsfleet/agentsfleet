# Codex PR-readiness review prompt

Paste the block below into Codex. It reviews the M227 change set read-only and ends with one verdict line.

```text
Review whether branch `feat/website-completion` of agentsfleet is ready for a Pull Request (PR).
Read-only: do not edit, stage, commit, push or run destructive git. Report findings; the owner's agent fixes them.

Where: worktree `/Users/kishore/.codex/worktrees/fd5a/agentsfleet`. The change set is the working tree
against HEAD: `git diff HEAD` plus untracked files from `git status -uall`. Part of it is staged.
The spec's comparison revision is `ad3ed8f92`.

Read first, in order:
1. `docs/v2/active/M227_001_P0_API_CLI_DOCS_UI_WORKSPACE_CHAT_CUTOVER.md`: Sections 1-7, Dimensions,
   Failure Modes, Invariants, Files Changed, and Discovery. Discovery holds owner decisions; do not reopen them.
2. `docs/reviews/workspace-setup/decisions.md`: 21 locked decisions.
3. `docs/architecture/api_resources.md`, `docs/architecture/data_flow.md#fleet-creation-and-setup-recovery`,
   `docs/architecture/runner_fleet.md#authoring-leases`.
4. `docs/reviews/workspace-setup/checks.md`, section "Oct 15, 2026: Dimension coverage map".
5. Repository rules: `.orly/LOCAL.md`, `.orly/AGENTS.md`, `.orly/docs/REST_API_DESIGN_GUIDELINES.md`,
   `docs/RUST_ERROR_STANDARD.md`, `.orly/dispatch/write_rust.md`, and `.orly/docs/DOCUMENTATION_RULES.md`
   for OpenAPI prose. For Rust, cite Microsoft Pragmatic Rust Guidelines IDs (`M-...`) from
   `~/Projects/oss/rust-guidelines/all.txt`; read it by section, never whole.

Review for:
A. Spec correctness. Each Dimension's behavior is implemented AND called in production, not only in tests.
   Flag spec text the code contradicts. Known example: Dimension 2.3 says readiness stays out of the
   review digest, but `rustd/crates/afd_api/tests/authoring/readiness/svix.rs:37` asserts a credential
   rotation changes `review_digest`.
B. Security and authority. Private draft text never reaches logs, events, other members, caches or
   telemetry. Authoring content cannot grant authority. The grant barrier holds before activation.
   Every model dispatch re-checks its capability. No credential value enters a digest or a response.
C. Concurrency and money. Database-time expiry; one host capacity shared by claims and both renewal
   kinds; fences; wallet locks; checked integer nanos arithmetic; exactly-once admission and settlement.
D. Schema. Migrations `schema/935` to `schema/950` and their registration in
   `rustd/crates/afd_db/src/migration.rs`. `950_drop_fleet_bundle_storage.sql` is an owner-approved
   destructive drop: prove no live reader or writer names the dropped tables or columns with
   `git grep -w -e source_markdown -e trigger_markdown -e bundle_content_hash -e bundle_snapshot_key
   -e fleet_library -e tenant_fleet_library`, ignoring frozen migrations and specs under `done/`.
   Check role grants and that no SQL carries a static string default or CHECK list.
E. API surface. `public/openapi.json` matches the served routes: 17 new person routes, 4 runner
   authoring routes, 4 removed. Check statuses, problem bodies, ETag and If-Match, idempotency keys,
   and garde-declared bounds against the REST guide. Check that the typed `FleetDefinition` and
   `Definition*` schemas describe exactly what `Definition::parse` accepts, and that their descriptions
   follow the documentation rules.
F. Removed legacy. No `SKILL.md` or `TRIGGER.md` parsing, bundle import or export, compatibility alias,
   or fallback path survives in live code, tests, scripts, docs, CLI help or playbooks. Run the
   discovery greps from the spec's "Files Changed" and "Dead Code Sweep" sections and classify every hit.
G. Rust quality. Error standard (`error_shell!`/`error_lifts!`, no hand-written error types);
   parse-don't-validate (for example `Reach::Write { base }` in
   `rustd/crates/afd_fleet_runtime/src/config/repository.rs`); no production `unwrap`/`expect` without
   a stated invariant; files at most 350 lines, functions at most 50, methods at most 70
   (`ServingPlane::new` in `rustd/crates/agentsfleetd/src/plane` is known to be over); module layout
   (`x/mod.rs` beside its children; crate roots deny `clippy::self_named_module_files`).
H. CLI and playbook. `agentsfleet chat --definition` (`cli/src/commands/chat/`,
   `cli/test/chat/definition.test.ts`) and step 3.3 of
   `playbooks/operations/slack_incident_drill/001_playbook.md`: do the documented commands, flags and
   printed lines match the CLI source exactly?
I. Tests. Does each test assert what its name claims, and would it fail if the behavior regressed?
   Compare with the coverage map. List the missing negative test for every Failure Modes row.
J. PR gates. What still blocks `orly gate pr`: `make harness-verify`, `make lint-all`,
   `make test-unit-all` (known: `make test-unit-cli` has 22 failing tests that predate the latest
   change), `make check-version`, spec completeness (no Dimension is DONE yet), the pending Test
   Baseline, the changelog entry, and the `~/Projects/docs` pages.

You may run read-only commands and `make harness-verify`, `make lint-all`, `make test-unit-all` and
`make check-version`. Do NOT run `make test-integration-rustd`; the owner runs it once, just before the push.

Output:
1. Findings ranked P0 (blocks the PR) to P3. Each gives `file:line`, what is wrong, a concrete failing
   scenario, and the fix.
2. A table: Dimension | implemented in production (yes, partial or no, with evidence) |
   tests adequate (yes or no) | what is missing.
3. Every command you ran, with its decisive output line.
4. The last line is exactly one of: `READY FOR PR` or `NOT READY: <the P0 and P1 items>`.
```
