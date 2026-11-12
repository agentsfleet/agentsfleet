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

Run these in order after migrations finish, with `ENV`, `VAULT` and `AGENTSFLEET_API_URL` set for the environment.
Every value comes from 1Password at runtime; nothing is pasted.

| # | Do | Proof |
|---|---|---|
| 1 | [Deploy](./founding/04_deploy_dev/001_playbook.md) with the runner disabled | `/readyz` succeeds |
| 2 | [Bootstrap the administrator](./operations/admin_bootstrap/001_playbook.md): account, platform scopes, `agentsfleet login`, workspace pointer, API key | `agentsfleet whoami` lists the platform scopes |
| 3 | Sync every bag in the [bag table](./operations/admin_bootstrap/001_playbook.md#sync-a-provider-secret-bag) | Each sync prints `✓ <bag> created` |
| 4 | [Seed the model catalogue](./operations/model_catalogue/001_playbook.md), then set the default with `agentsfleet platform keys set` | `agentsfleet platform keys list` shows one active default |
| 5 | [Enroll the runner](./operations/runner_onboarding/001_playbook.md) and redeploy; the restart also loads the startup-read bags | `agentsfleet platform runners list` shows it online |
| 6 | [Configure observability](./operations/observability/001_playbook.md) | Runtime series arrive |
| 7 | Run [development](./founding/06_verify_dev/001_playbook.md) or [production](./founding/09_verify_prod/001_playbook.md) acceptance | Named lanes pass |

After a database wipe, an existing Clerk user needs its signed `user.created` event resent; signing in creates no rows.
A green health check proves neither credentials nor inference.
A provider registered earlier keeps its 1Password item, so a wipe needs only step 3; a new provider starts at its registration page.

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
