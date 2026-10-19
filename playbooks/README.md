# `agentsfleet` Playbooks

These files rebuild, deploy, and operate `agentsfleet`. An Agent executes them;
a Human owns the outcome and every approval.

## Choose the route

| Human request | Route | Start |
|---|---|---|
| Set up for the first time or rebuild after a wipe | Chronological setup | [`founding/README.md`](./founding/README.md) |
| Deploy the existing development system | Routine deployment | [`deploy/dev/001_playbook.md`](./deploy/dev/001_playbook.md) |
| Deploy the existing production system | Routine deployment | [`deploy/prod/001_playbook.md`](./deploy/prod/001_playbook.md) |
| Perform one maintenance task | On-demand operation | The matching directory under [`operations/`](./operations/) |

These short requests are sufficient:

- `Set up agentsfleet development and production from scratch` selects the
  chronological setup route.
- `Start the standard production deployment` selects the routine production
  route.

The Agent runs read-only readiness checks, reports blockers, and stops at every
Human approval. The Agent must not silently switch routes.

## Roles

| Role | Responsibility |
|---|---|
| Human | Owns accounts, billing, secret entry, external consoles, destructive decisions, releases, and rollout approvals. |
| Agent | Runs repeatable workstation commands, prepares hosts, synchronizes configuration, starts Pipelines, and records evidence. |
| Pipeline | GitHub Actions builds, publishes, deploys, restarts, and runs automated verification. |

The Human is the owner. Agent and Pipeline are executors. Pipeline vault access
is pre-authorized through its GitHub environment; an Agent must obtain Human
approval before reading a vault or applying an external change.

## Verification

Every action names its executor, verifier, required evidence, and whether failure
blocks the next action.

- A Human console action is confirmed by the Human and checked by the Agent
  through an API when the provider exposes one.
- An Agent action is verified by the named `00_gate.sh` or `verify.sh`.
- A Pipeline action is verified by a named green GitHub Actions job and its run
  URL.
- `check` and `verify` are read-only. `apply` requires an explicit Human-approved
  write variable.
- A failed or skipped verification is not evidence and blocks the next action.
- Rebuild steps 06 and 09 aggregate development and production evidence,
  respectively; they do not replace the checks beside earlier actions.

Secrets move through 1Password. Never paste a raw provider credential into chat
or a command argument. A hostname in a playbook is desired state, not proof that
the route is live.

## Chronological setup

Run these steps in order for the first installation or after a wipe. Resume at
the first step whose required evidence is missing.

<!-- playbook-inventory:start -->
- `founding/01_bootstrap` — create accounts and populate secret stores.
- `founding/02_preflight` — prove pre-priming access and deployment inputs.
- `founding/03_priming_infra` — create resources and capture provider outputs.
- `founding/04_deploy_dev` — deploy the development control plane.
- `founding/05_runner_bootstrap_dev` — prepare and enroll the development runner.
- `founding/06_verify_dev` — run the complete development acceptance lanes.
- `founding/07_deploy_prod` — authorize and deploy the production control plane.
- `founding/08_runner_bootstrap_prod` — prepare and enroll production runners.
- `founding/09_verify_prod` — approve rollout and run production acceptance.
<!-- playbook-inventory:end -->

Production cannot start until step 06 is green. Setup is complete only when step
09 and its listed operational gates pass.

## Prime a clean database

Use this order after migrations finish, separately for each environment.
Set `VAULT_DEV` and `VAULT_PROD` from the deployment environment.
For single-vault sync scripts, also set `VAULT` to the selected environment's vault.
All credential values resolve from `op://` references at runtime.

| Order | Step | Database result | Required verification |
|---|---|---|---|
| 1 | [Deploy the API](./founding/04_deploy_dev/001_playbook.md), with runner rollout disabled | Schema and `audit._sqlx_migrations`; application tables begin empty | Migration succeeds and `/healthz` and `/readyz` return success |
| 2 | [Bootstrap the administrator](./operations/admin_bootstrap/001_playbook.md), through its credential step | Tenant, user, workspace, membership, wallet, and API key; Clerk receives owner metadata | Signed `user.created` succeeds; actual workspace ID is recorded; refreshed session opens platform pages |
| 3 | [Seed and configure models](./operations/model_catalogue/001_playbook.md), then select the platform default | `model.platform_models`, dated `model.platform_model_prices`, `model.platform_defaults`, and its encrypted provider credential | Unchanged seed reports no diff; chosen model has inference settings and a credentialed active default |
| 4 | Sync `approval-signing` using the guarded command below | Admin workspace's `vault.secrets` bag with `webhook_secret` | Sync succeeds; later acceptance proves a signed approval reply |
| 5 | Register [GitHub](./operations/github_app_registration/001_playbook.md), [Slack](./operations/slack_app_registration/001_playbook.md), [Linear](./operations/linear_app_registration/001_playbook.md), [Jira](./operations/jira_app_registration/001_playbook.md), and [Zoho](./operations/zoho_app_registration/001_playbook.md) | Provider application bags in admin `vault.secrets`; connecting a workspace later creates its credentials and `connector.installs` | Each guarded bag sync succeeds; each required workspace connect and refresh passes |
| 6 | [Register QStash](./operations/qstash_registration/001_playbook.md) | Admin `qstash` secret bag; authored schedules later create `fleet.schedules` rows | Registration is synced; one signed scheduled delivery is accepted and replay is deduplicated |
| 7 | [Register SMTP](./operations/smtp_relay_registration/001_playbook.md) | Admin `smtp-relay` secret bag; test invite creates an `identity.invites` row | Test invite reports sent, arrives, and accepts |
| 8 | Restart the control plane through the matching deployment workflow | No new priming rows; startup reads the completed provider bags | Readiness succeeds; required provider connections work |
| 9 | [Enroll and deploy runners](./operations/runner_onboarding/001_playbook.md) | `runner.runners` identity and assigned policy | Token is saved once; pipeline host gate passes; runner reports online |
| 10 | [Configure observability](./operations/observability/001_playbook.md) | No application rows; environment dashboard and alert rules live in Grafana | Read-only provider drift check succeeds and runtime series arrive |
| 11 | Run [development acceptance](./founding/06_verify_dev/001_playbook.md) or [production acceptance](./founding/09_verify_prod/001_playbook.md) | Disposable test fleets and their execution history | Named acceptance lanes and Human visual checks pass |

The Human creates `approval-signing-secret/credential` in the selected 1Password vault.
After approval, sync it without putting its value in arguments or output:

```bash
ENV=dev VAULT="${VAULT_DEV:?Set the development vault name}" \
ALLOW_VAULT_READS=1 ALLOW_PLATFORM_SECRET_WRITES=1 \
  ./playbooks/lib/platform_secret_sync.sh approval-signing
```

Expected output is `✓ approval-signing created in dev admin workspace`, or `replaced` when the bag already exists.
For production, select `ENV=prod` and `VAULT_PROD` after separate approval.

Complete every required provider registration before acceptance; a green health check does not prove credentials or inference work.
After a database wipe, existing Clerk users need signed signup-event replay; signing in alone creates no database rows.

### Recovery and maintenance routes

Resolve a [failed migration](./operations/failed_migration/001_playbook.md) before priming.
A [region move](./operations/database_region_move/001_playbook.md) copies an existing database with matching schema history; it does not rebuild old rows into this schema.
[Database teardown](./operations/teardown/database/001_playbook.md) permanently removes application data and requires separate approval before this sequence.
[Dragonfly teardown](./operations/teardown/dragonfly/001_playbook.md) removes queued work and deduplication state; stop all writers and verify every cluster member.
[User teardown](./operations/teardown/user/001_playbook.md) documents the unsupported purge path; deleting a Clerk identity does not erase database rows.

## Routine deployments

These routes deploy an installation that already passed the rebuild sequence.

<!-- playbook-inventory:start -->
- `deploy/dev` — deploy and verify the existing development system.
- `deploy/prod` — release and verify the existing production system.
<!-- playbook-inventory:end -->

## On-demand operations

<!-- playbook-inventory:start -->
- `operations/acceptance` — the human visual pass over a deployed build, and the verdict file the milestone's Pull Request carries.
- `operations/admin_bootstrap` — provision the platform administrator.
- `operations/ci_rust_images` — build the one pinned Rust base image for Continuous Integration (CI): the musl toolchain for the static daemon build, and the components the lint and unit lanes need.
- `operations/credential_rotation` — rotate an exposed development credential.
- `operations/database_region_move` — move a PlanetScale database to another region, verified table by table.
- `operations/failed_migration` — recover a release whose migration failed: fix the blocker and roll forward.
- `operations/github_app_registration` — register the platform GitHub App.
- `operations/installer_deploy` — deploy and verify `agentsfleet.dev`.
- `operations/ip_allowlisting` — restrict datastore ingress to Fly.io egress.
- `operations/jira_app_registration` — register the platform Jira app.
- `operations/linear_app_registration` — register the platform Linear app.
- `operations/model_catalogue` — prime and refresh the model catalogue.
- `operations/observability` — provision and verify observability providers.
- `operations/qstash_registration` — register QStash schedule credentials.
- `operations/runner_onboarding` — prepare and enroll a runner.
- `operations/slack_app_registration` — register the platform Slack app.
- `operations/slack_incident_drill` — set up and audit the development-first Linkwarden Slack incident drill.
- `operations/smtp_relay_registration` — register the relay that sends invite emails.
- `operations/teardown/database` — destructively empty a database.
- `operations/teardown/dragonfly` — destructively empty the Dragonfly cluster.
- `operations/teardown/user` — delete a development user and verify purge.
- `operations/zoho_app_registration` — register the platform Zoho Desk app.
<!-- playbook-inventory:end -->

## Script shape

Each playbook directory contains `001_playbook.md`. Where automation is useful:

- `00_gate.sh` is a thin entry point with an explicit command list.
- `check`, `apply`, and `verify` are the standard actions.
- `dev` and `prod` select one environment; write actions never default to both.
- Provider-specific behavior lives under `providers/<provider>/`.
- Shared libraries contain only behavior used by more than one caller.
- `*_test.sh` files are local regression tests.

Run the local integrity gate after any edit:

```bash
make check-playbooks
```

It runs ShellCheck, every shell regression test, path-reference checks, safety
checks, and an exact README-to-disk inventory comparison.

See [`ARCHITECTURE.md`](./ARCHITECTURE.md) for private API ingress.
