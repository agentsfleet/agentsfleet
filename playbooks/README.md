---
type: reference
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# `agentsfleet` Playbooks

These pages rebuild, deploy and operate `agentsfleet`.
An Agent runs them. A Human owns every outcome and approval.

## Choose the route

| Human request | Start |
|---|---|
| Set up from scratch, or rebuild after a wipe | [Chronological setup](#chronological-setup) |
| Deploy the existing development system | [`deploy/dev/001_playbook.md`](./deploy/dev/001_playbook.md) |
| Deploy the existing production system | [`deploy/prod/001_playbook.md`](./deploy/prod/001_playbook.md) |
| Run one maintenance task | The matching directory in [On-demand operations](#on-demand-operations) |

`Set up agentsfleet development and production from scratch` selects chronological setup.
`Start the standard production deployment` selects the production route.
The Agent runs read-only checks first, reports blockers, and never switches routes silently.

## Roles and rules

| Role | Owns |
|---|---|
| Human | Accounts, billing, secret entry, provider consoles, destructive decisions, release tags, rollout approvals |
| Agent | Workstation commands, host preparation, configuration sync, starting Pipelines, evidence |
| Pipeline | GitHub Actions builds, deploys, restarts and automated verification |

- The Pipeline reads vaults through its GitHub environment without asking.
- The Agent asks the Human before it reads a vault or changes a provider.
- `check` and `verify` actions are read-only. `apply` needs a Human-approved `ALLOW_*` variable.
- Each action names its verifier: a `00_gate.sh` or `verify.sh` for Agent work, a green job URL for Pipeline work.
- A failed or skipped verification is not evidence, and it blocks the next action.
- Secrets move through 1Password. Never paste a provider credential into chat or a command argument.
- A hostname in a playbook is target state, not proof that the route is live.

The Agent stops for Human approval before it:

1. reads a vault interactively;
2. changes an external provider;
3. changes a live host or restarts work in flight;
4. creates or pushes a production release tag;
5. approves the production runner fleet;
6. runs a destructive teardown.

## Chronological setup

Run these in order for a first install or after a wipe.
Resume at the first step whose `Verify it works` evidence is missing, red or skipped.

| Step | Result |
|---|---|
| [01 Bootstrap](./founding/01_bootstrap/001_playbook.md) | Accounts, vaults, GitHub secrets, Vercel variables |
| [02 Preflight](./founding/02_preflight/001_playbook.md) | `bootstrap` gate green for both environments |
| [03 Priming](./founding/03_priming_infra/001_playbook.md) | Fly.io apps, datastores, tunnels, repository variables; `deployment` gate green |
| [04 Deploy dev](./founding/04_deploy_dev/001_playbook.md) | Development control plane green, runner lanes skipped |
| [05 Dev runner](./founding/05_runner_bootstrap_dev/001_playbook.md) | Priming done, runner enrolled, full development run green |
| [06 Verify dev](./founding/06_verify_dev/001_playbook.md) | Allowlisting, observability and provider live checks green |
| [07 Deploy prod](./founding/07_deploy_prod/001_playbook.md) | Approved tag released, runner jobs skipped |
| [08 Prod runners](./founding/08_runner_bootstrap_prod/001_playbook.md) | Priming done, hosts prepared, tokens stored |
| [09 Verify prod](./founding/09_verify_prod/001_playbook.md) | Canary and fleet green, npm `latest` promoted |

Production starts only after step 06 is green.
Setup is complete when step 09 and its operational gates pass.

## Prime a clean database

Run this order in each environment after migrations finish.
Set `VAULT_DEV` and `VAULT_PROD` from the deployment environment.
Single-vault sync scripts also need `VAULT` set to the selected environment's vault.

Every credential resolves from an `op://` reference at runtime.

| Order | Step | Database result | Verify |
|---|---|---|---|
| 1 | [Deploy the API](./founding/04_deploy_dev/001_playbook.md) with runner rollout disabled | Schema and `audit._sqlx_migrations`; application tables empty | Migration succeeds; `/healthz` and `/readyz` succeed |
| 2 | [Bootstrap the administrator](./operations/admin_bootstrap/001_playbook.md) through its credential step | Tenant, user, workspace, membership, wallet, API key; Clerk owner metadata | Signed `user.created` succeeds; workspace ID recorded; refreshed session opens platform pages |
| 3 | [Seed and configure models](./operations/model_catalogue/001_playbook.md), then select the platform default | `model.platform_models`, dated `model.platform_model_prices`, `model.platform_defaults`, encrypted provider credential | Unchanged seed reports no diff; chosen model has inference settings and a credentialed active default |
| 4 | Sync `approval-signing` (command below) | Admin `vault.secrets` bag with `webhook_secret` | Sync succeeds; acceptance later proves a signed approval reply |
| 5 | Register [GitHub](./operations/github_app_registration/001_playbook.md), [Slack](./operations/slack_app_registration/001_playbook.md), [Linear](./operations/linear_app_registration/001_playbook.md), [Jira](./operations/jira_app_registration/001_playbook.md), [Zoho](./operations/zoho_app_registration/001_playbook.md) | Provider bags in admin `vault.secrets`; a workspace connect later creates `connector.installs` | Each guarded bag sync succeeds; each workspace connect and refresh passes |
| 6 | [Register QStash](./operations/qstash_registration/001_playbook.md) | Admin `qstash` bag; authored schedules later create `fleet.schedules` | One signed scheduled delivery accepted; its replay deduplicated |
| 7 | [Register SMTP](./operations/smtp_relay_registration/001_playbook.md) | Admin `smtp-relay` bag; a test invite creates `identity.invites` | Test invite reports sent, arrives, accepts |
| 8 | Restart the control plane through the environment's deployment workflow | None; startup reads the provider bags | Readiness succeeds; provider connections work |
| 9 | [Enroll and deploy runners](./operations/runner_onboarding/001_playbook.md) | `runner.runners` identity and assigned policy | Token saved once; Pipeline host gate passes; runner online |
| 10 | [Configure observability](./operations/observability/001_playbook.md) | None; dashboard and alert rules live in Grafana | Read-only drift check succeeds; runtime series arrive |
| 11 | Run [development](./founding/06_verify_dev/001_playbook.md) or [production](./founding/09_verify_prod/001_playbook.md) acceptance | Disposable test fleets and their run history | Named acceptance lanes and Human visual checks pass |

For step 4, the Human creates `approval-signing-secret/credential` in the selected vault.
The Agent then syncs it with the [shared bag procedure](./operations/admin_bootstrap/001_playbook.md#sync-a-provider-secret-bag).

A green health check proves neither credentials nor inference.
Finish every provider registration before acceptance.
After a database wipe, existing Clerk users need a signed signup-event replay; signing in creates no rows.

### Recovery routes

- Resolve a [failed migration](./operations/failed_migration/001_playbook.md) before priming.
- A [region move](./operations/database_region_move/001_playbook.md) copies a database with matching schema history. It does not rebuild old rows into a new schema.
- [Database teardown](./operations/teardown/database/001_playbook.md) deletes application data permanently and needs its own approval.
- [User teardown](./operations/teardown/user/001_playbook.md) documents the unsupported purge path. Deleting a Clerk identity leaves its database rows.

## On-demand operations

| Directory under `operations/` | Use |
|---|---|
| `acceptance` | Human visual pass over a deployed build, and the verdict file the milestone Pull Request carries |
| `admin_bootstrap` | Create the platform administrator |
| `ci_rust_images` | Build the pinned Rust base image for Continuous Integration (CI): musl toolchain plus lint and unit needs |
| `credential_rotation` | Rotate an exposed development credential |
| `database_region_move` | Move a PlanetScale database to another region, verified table by table |
| `failed_migration` | Fix a failed release migration and roll forward |
| `github_app_registration` | Register the platform GitHub App |
| `installer_deploy` | Deploy and verify `agentsfleet.dev` |
| `ip_allowlisting` | Restrict datastore ingress to Fly.io egress |
| `jira_app_registration` | Register the platform Jira app |
| `linear_app_registration` | Register the platform Linear app |
| `model_catalogue` | Prime and refresh the model catalogue |
| `observability` | Set up and verify observability; `002_round_trip_percentiles.md` reads chat round trip p95 and p99 per fleet |
| `qstash_registration` | Register QStash schedule credentials |
| `runner_onboarding` | Prepare and enroll a runner |
| `slack_app_registration` | Register the platform Slack app |
| `slack_incident_drill` | Set up and audit the development-first Linkwarden Slack incident drill |
| `smtp_relay_registration` | Register the relay that sends invite emails |
| `teardown/database` | Empty a database (destructive) |
| `teardown/user` | Delete a development user and verify purge |
| `zoho_app_registration` | Register the platform Zoho Desk app |

## Script shape

- Each playbook directory holds `001_playbook.md`.
- `00_gate.sh` is a thin entry point with an explicit command list.
- `check`, `apply` and `verify` are the standard actions.
- `dev` or `prod` selects one environment; a write never defaults to both.
- Provider code lives under `providers/<provider>/`.
- Shared libraries hold only code with more than one caller.
- `*_test.sh` files are local regression tests.

After any edit, run the local integrity gate:

```bash
make check-playbooks
```

It runs vault-gate parity, path-reference checks, ShellCheck and every shell regression test.
It exits 0 when all pass. A failure prints `✗ <N> regression test file(s) failed`.

[`ARCHITECTURE.md`](./ARCHITECTURE.md) describes private API ingress and how to diagnose the API.
