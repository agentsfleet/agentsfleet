---
type: reference
audience: operator
verified: 2026-11-18
product_version: 0.77.0
executable: false
---

# Operations

## Recovery and fleet operation

| Task | Guide |
| --- | --- |
| Back up, restore, upgrade, or repair a signup | [Recovery](./recovery/README.md) |
| Recover a failed migration | [Failed migration](./failed_migration/001_playbook.md) |
| Add or replace a runner | [Hosted runner onboarding](./runner_onboarding/001_playbook.md) · [Self-hosted runner enrollment](../PRIME.md#enroll-a-runner) |
| Configure the runner listener | [Runner TLS](./runner_stream_tls/001_playbook.md) |
| Refresh the model catalogue and prices | [Catalogue priming](./model_catalogue/001_playbook.md) |
| Connect Grafana and measure chat time | [Observability](./observability/001_playbook.md) · [Round-trip percentiles](./observability/002_round_trip_percentiles.md) |

## Optional capabilities

| Capability | Guide | Verification |
| --- | --- | --- |
| Email invitations | [SMTP relay](./smtp_relay_registration/001_playbook.md) | Receive an invitation in a separate mailbox. |
| Schedules | [QStash](./qstash_registration/001_playbook.md) | Accept a signed fire and see the run in the fleet. |
| Connected apps | [GitHub](./github_app_registration/001_playbook.md), [Slack](./slack_app_registration/001_playbook.md), [Zoho Desk](./zoho_app_registration/001_playbook.md), [Jira](./jira_app_registration/001_playbook.md), [Linear](./linear_app_registration/001_playbook.md) | Connect and inspect access from the intended workspace. |
| Incident drill | [Linkwarden Slack example](./slack_incident_drill/001_playbook.md) | Run the integration exercise when both Slack and GitHub are configured. |

## Hosted maintenance and acceptance

| Task | Guide |
| --- | --- |
| Grant platform permissions | [Hosted administrator bootstrap](./admin_bootstrap/001_playbook.md) |
| Rotate an exposed key | [Hosted credential rotation](./credential_rotation/001_playbook.md) |
| Move to a new database region | [PlanetScale region move](./database_region_move/001_playbook.md) |
| Maintain private datastore ingress | [PlanetScale allowlisting](./ip_allowlisting/001_playbook.md) |
| Publish the install site | [Installer deployment](./installer_deploy/001_playbook.md) |
| Publish pinned build images | [CI Rust images](./ci_rust_images/001_playbook.md) |
| Verify existing hosted releases | [Acceptance](./acceptance/001_playbook.md) |
| Wipe a database for an approved rebuild | [Database teardown](./teardown/database/001_playbook.md) |

[Acceptance baselines](./acceptance/baselines/) and [verdicts](./acceptance/verdicts/) retain release evidence. The current baselines and verdict template stay at the paths used by CI.
