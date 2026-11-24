# Self-hosted setup and priming proposal

The approved entry points are [SELF_HOST.md](../../playbooks/SELF_HOST.md) and [PRIME.md](../../playbooks/PRIME.md). The [hosted operations index](../../playbooks/hosted/README.md) carries the original release and clean-database sequence. [Recovery](../../playbooks/operations/recovery/README.md) includes backup, isolated restore, and the retained-account diagnostic.

## Remove these from the setup sequence first

| Current material | Proposed disposition |
| --- | --- |
| [Historical acceptance baselines](../../playbooks/operations/acceptance/baselines/) and [verdicts](../../playbooks/operations/acceptance/verdicts/) | Move superseded evidence out of the setup index. Keep the current comparison inputs, the verdict template, and every file still read by a workflow. Move older evidence to a dated archive once inbound references have been checked. |
| [Development user teardown](../../playbooks/operations/teardown/user/001_playbook.md) | Fold its finding into recovery documentation. The signed identity route leaves application records on `user.deleted`, and the page provides diagnosis for a purge that the product cannot perform. Keep the separate database teardown procedure. |
| [Linkwarden Slack incident drill](../../playbooks/operations/slack_incident_drill/001_playbook.md) | Put it under an opt-in integration example. It configures GitHub and Slack for an incident drill. Use a simple chat fleet for install verification. |
| [Installer deployment](../../playbooks/operations/installer_deploy/001_playbook.md), [Rust build images](../../playbooks/operations/ci_rust_images/001_playbook.md), and [PlanetScale allowlisting](../../playbooks/operations/ip_allowlisting/001_playbook.md) | Remove these from the self-hosted route. Keep them as hosted maintenance guides. Workflows and `make/build.mk` read the build-image files directly. |
| [Fly.io and Cloudflare ingress](../../playbooks/ARCHITECTURE.md), hosted [development deployment](../../playbooks/deploy/dev/001_playbook.md) and [production deployment](../../playbooks/deploy/prod/001_playbook.md) | Group under a hosted operations index. Keep the release, canary and rollback instructions for `agentsfleet.net`. |
| [Development](../../playbooks/founding/04_deploy_dev/001_playbook.md) and [production](../../playbooks/founding/07_deploy_prod/001_playbook.md) first deployments, their [runner](../../playbooks/founding/05_runner_bootstrap_dev/001_playbook.md) [bootstrap](../../playbooks/founding/08_runner_bootstrap_prod/001_playbook.md) and [verification](../../playbooks/founding/09_verify_prod/001_playbook.md) pages | Consolidate the prose after preserving the distinct first-install skip conditions, environment-specific rollout checks and release promotion. CI calls the preflight gate and development runner wrapper by path, so move scripts only alongside their callers. |

Consolidate links and move the redundant user-teardown explanation first. Check callers before deleting any files. Continuous Integration (CI) reads first-use comparison files under `operations/acceptance`, and `make/test-infra.mk` reads the teardown SQL.

[First-use CI lane](../../.github/workflows/test-integration-rustd.yml) · [Test environment](../../make/test-infra.mk) · [Playbook reference gate](../../make/quality.mk#L152)

The hosted account, secret-store, preflight and infrastructure pages remain the source for existing development and production installations. Keep failed migration, credential rotation, database region move and database teardown as on-demand operations. Add a backup and restore guide.

Keep provider registration for GitHub, Slack, Zoho, Jira, Linear, mail and schedules as capability guides. Keep runner onboarding and Transport Layer Security (TLS) steps under the runner guide, and move Grafana setup and chat percentiles under observability. [Current playbook index](../../playbooks/README.md)

## Entry points

```text
playbooks/README.md             Index for self-hosted and hosted operations
playbooks/SELF_HOST.md          Give this path to the setup agent
  ├── deploy/self-host/         Versioned deployment, config template and host service files
  ├── playbooks/PRIME.md        Entry point for a migrated database with working ingress
  │   ├── identity and platform administrator
  │   ├── model catalogue, provider secret, default and decision model
  │   ├── runner registration and verification
  │   └── optional mail, connectors and schedules
  ├── install verification     Recorded fleet, stream and runner proofs
  └── operations index         Upgrade, diagnose, back up, restore and rotate
```

Give the agent `playbooks/SELF_HOST.md` from the version you are installing. Ask it to follow the guide through the first successful fleet reply. `SELF_HOST.md` records the environment and the order.

Give the agent `PRIME.md` after restoring or replacing a database.

Each step links to the executable file or the owning reference. Its instructions specify inputs, the resulting state, the verification command and the recovery path.

## What `SELF_HOST.md` would contain

| Section | Contents | Success evidence |
| --- | --- | --- |
| Choose target and release | Select the deployment host, application and API origins, runner address, database location, release revision and enabled capabilities. List the exact supported Linux distribution, architecture and minimum runtime versions verified by installation tests. Record the daemon, runner, dashboard and CLI revisions together. | An installation manifest with release and file digests, chosen origins, host checks and capability selections. |
| Topology and required inputs | Start with one Linux deployment host and one runner, which may share it if the kernel checks pass. Describe Postgres, a three-node core NATS cluster, the daemon, dashboard, TLS ingress and the runner listener. Configure an identity project for dashboard and CLI sign-in and one model provider for the first fleet. Define each secret's consumer and supply path from repository config. | A read-only preflight reports service ports, runtime versions, storage, TLS and named missing inputs. Run it before installing. |
| Deploy control plane | Start Postgres and NATS, apply migrations under the deployment workflow, start `agentsfleetd`, verify API ingress, then start the dashboard. Use one versioned deployment bundle and one configuration source. Explain persistent volumes, the data-plane network boundary and separate public dashboard, API and runner endpoints. | Known schema at the chosen revision, public `/healthz` and `/readyz`, three reachable NATS members, dashboard sign-in page and a reachable runner listener. |
| Prime | Run `PRIME.md` after the schema and endpoint checks. Point it at the same manifest and verify the signed identity event before attempting CLI login. | Priming evidence references the exact installation, identity scope, provider and runner IDs. |
| Verify the first fleet | Sign in through the dashboard and CLI. Create a private setup, get a real assistant reply, inspect and save a small fixture named **Release checklist**, then send it a runtime message. Check the stream output, runner assignment and saved turn from a fresh session. Keep the fixture ID in verification output for future smoke checks. | One setup reply, one saved fleet, one completed runtime turn, runner online, and traces tied to the saved fleet and turn IDs. |
| Resume and operate | On rerun, read the installed state and resume at the first missing proof. Link to controlled upgrades and rollback, database backup and test restore, key rotation, runner replacement and diagnostics. Document ownership and prerequisites for resources the operator can remove. | A restart preserves the saved fleet and an interrupted setup can resume. A separate recovery drill proves that a backup restores into an isolated installation. |

For the first supported profile, self-hosted means the operator runs the application, daemon, database, bus and runner on infrastructure they control. The dashboard currently uses Clerk and the first real reply requires a model provider. The install guide must state those dependencies and their setup before claiming that the application is usable.

A deployment with a self-run identity provider and local inference would need its own verified compatibility work. Treat mail, schedules, connectors and observability as individually selectable capabilities. If the operator enables one, its verifier becomes part of the install evidence.

[Identity configuration](../AUTH.md#configuration-knobs-from-rustdcratesdaemonmainsrcpreflightmodrs) · [Postgres and NATS topology](../architecture/datastore_scaling.md) · [Runner transport](../architecture/runner_protocol.md)

The local development scripts prove that the code can run on a machine with its own data services. Use their service wiring, schema setup, and integration probes as input to the deployment design. The acceptance emulator, local certificates, development authentication fixtures and development host defaults belong to that test environment.

The proposed `deploy/self-host/` bundle needs a real clean-host installation and restore before its commands become the supported setup route. [Local development](../development.md) · [Development service definitions](../../docker-compose.yml) · [Local acceptance bootstrap](../../deploy/local/bootstrap.ts)

## What `PRIME.md` would contain

| Order | Reuse | Inputs and verification |
| --- | --- | --- |
| 1. Establish operator identity | Signed signup and [administrator bootstrap](../../playbooks/operations/admin_bootstrap/001_playbook.md) | Require a working identity issuer and signed signup delivery. Resolve the returned operator and workspace IDs and prove the minimal scopes for the requested actions in `whoami`. Give runtime automation its own tenant key when needed. |
| 2. Seed model data | [Model catalogue and priming](../../playbooks/operations/model_catalogue/001_playbook.md) | Read the pinned catalogue for the release. Show the diff before applying catalogue and price changes. Confirm provider, reference and price at the same catalogue generation. |
| 3. Configure inference | [Provider secret bag and default selection](../../playbooks/operations/admin_bootstrap/001_playbook.md#sync-a-provider-secret-bag) | Map one private provider credential into the operator workspace. Select the platform default and configure a decision model if the assigned runner policy requires one. Verify identifiers, invocation and billing on a real test turn. |
| 4. Enroll a runner | [Onboarding](../../playbooks/operations/runner_onboarding/001_playbook.md) and [runner listener TLS](../../playbooks/operations/runner_stream_tls/001_playbook.md) | Verify kernel isolation, stable host ID, allowed network policy and trust chain. Store the returned token in the service credential store. A repeated registration resolves the existing host; a lost token follows the explicit rotation route. An online heartbeat and self-test prove the host reached the intended daemon. |
| 5. Enable capabilities | [Operational capability guides](../../playbooks/operations/README.md#optional-capabilities) | For each selected provider, write the endpoint, scopes, required secret references, probe and removal procedure. Mail proves one invitation, schedules prove an accepted signed fire, and connected apps prove a workspace-scoped connection. |
| 6. Record the result | First-fleet verification in `SELF_HOST.md` | Return machine-readable evidence keyed by release, deployment, workspace, runner, fleet and turn. Persist identifiers, observed status and durations. Credentials and prompts stay with their owning services. |

Priming needs an explicit target and uses existing natural keys to identify a resource on rerun. Each step reads before writing and checks the result after writing. When a request outcome is uncertain, re-read the target by that key and continue from the recorded result.

Resource replacement and key rotation receive separate actions. Optional capabilities have their own verification results. The final summary names which checks passed, failed, or were skipped and what evidence each produced.

## Migration and acceptance

1. Approve the table of retained, consolidated and archived pages. Inventory links and callers in the repository, Continuous Integration and the public documentation before moving anything.
2. Draft `SELF_HOST.md` and `PRIME.md` against the existing authentication, deployment and runner files. Start from the single pointer for both a clean installation and a restart after a failed priming step. Capture every step where a human must supply an input.
3. Build the minimal supported self-hosted bundle using existing tools. Prove fresh install, restart, restart-and-resume, backup restoration, key rotation and a real model reply. Test a missing credential, a wrong target, an offline runner and a missing provider response as distinct failures.
4. Add live acceptance coverage for sign-in, tenant isolation, fleet setup and saved-fleet chat. Add deterministic shell, API and browser tests for missing inputs, partial priming and duplicate registration. Record submit-to-acceptance, runner assignment, first visible answer and final completion for the fixture.
5. Move hosted notes and evidence after the replacement route passes. Run `make check-playbooks` and the referenced workflow and acceptance checks before dropping any original entry point. Preserve an index link for operators following old hosted links.

Approval covers the agent's entry point, the scope of the first self-hosted profile, and which hosted pages to consolidate.
