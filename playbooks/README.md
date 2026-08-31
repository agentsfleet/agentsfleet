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

## Routine deployments

These routes deploy an installation that already passed the rebuild sequence.

<!-- playbook-inventory:start -->
- `deploy/dev` — deploy and verify the existing development system.
- `deploy/prod` — release and verify the existing production system.
<!-- playbook-inventory:end -->

## On-demand operations

<!-- playbook-inventory:start -->
- `operations/admin_bootstrap` — provision the platform administrator.
- `operations/cutover` — the Rust daemon cutover runbook, its declared-divergence register, and the probe runner that asserts rubric-row coverage.
- `operations/ci_rust_images` — build the one pinned Rust base image for Continuous Integration (CI): the musl toolchain for the static daemon build, and the components the lint and unit lanes need.
- `operations/ci_zig_images` — build pinned Zig images for Continuous Integration (CI).
- `operations/credential_rotation` — rotate an exposed development credential.
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
- `operations/teardown/database` — destructively empty a database.
- `operations/teardown/redis` — destructively empty Redis.
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
