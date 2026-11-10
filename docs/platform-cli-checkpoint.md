# Platform CLI checkpoint

This branch saves unfinished work for the next session. The dashboard does not typecheck yet.

Resume on `feat/m248-platform-cli`, based on `21e81eba77fbe69e5d6a25bf2ca71c857e107f8b`.
The local worktree is `/workspace/agentsfleet-platform-cli`.

## Approved scope

- Move Platform runners, model catalogue, provider defaults, and fleet library management into the command-line interface (CLI).
- Remove the Platform menu, pages, dialogs, and platform-only controls inside workspace chat.
- Move operator runner endpoints to `/v1/admin/runners`. Preserve the machine-owned `/v1/runners/me` endpoints.
- Keep workspace model selection, workspace library browsing, and workspace publication working.
- Replace Platform browser acceptance journeys with CLI operator acceptance coverage.
- Measure the app bundle reduction against the recorded baseline.
- Improve human CLI output and add animations for interactive terminals. Keep JSON and piped output clean.
- Record login, whoami, chat, steer, chat list, runner show, and fleet show for Indy's visual review.
- Update dependencies across the root, design system, app, CLI, and website. Major updates are authorized.

## Changes present

Platform handlers and schemas live under `cli/src/commands/platform/`.
Argument parsing lives under `cli/src/program/tree/platform*.command.ts`.
The root tree registers `platform runners`, `platform models`, `platform keys`, and `platform fleet-library`.

The implementation includes runner reads, enrollment, edits, state actions, token rotation, checks, events, and leases.
It also includes catalogue writes, platform default selection, and conditional fleet library writes.
These handlers still need acceptance tests and review.

The shared builders in `program/tree/flags.ts` now serve Platform arguments.
The new Platform flag module no longer duplicates identifier or integer validation messages.
Time arguments still need alignment with the existing CLI behavior.

Runner operator route literals and in-repository callers now use `/v1/admin/runners`.
Enrollment shares that collection, with method-sensitive `runner:read` and `runner:enroll` requirements.
The machine route prefix remains separate in `af_wire/src/paths.rs`.

Platform pages, dialogs, operator clients, dedicated tests, and browser journeys have been removed.
Navigation and chat cleanup is incomplete.
Retained runner execution fixtures have new helpers under `ui/packages/app/tests/helpers/runner-control*`.

## Known failures and unfinished checks

The last CLI typecheck passed after the dependency updates and shared flag changes.
The earlier CLI build and `platform --help` also passed.
Rebuild the CLI before testing: the generated files predate the final folder move.

The dashboard typecheck fails on incomplete cleanup:

- `WorkspaceChat.tsx` still contains the removed platform staging callback.
- Several layout components still destructure or require `operatorScopes`.
- Shared tests import deleted Platform pages, actions, dialogs, and loading components.
- Draft fixtures still accept `PlatformPublisher` and call removed staging methods.
- Browser performance, layout, and execution tests still import deleted Platform helpers.
- The browser acceptance configuration still names the removed platform library project.

The work gate and staged secret scan passed when preparing this checkpoint.
The verification gate failed on Rust formatting and a changed historical review paragraph.
Formatting was corrected, and incidental edits to historical review and baseline records were removed.

No Rust compilation, OpenAPI regeneration, full repository test lane, or CLI acceptance lane has completed.
Do not describe this checkpoint as ready to merge or deploy.

## Dependency updates

`bun update --latest --recursive` updated the root and UI workspaces.
`bun update --latest` updated the separate CLI package and lockfile.
The package files and lockfiles contain the exact versions.

Examples include Next.js `16.4.4` to `16.4.6`, Effect to `4.1.3`, Playwright to `1.65.0`, and jsdom `30.1.3` to `31.0.0`.

The update reported that `patches/@assistant-ui%2Freact@0.15.32.patch` no longer applies to `@assistant-ui/react@0.15.33`.
Compare the patch with upstream before deciding whether it needs rebasing.
Preserve the behavior that required the patch; the upgrade is not yet validated.

## Indy's review requests

Use enums for runner states, actions, and other finite values.
The new fixture vocabulary still uses constant objects and string unions.
That requested change has not been made.

Keep the sandbox engine separate from the isolation tier.
The removed runner UI showed Bubblewrap as the default engine, followed by the isolation selector.
Do not invent an unsupported policy field or treat `bubblewrap` as a tier.

Verify whether `container_nested` has distinct supported behavior before keeping or removing it.
The wire enum currently declares it, and `afd_runner/src/reconcile.rs` maps it and `landlock_full` to the same cage requirements.
Production `runner/main/src/host.rs` boots `BubblewrapBackend`; debug builds also have `UnsandboxedEngine` for `dev_none`.

A separate nested-container enforcement path has not been established.
Finish the source and test investigation before changing the supported tier list.

Apply the same feature-folder rule to other multi-file CLI groups, including billing and its schema.
Only Platform has been grouped so far.
Update imports and tests together when moving the remaining groups.

Use standard argument names and the shared builders.
The runner API supports `limit`, `starting_after`, `event_type`, `since`, `until`, `workspace_id`, and `fleet` on their respective reads.
The runner page limit is 1–100.

Runner event timestamps must use RFC 3339 on the wire.
The existing general CLI `--since` also advertises relative durations.
Align the accepted values without sending unsupported duration strings to runner endpoints.

## Credentials and conditional writes

`--token-out` reserves a new mode-0600 file before enrollment or token rotation.
The command saves the returned token there and prints only the path and runner metadata.
Those one-time credential calls have automatic retries disabled.

`--policy` currently reads a complete assigned policy from JSON.
Labels currently use comma-separated tags; the backend does not use Kubernetes key/value labels.
An empty label value clears the list.

Platform library writes preserve explicit operation identifiers and inspected version tags.
Staging a candidate does not publish it.
Model administration and platform key administration retain their separate scopes.

## Bundle baseline

The original app build and size check completed before source and dependency changes.
The configured environment-file value contained a literal tilde path that did not exist here.
Unsetting `AGENTSFLEET_UI_ENV_FILE` for the build allowed the supplied environment variables to be used.

| Measurement | Baseline |
| --- | ---: |
| Shared dashboard, gzip bytes from `bun run size --json` | 267750 |
| Emitted client JavaScript files | 113 |
| Client JavaScript bytes | 3814679 |
| Sum of individually gzipped JavaScript chunks | 1236748 |

The totals were calculated over `.next/static/chunks/**/*.js`, using Python gzip with `mtime=0`.
No updated bundle measurement exists yet.
Measure both shared dashboard and total emitted client JavaScript after the cleanup and upgrades.

## Resume sequence

1. Resolve the enum, sandbox-tier, and standard argument review requests.
2. Repair the incomplete dashboard source and retained tests.
3. Remove the remaining Platform browser project configuration while preserving workspace journeys.
4. Add deterministic CLI tests and live operator acceptance cases, separate from tenant-only success cases.
5. Finish feature-folder alignment and human output improvements, then record the requested command demonstrations.
6. Resolve dependency compatibility, including the assistant-ui patch.
7. Regenerate OpenAPI from Rust with `--no-banner` and verify the changed route scopes and callers.
8. Prepare companion documentation on a separate branch in the docs repository; it has not been edited.
9. Run the required make checks, rebuild the app, and compare the bundle measurements.

Do not bypass hooks, weaken checks, or label incomplete work as verified.
The user requested a checkpoint commit and branch push so another session can continue.
