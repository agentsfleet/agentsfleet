---
type: reference
audience: operator
verified: 2026-11-18
product_version: 0.77.0
executable: false
---

# Hosted installation and operations

Use this route for development and production on agentsfleet.net.
The [operations index](../operations/README.md) also covers capabilities and shared recovery.

| Task | Start |
| --- | --- |
| First installation | [Chronological setup](#chronological-setup) |
| Prime after migration | [Prime a clean database](#prime-a-clean-database) |
| Release in development | [Development deployment](../deploy/dev/001_playbook.md) |
| Release in production | [Production deployment](../deploy/prod/001_playbook.md) |
| Diagnose ingress | [Hosted architecture](../ARCHITECTURE.md) |

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
| [01 Bootstrap](../founding/01_bootstrap/001_playbook.md) | Accounts, vaults, GitHub secrets, Vercel variables |
| [02 Preflight](../founding/02_preflight/001_playbook.md) | `bootstrap` gate green for both environments |
| [03 Priming](../founding/03_priming_infra/001_playbook.md) | Fly.io apps, datastores, tunnels, repository variables; `deployment` gate green |
| [04 Deploy dev](../founding/04_deploy_dev/001_playbook.md) | Development control plane green, runner lanes skipped |
| [05 Dev runner](../founding/05_runner_bootstrap_dev/001_playbook.md) | Priming done, runner enrolled, full development run green |
| [06 Verify dev](../founding/06_verify_dev/001_playbook.md) | Allowlisting, observability and provider live checks green |
| [07 Deploy prod](../founding/07_deploy_prod/001_playbook.md) | Approved tag released, runner jobs skipped |
| [08 Prod runners](../founding/08_runner_bootstrap_prod/001_playbook.md) | Priming done, hosts prepared, tokens stored |
| [09 Verify prod](../founding/09_verify_prod/001_playbook.md) | Canary and fleet green, npm `latest` promoted |

Production starts only after step 06 is green.
Setup is complete when step 09 and its operational gates pass.

## Prime a clean database

Run these in order after migrations finish, with `ENV`, `VAULT` and `AGENTSFLEET_API_URL` set for the environment.
Every value comes from 1Password at runtime; nothing is pasted.

1. [Deploy](../founding/04_deploy_dev/001_playbook.md) with the runner disabled.
   Proof: `/readyz` succeeds.
2. [Bootstrap the administrator](../operations/admin_bootstrap/001_playbook.md): account, platform scopes, `agentsfleet login`, workspace pointer, API key.
   Proof: `agentsfleet whoami` lists the platform scopes.
3. Sync every bag in the [bag table](../operations/admin_bootstrap/001_playbook.md#sync-a-provider-secret-bag).
   Proof: each sync prints `✓ <bag> created`.
4. [Seed the model catalogue](../operations/model_catalogue/001_playbook.md).
5. Store the provider secret and [select the default](../operations/model_catalogue/001_playbook.md#activate-a-primed-model).
   Proof: `agentsfleet platform models default list` shows the selected model.
6. [Seed the decision model](../operations/model_catalogue/001_playbook.md#seed-the-decision-model).
   Proof: `agentsfleet platform models list` shows `typesafe`.
7. [Enroll the runner](../operations/runner_onboarding/001_playbook.md) with the decision model.
8. Redeploy; the restart also loads the startup-read bags.
   Proof: `agentsfleet platform runners list` shows the runner online.
9. [Configure observability](../operations/observability/001_playbook.md).
   Proof: runtime series arrive.
10. Run [development](../founding/06_verify_dev/001_playbook.md) or [production](../founding/09_verify_prod/001_playbook.md) acceptance.
    Proof: the named lanes pass.

After a database wipe, an existing Clerk user needs its signed `user.created` event resent; signing in creates no rows.
A green health check proves neither credentials nor inference.
A provider registered earlier keeps its 1Password item, so a wipe needs only step 3; a new provider starts at its registration page.

### Recovery routes

- Resolve a [failed migration](../operations/failed_migration/001_playbook.md) before priming.
- A [region move](../operations/database_region_move/001_playbook.md) copies a database with matching schema history. It does not rebuild old rows into a new schema.
- [Database teardown](../operations/teardown/database/001_playbook.md) deletes application data permanently and needs its own approval.
- [Identity recovery](../operations/recovery/README.md#identity-recovery) covers missing and retained accounts.
