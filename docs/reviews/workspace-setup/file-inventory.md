---
type: reference
audience: contributor
verified: 2026-10-16
product_version: 0.61.0
executable: false
---

# Workspace setup file inventory

## Synopsis

Exact whole-repository rename hits and carried uncommitted paths at the inspected revision. Specs name these same paths in grouped rows. Planned CREATE files do not exist yet and are not source evidence. Implementation is active in the existing worktree; this inventory records intended and completed changes, not proof of served behavior.

## Example with output

The discovery command is `git grep -l -w -e daily_dollars -e monthly_dollars -e trigger_markdown -e library-entries`. It found 202 tracked paths. Historical files and input-only names are classified below; this is not a blanket zero-match requirement.

## Options

| File | Scope | Action |
|---|---|---|
| `README.md` | M227_002 carried batch | complete/edit later |
| `branding/README.md` | M227_002 carried batch | complete/edit later |
| `branding/agentsfleet-dark.png` | M227_002 carried batch | complete/edit later |
| `branding/agentsfleet-dark.svg` | M227_002 carried batch | complete/edit later |
| `branding/agentsfleet-light.png` | M227_002 carried batch | complete/edit later |
| `branding/agentsfleet-light.svg` | M227_002 carried batch | complete/edit later |
| `branding/agentsfleet-mark-glow.png` | M227_002 carried batch | complete/edit later |
| `branding/agentsfleet-mark-glow.svg` | M227_002 carried batch | complete/edit later |
| `branding/agentsfleet-mark-transparent.png` | M227_002 carried batch | complete/edit later |
| `branding/agentsfleet-mark-transparent.svg` | M227_002 carried batch | complete/edit later |
| `branding/agentsfleet-mark.svg` | M227_002 carried batch | complete/edit later |
| `branding/avatar-1024.png` | M227_002 carried batch | complete/edit later |
| `branding/avatar-512.png` | M227_002 carried batch | complete/edit later |
| `branding/favicon.ico` | M227_002 carried batch | complete/edit later |
| `branding/favicon.svg` | M227_002 carried batch | complete/edit later |
| `branding/github-avatar.png` | M227_002 carried batch | complete/edit later |
| `branding/github-social.png` | M227_002 carried batch | complete/edit later |
| `branding/github-social.svg` | M227_002 carried batch | complete/edit later |
| `cli/README.md` | M227_002 carried batch | complete/edit later |
| `cli/src/commands/fleet_install_source.ts` | M227_001 cutover | complete/edit later |
| `cli/src/commands/fleet_library_create.ts` | M227_001 cutover | complete/edit later |
| `cli/src/lib/api-paths.ts` | M227_001 cutover | complete/edit later |
| `cli/test/acceptance/fixtures/template-ops.ts` | M227_001 cutover | complete/edit later |
| `cli/test/acceptance/library-onboard-live.spec.ts` | M227_001 cutover | complete/edit later |
| `cli/test/fleet-update.integration.test.ts` | M227_001 cutover | complete/edit later |
| `cli/test/library-create-wire.integration.test.ts` | M227_001 cutover | complete/edit later |
| `cli/test/library-entries.unit.test.ts` | M227_001 cutover | complete/edit later |
| `cli/test/library-verbs.integration.test.ts` | M227_001 cutover | complete/edit later |
| `cli/test/library-verbs.unit.test.ts` | M227_001 cutover | complete/edit later |
| `cli/test/log-level.unit.test.ts` | M227_001 cutover | complete/edit later |
| `docs/DESIGN_SYSTEM.md` | M227_002 carried batch | complete/edit later |
| `docs/SKILL_FRONTMATTER_SCHEMA.md` | M227_001 cutover | complete/edit later |
| `docs/architecture/README.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/architecture/billing_and_provider_keys.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/architecture/capabilities.md` | M227_001 cutover | complete/edit later |
| `docs/reviews/workspace-setup/overview.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/api.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/architecture/api_resources.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/chatgpt-review-prompt.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/claude-review-prompt.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/codex-response.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/architecture/data_flow.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/decisions.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/architecture/fleet_definitions.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/end-to-end.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/followup-review-prompt.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/public-changes.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/review-packet.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/rewrite-report.md` | M227_001 cutover | preserved review artifact |
| `docs/architecture/runner_fleet.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/runtime-baseline.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/reviews/workspace-setup/source-findings.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/architecture/connectors.md` | M227_001 cutover | complete/edit later |
| `docs/architecture/data_flow.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/architecture/fleet_definitions.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/architecture/product_analytics.md` | M227_002 carried batch | complete/edit later |
| `docs/architecture/runner_execution.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/architecture/runner_fleet.md` | M227_001 cutover | complete/edit later |
| `docs/architecture/scenarios/production-deploy-repair.md` | M227_001 cutover | complete/edit later |
| `docs/architecture/web_app.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/architecture/website-job-entrance.md` | M227_002 carried batch | complete/edit later |
| `docs/designs/chat-within-a-workspace.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/designs/workspace-chat-options.md` | M227_001 cutover | common architecture updated; implementation proofs pending |
| `docs/v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M122_001_P1_API_DOCS_SERVED_VS_DOCUMENTED_PARITY.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M122_005_P1_API_FLEET_BUDGET_ENFORCEMENT.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M128_001_P1_API_UI_RUNTIME_PLATFORM_CATALOG.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M131_001_P1_API_UI_FLEET_CONSOLE.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M137_001_P1_UI_STEER_FIRST_CONSOLE_PLAIN_LANGUAGE.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M157_001_P1_API_INFRA_OBS_SKILL_INCIDENT_TO_APPROVED_DRAFT_PR.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M17_001_P1_API_HARNESS_AGENT_PROFILE_TEARDOWN.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M199_001_P0_CLI_DOCS_LIBRARY_ONBOARDING_AND_APPROVAL_INBOX.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M19_003_P1_CLI_INSTALL_FROM_PATH.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M202_001_P0_API_CLI_DOCS_GRANT_COVERS_REPOSITORY_WRITE.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M204_001_P1_API_CLI_UI_WORKSPACE_LIBRARY_REMOVAL.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M213_002_P0_API_DOCS_RUST_SANDBOX_EGRESS_ALLOWLIST.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M2_002_P0_SKILL_CLAWHUB_SKILL_FORMAT.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M37_001_P1_SKILL_PLATFORM_OPS_ZOMBIE.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M44_001_P1_API_CLI_INSTALL_CONTRACT_AND_DOCTOR.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M46_001_P1_SKILL_DOCS_API_FRONTMATTER_SCHEMA.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M65_001_P1_TESTING_SECURITY_AUTH_E2E_FULL_LIFECYCLE_SCENARIOS.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M68_001_P1_API_CLI_UI_DOCS_WEBSITE_TRIGGER_REGISTRATION_AND_FREE_TRIAL.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M84_008_P0_API_RUNNER_SKILL_INSTRUCTIONS_LEASE.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M90_002_P0_API_EVENT_LIFECYCLE_STRAND_RECOVERY.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M94_002_P1_API_CLI_DOCS_SKILL_UI_FLEET_BUNDLE_IMPORT.md` | M227_001 cutover | PRESERVE historical |
| `docs/v2/done/M96_001_P2_API_DOCS_INFRA_BUNDLE_R2_CANONICAL_STORE.md` | M227_001 cutover | PRESERVE historical |
| `public/openapi.json` | M227_001 cutover | complete/edit later |
| `rustd/Cargo.lock` | M227_001 cutover | complete/edit later |
| `rustd/Cargo.toml` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_admission/src/admit_receipt.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/src/router/mount.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/app_ingress_live/fixture.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/fleet_lifecycle_live/fixture.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/fleet_lifecycle_live/library_removal.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/fleet_lifecycle_live/slack_channel.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/harness/webhook.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/ingress_live/fixture.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/integration_authoring.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_api/tests/authoring/billing.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_api/tests/authoring/privacy.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_api/tests/authoring/recovery.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_api/tests/integration_fleet_install_credentials.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/integration_slack_mention.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/integration_slack_mention/resident_bound.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/integration_workspace_library_entries.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/list_limits.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/route_inventory.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/slack_mention_live/fixture.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/workspace_fleets_input.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api/tests/workspace_library_entries_live/fixture.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api_ingress/src/handler/mention/resident.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api_operator/src/handler/admin/library_import.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api_runner/src/handler/runner/authoring/mod.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_api_runner/src/lib.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api_tenant/src/handler/draft/mod.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_api_tenant/src/handler/draft/message.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_api_tenant/src/handler/draft/review.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_api_tenant/src/handler/draft/stream.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_api_tenant/src/handler/fleet/detail.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api_tenant/src/handler/fleet/detail_request.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api_tenant/src/handler/library_entry.rs` | M227_001 cutover | DELETE later |
| `rustd/crates/afd_api_tenant/src/handler/library_entry/list.rs` | M227_001 cutover | DELETE later |
| `rustd/crates/afd_api_tenant/src/handler/library_entry/remove.rs` | M227_001 cutover | DELETE later |
| `rustd/crates/afd_api_tenant/src/handler/library_entry/tests.rs` | M227_001 cutover | DELETE later |
| `rustd/crates/afd_api_tenant/src/lib.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api_wire/src/admin_library.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api_wire/src/authoring/` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_api_wire/src/fleet.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api_wire/src/lib.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_api_wire/src/workspace_library.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_approval/src/request/install.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_authoring/Cargo.toml` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_authoring/src/candidate/mod.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_authoring/src/draft/mod.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_authoring/src/lease/mod.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_authoring/src/lib.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_authoring/src/message/mod.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_authoring/src/privacy/mod.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_authoring/src/sql/mod.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_authoring/tests/authoring_suite.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_bench/src/lane/lease/drain/stage.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_billing/src/budget.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_billing/src/nanos.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_core/src/error_code.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_core/src/error_code/authoring.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_core/src/error_code/fleet.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_core/src/problem.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_core/src/problem/authoring.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/afd_core/src/problem/fleet.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_db/src/migration.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_events/src/steer.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/src/lease/admit/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/src/lease/answer/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/src/lease/assign.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/src/lease/coverage.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/src/lease/deliver/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/src/lease/mint.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/src/lease/sql/lease.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/src/lease/sql/renew.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/src/lease/sql/report.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/tests/integration_credential_mint/write_gate.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/tests/integration_lease_gates.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/tests/integration_lease_installed.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/tests/integration_ledger_fleet_name.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/tests/integration_ledger_scope.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/tests/integration_money_gates.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/tests/integration_renew_coverage.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet/tests/integration_terminal_redelivery.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/src/edit.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/src/edit/conditional.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/src/edit/rewrite.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/src/edit/rewrite/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/src/install.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/src/install/authored.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/src/install/row.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/src/read.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/src/sql.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/src/sql/install.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/tests/integration_install_credentials.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/tests/integration_install_grants.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/tests/support/lane.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_lifecycle/tests/support/lane_seed.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_runtime/src/config/attach/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_runtime/src/config/binding_match.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_runtime/src/config/policy.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_runtime/src/config/policy/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_runtime/src/config/raw/policy.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_runtime/src/error.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_runtime/src/error/raise.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_runtime/src/frontmatter/trigger.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_runtime/tests/frontmatter_fields.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_fleet_runtime/tests/frontmatter_mention.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_gate/src/gate/first.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_gate/src/gate/sql.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_gate/src/policy/egress/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_gate/src/policy/fixture.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_gate/tests/support/gate_fixture.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_http/src/envelope.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_http/src/handler/library_onboard.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_http/src/route/fleet.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_http/src/route/runner.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_http/src/route/workspace.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_http/src/services/ingress/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_ingress/src/app/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_ingress/src/binding/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_ingress/src/slack/notice.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_ingress/src/slack/notice_tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_ingress/src/slack/resident.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_ingress/src/slack/resident_tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_ingress/src/slack/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/catalogue/entries.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/catalogue/entries/sql.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/catalogue/gallery/sql.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/catalogue/import.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/catalogue/import/tenant.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/catalogue/store.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/github.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/github/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/model.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/persist/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/prepare.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/prepare/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/preview.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/snapshot.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/snapshot/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/source/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/validate.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/src/validate/tests.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/tests/integration_catalogue/entry_privileges.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/tests/integration_catalogue/fixtures.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/tests/integration_catalogue/gallery_seed.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/tests/integration_catalogue/owned_paging.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_library/tests/version_policy.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_mail/src/invite/snapshots/afd_mail__invite__tests__invite_html.snap` | M227_002 carried batch | complete/edit later |
| `rustd/crates/afd_mail/templates/invite.html` | M227_002 carried batch | complete/edit later |
| `rustd/crates/afd_mail/templates/layout.html` | M227_002 carried batch | complete/edit later |
| `rustd/crates/afd_runner/src/heartbeat.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_wire/src/event/steer.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_wire/src/lease.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afd_wire/src/paths.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/afr_agent/src/authoring/mod.rs` | M227_001 cutover | CREATE later |
| `rustd/crates/agentsfleetd/tests/integration_runner_messages.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/agentsfleetd/tests/support/definition_install/mod.rs` | M227_001 cutover | complete/edit later |
| `rustd/crates/agentsfleetd/tests/support/e2e_seed.rs` | M227_001 cutover | complete/edit later |
| `schema/450_fleet_library.sql` | M227_001 cutover | PRESERVE historical |
| `schema/460_tenant_fleet_library.sql` | M227_001 cutover | PRESERVE historical |
| `schema/500_fleets.sql` | M227_001 cutover | PRESERVE historical |
| `schema/917_tenant_fleet_library_delete_grant.sql` | M227_001 cutover | PRESERVE historical |
| `schema/935_authoring_drafts.sql` | M227_001 cutover | CREATE later |
| `tests/fixtures/fleetbundle/ci-repairer/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/ci-responder/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/delegating-triager/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/incident-repairer/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/incident-responder/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/incident-verifier/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/platform-ops/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/security-reviewer/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/skill/name_mismatch/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/steer-probe/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/test-fixer/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/trigger/full.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/trigger/minimal.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/trigger/runtime_at_top_level.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/trigger/unknown_runtime_key.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/trigger/with_model_and_context.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/zoho-recruiter-daily-summarizer/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `tests/fixtures/fleetbundle/zoho-sprint-daily-summarizer/TRIGGER.md` | M227_001 cutover | complete/edit later |
| `ui/packages/app/app/(auth)/layout.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/AddFleetDialog.upload.test.tsx` | M227_001 cutover | complete/edit later |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/SkillEditor.test.tsx` | M227_001 cutover | complete/edit later |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/SkillEditor.tsx` | M227_001 cutover | complete/edit later |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` | M227_001 cutover | complete/edit later |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/actions.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/defaults/page.tsx` | M227_001 cutover | DELETE later |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/security/page.tsx` | M227_001 cutover | DELETE later |
| `ui/packages/app/app/favicon.ico` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/app/icon.svg` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/app/layout.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/components/domain/chat/DraftThread.tsx` | M227_001 cutover | CREATE later |
| `ui/packages/app/components/domain/chat/authoring-runtime.ts` | M227_001 cutover | CREATE later |
| `ui/packages/app/components/domain/fleet-library/LibrarySourceTabs.test.tsx` | M227_001 cutover | complete/edit later |
| `ui/packages/app/components/domain/fleet-library/LibrarySourceTabs.tsx` | M227_001 cutover | complete/edit later |
| `ui/packages/app/components/domain/fleet-library/bundle-files.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/components/domain/fleet-library/library-source-form.test.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/components/domain/fleet-library/library-source-form.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/components/domain/island-dynamic/intent-shell-controls.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/components/domain/useFleetMessageDelivery.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/components/layout/GettingStartedWidget.refresh.test.tsx` | M227_001 cutover | DELETE later |
| `ui/packages/app/components/layout/GettingStartedWidget.test.tsx` | M227_001 cutover | DELETE later |
| `ui/packages/app/components/layout/GettingStartedWidget.tsx` | M227_001 cutover | DELETE later |
| `ui/packages/app/components/layout/MobileNavigationDialog.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/components/layout/ShellControls.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/components/layout/SidebarNavigation.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/components/layout/SidebarNavigationSections.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/components/layout/sidebar-navigation-data.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/lib/analytics/events.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/lib/api/drafts.ts` | M227_001 cutover | CREATE later |
| `ui/packages/app/lib/api/fleet-library.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/lib/api/fleets.test.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/lib/api/fleets.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/lib/api/library-types.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/lib/types.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/public/apple-touch-icon.png` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/public/brand.svg` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/tests/add-template-dialog.test.tsx` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/app-components.test.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/tests/app-pages.test.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/tests/app-shell-navigation.test.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/tests/e2e-teardown-sweep.test.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/e2e/acceptance/fixtures/grants.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/e2e/acceptance/fixtures/install-ui.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/e2e/acceptance/fixtures/seed.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/e2e/acceptance/fixtures/teardown.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/e2e/acceptance/fleet-webhook-delivery.spec.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/e2e/acceptance/install-fleet-cli.spec.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/e2e/acceptance/measured-layout.spec.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/e2e/acceptance/runner-detail.spec.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/e2e/acceptance/workspace-chat.spec.ts` | M227_001 cutover | CREATE later |
| `ui/packages/app/tests/e2e/acceptance/workspace-library.spec.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/e2e/wake-pulse.spec.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/tests/fleets-api-client.test.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/fleets-routes/harness.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/app/tests/library-entries-client.test.ts` | M227_001 cutover | complete/edit later |
| `ui/packages/design-system/assets/brand.svg` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/assets/favicon.ico` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/assets/favicon.svg` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/assets/logo-dark.svg` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/assets/logo-light.svg` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/design-system/BrandMark.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/design-system/BrandMark.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/design-system/Terminal.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/design-system/Terminal.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/design-system/index.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/index.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/theme.css` | M227_002 carried batch | complete/edit later |
| `ui/packages/design-system/src/tokens.css` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/index.html` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/public/apple-touch-icon.png` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/public/favicon.ico` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/public/favicon.svg` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/public/social.png` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/App.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/App.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/analytics/landing-clicks.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/analytics/posthog.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/analytics/posthog.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/AdoptionSections.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/AgentIllustration.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/CTABlock.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/CTABlock.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/CliQuickstart.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/FAQ.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/FAQ.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/FleetPreview.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/Footer.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/Footer.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/Hero.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/Hero.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/HowItWorks.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/HowItWorks.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/IncidentFlow.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/JobComposer.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/JobSuggestions.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/PrebuiltFleets.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/PrebuiltFleets.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/Pricing.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/Pricing.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/SignalField.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/StoryMessage.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/StoryMessage.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/motion.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/use-chat-scroll.test.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/use-chat-scroll.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/use-motion-environment.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/components/use-scene-motion.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/config.test.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/config.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/entrance.css` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/incident-flow.css` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/lib/chat-story.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/lib/example-jobs.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/lib/llms-text.test.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/lib/llms-text.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/lib/marketing-copy.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/marketing-spec.test.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/pages/Home.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/pages/Home.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/pages/Terms.test.tsx` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/styles.css` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/test-setup.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/src/workbench.css` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/tests/e2e/content-accessibility.spec.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/tests/e2e/home.spec.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/tests/e2e/humans-fleets-links.spec.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/tests/e2e/navigation.spec.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/website/tests/e2e/smoke.spec.ts` | M227_002 carried batch | complete/edit later |
| `ui/packages/app/tests/analytics-events.test.ts` | M227_001 cutover | EDIT later: existing event catalog privacy proof |
| `rustd/crates/afd_authoring/src/error/mod.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/src/error/raise.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/src/model/mod.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/src/model/capability.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/src/model/dispatch.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/src/model/receipt/` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/src/lease/claim.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/src/lease/renew.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/src/lease/admit.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/src/candidate/edit.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/src/sql/draft.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/src/sql/attempt.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_api_runner/src/handler/runner/authoring/model_call.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_api_runner/src/handler/runner/authoring/activity.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_api_runner/src/handler/runner/authoring/renew.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_api_runner/src/handler/runner/authoring/report.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afr_agent/src/authoring/context.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afr_agent/src/authoring/proposal.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afr_supervisor/src/client/authoring.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afr_supervisor/src/report_spool/authoring.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_wire/src/authoring/` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_api/tests/authoring/mod.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_api/tests/authoring/model_access.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_api/tests/authoring/capacity.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_api/tests/authoring/candidate.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/tests/draft/proposals/mod.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/tests/draft/proposals/revisions.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/tests/draft/model_calls/mod.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/tests/draft/model_calls/arithmetic.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/tests/draft/mod.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/tests/draft/fixtures.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_authoring/tests/draft/recovery.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afr_supervisor/src/report_spool/tests/authoring.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `ui/packages/app/tests/e2e/acceptance/fixtures/build-preflight.test.ts` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_events/tests/integration_queued_history.rs` | M227_001 cutover | CREATE later: Astra review correction |
| `rustd/crates/afd_api_runner/src/openapi.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afd_api_runner/src/handler/runner/mod.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afd_fleet/src/lease/renew.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afd_events/src/history/statement/queued.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afd_events/src/history/queued.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afr_supervisor/Cargo.toml` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afr_supervisor/src/client.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afr_supervisor/src/report_spool.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afr_providers/src/connect.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afr_providers/src/transport.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afr_providers/src/retry.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `ui/packages/app/tests/e2e/acceptance/global-setup.ts` | M227_001 cutover | EDIT later: Astra review correction |
| `ui/packages/app/tests/e2e/acceptance/fixtures/preflight.ts` | M227_001 cutover | EDIT later: Astra review correction |
| `ui/packages/app/tests/e2e/acceptance/fixtures/cli-runner.ts` | M227_001 cutover | EDIT later: Astra review correction |
| `cli/test/acceptance/global-setup.ts` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afd_admission/tests/receipt_faults.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afr_supervisor/src/report_spool/tests.rs` | M227_001 cutover | EDIT later: Astra review correction |
| `rustd/crates/afd_http/Cargo.toml` | M227_001 cutover | EDIT later: domain service wiring/error conformance |
| `rustd/crates/afd_api_runner/Cargo.toml` | M227_001 cutover | EDIT later: domain service wiring/error conformance |
| `rustd/crates/afd_api_tenant/Cargo.toml` | M227_001 cutover | EDIT later: domain service wiring/error conformance |
| `rustd/crates/agentsfleetd/Cargo.toml` | M227_001 cutover | EDIT later: domain service wiring/error conformance |
| `rustd/crates/afd_http/src/services/mod.rs` | M227_001 cutover | EDIT later: domain service wiring/error conformance |
| `rustd/crates/afd_http/src/services/contract.rs` | M227_001 cutover | EDIT later: domain service wiring/error conformance |
| `rustd/crates/agentsfleetd/src/plane.rs` | M227_001 cutover | EDIT later: domain service wiring/error conformance |
| `rustd/crates/agentsfleetd/src/plane/services.rs` | M227_001 cutover | EDIT later: domain service wiring/error conformance |
| `docs/RUST_ERROR_STANDARD.md` | M227_001 cutover | EDIT later: domain service wiring/error conformance |
| `rustd/crates/afd_http/src/services/authoring.rs` | M227_001 cutover | CREATE later: domain service wiring/error conformance |

## API-trigger follow-up

This app-only follow-up removes the retired trigger, validates webhook edits, and reads webhook addresses from Save.
The task-start comparison is `/tmp/m227-api-trigger/change.diff`; the exact list has 20 app files: nine production modules and eleven tests/helpers/fixtures.
One source file was added; no files were deleted. `events.spec.ts`, Rust, package files and lockfiles were left untouched.

| File | Action |
|---|---|
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/TriggerPanel.test.tsx` | EDIT |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/TriggerPanel.tsx` | EDIT |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/last-delivery.test.ts` | EDIT |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/last-delivery.ts` | EDIT |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/trigger-key.ts` | EDIT |
| `ui/packages/app/components/domain/chat/DefinitionEditor.tsx` | EDIT |
| `ui/packages/app/components/domain/chat/chat-defaults.ts` | EDIT |
| `ui/packages/app/lib/api/draft-triggers.ts` | CREATE |
| `ui/packages/app/lib/api/draft-types.ts` | EDIT |
| `ui/packages/app/lib/api/fleet-detail.ts` | EDIT |
| `ui/packages/app/lib/api/fleets.test.ts` | EDIT |
| `ui/packages/app/lib/types.ts` | EDIT |
| `ui/packages/app/tests/draft-definition.test.ts` | EDIT |
| `ui/packages/app/tests/e2e/acceptance/fixtures/definition.ts` | EDIT |
| `ui/packages/app/tests/e2e/acceptance/fixtures/grants.ts` | EDIT |
| `ui/packages/app/tests/e2e/acceptance/fleet-webhook-delivery.spec.ts` | EDIT |
| `ui/packages/app/tests/e2e/acceptance/install-fleet-cli.spec.ts` | EDIT |
| `ui/packages/app/tests/fleets-routes/detail-lifecycle.test.ts` | EDIT |
| `ui/packages/app/tests/helpers/draft-fixtures.ts` | EDIT |
| `ui/packages/app/tests/workspace-chat/editor.test.tsx` | EDIT |

## Errors

The inventory is a discovery map, not a passing test or implemented route list. Re-run discovery before implementation and update the owner spec for extra touched files. Input-only trigger_markdown, negative removed-route assertions, planning removal text, historical specs and frozen schemas are deliberate surviving mentions; live persisted/writable fields and aliases are not.

## Related pages

- [Owning spec](../../v2/done/M227_001_P0_API_CLI_DOCS_UI_WORKSPACE_CHAT_CUTOVER.md)
- [Folded batch](../../v2/done/M227_002_P1_CLI_DOCS_UI_WEBSITE_BRANDING_BATCH.md)

## Active implementation additions

| Path | Action | Evidence or purpose |
|---|---|---|
| `rustd/crates/afd_fleet_runtime/src/definition/` | IMPLEMENTED | Native validated Definition, digest and atomic edits; 13 focused tests in the 35-test runtime suite |
| `rustd/crates/afd_fleet_runtime/src/optional.rs` | IMPLEMENTED | Explicit-null refusal for optional native fields |
| `rustd/crates/afd_fleet_runtime/src/definition/portable.rs` | REMOVED | New adapter removed after chat-only owner decision; legacy callers still await replacement |
| `rustd/crates/afd_fleet_runtime/tests/definition/`, `tests/frontmatter/`, `tests/errors/` | ORGANIZED | Crate-root suites grouped by behavior with mod.rs entrypoints |
| `schema/935_authoring_drafts.sql` through `schema/940_authoring_accounting.sql` | CREATE | Separate private authoring tables and independently retained accounting |
| `rustd/crates/afd_runner/src/capacity/`, `tests/capacity/` | CREATE | Shared host lock, fresh database clock and mixed-kind capacity proofs |
| `rustd/crates/afd_fleet/src/lease/issue.rs`, `src/lease/renew.rs`, `src/lease/sql/renew.rs`, `src/error/` | EDIT | Runtime lease issue/renewal use shared host guard; expired leases cannot revive |
| `rustd/crates/afd_db/src/migration.rs`, `tests/migrations.rs` | EDIT | Register additive authoring migrations; schema registration proofs |
| `docs/reviews/workspace-setup/checks.md` | CREATE / UPDATE | Required-test ledger and actual focused command results |
| `/Users/kishore/Projects/orly/docs/REST_API_DESIGN_GUIDELINES.md`, `.orly/docs/REST_API_DESIGN_GUIDELINES.md`, `.orly/orly.json` | AMENDED / MATERIALIZED | Explicit owning-source exception authorized before conversation handlers |


## Served foundations and focused modules

These paths exist in the working tree; the check ledger supplies behavioral evidence. Claim, renewal, terminal settlement and runner execution are still required.

| Paths | Current work |
|---|---|
| `afd_authoring/src/{access.rs,draft/,message/,candidate/,privacy/,replay/,types/,sql/}` | Creator-private draft lifecycle, immutable candidate proposals and replay |
| `afd_authoring/src/lease/{mod,guard,sql}.rs` | Runner/subject/model/time publication and invocation guards |
| `afd_authoring/src/model/{budget,capability,dispatch,prepare,sql}.rs`, `provider/`, `receipt/` | Commit-before-send model dispatch, guarded transport, token/money bounds and late/unknown receipt handling |
| `afd_wire/src/authoring/{mod,activity,model}.rs`, `afd_api_wire/src/authoring/` | Strict shared wire shapes; no provider override or key delivery |
| `afd_api_runner/src/handler/runner/authoring/{mod,activity,model_call}.rs` | Served own-lease activity and model calls |
| `afd_api_tenant/src/handler/draft/` | Served creator-only draft/message operations |
| `afd_credential/src/provider/registered/` | Owner metadata and transaction-scoped pinned key resolution |
| `afd_authoring/tests/draft/model_calls/`, `tests/draft/proposals/`, `afd_api/tests/authoring/models/` | Real datastore/vault/router proofs with only the physical model transport stubbed |
| `afd_http/src/services/contract.rs`, `agentsfleetd/src/plane{.rs,/}`, `agentsfleetd/src/serve/{clients,runtime}.rs` | Production service and outbound client wiring |
| `afd_api/tests/harness/{mod,services,fleet,stubs_provider}.rs`, `afd_db/src/test_util/rows.rs` | Shared harness and panic-safe cleanup of owned rows |
| `schema/941_native_setup_revisions.sql`, `schema/942_authoring_replay_receipts.sql` | Native revision identities and content-free private-operation tombstones |

Rust paths in this table are relative to `rustd/crates/`; schema paths are relative to the repository root. All growing suites use crate-root behavior groups with `mod.rs` entrypoints.

## App API alignment completion

The final app alignment delta contains 60 source/test/fixture files: 14 production modules and 46 test/helper/fixture files. Two test files are new; no files were deleted. Generated Vitest cache is excluded. `make test-unit-app` reports 369 files and 3401 tests passed (`/tmp/m227-app-wire/unit-app-final.log`). The [completion review](ui-completion-review.md) records command results and limits.

### Production modules

| File | Action |
|---|---|
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.tsx` | EDIT |
| `ui/packages/app/app/(dashboard)/admin/runners/actions.ts` | EDIT |
| `ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.tsx` | EDIT |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeyList.tsx` | EDIT |
| `ui/packages/app/lib/api/api_keys.ts` | EDIT |
| `ui/packages/app/lib/api/events.ts` | EDIT |
| `ui/packages/app/lib/api/fleet-library.ts` | EDIT |
| `ui/packages/app/lib/api/runners-types.ts` | EDIT |
| `ui/packages/app/lib/api/runners.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-frames.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-reply-frames.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-reply-registry.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-tool-frames.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-tool-trace.ts` | EDIT |

### Tests, helpers and fixtures

| File | Action |
|---|---|
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.actions.test.tsx` | EDIT |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.selftest.test.tsx` | EDIT |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.test.tsx` | EDIT |
| `ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.test.tsx` | EDIT |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeyList.test.tsx` | EDIT |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/ChatView.test.tsx` | EDIT |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.test.tsx` | EDIT |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/actions.test.ts` | EDIT |
| `ui/packages/app/components/domain/useFleetRunSummary.test.tsx` | EDIT |
| `ui/packages/app/lib/api/api_keys.test.ts` | EDIT |
| `ui/packages/app/lib/api/events.test.ts` | EDIT |
| `ui/packages/app/lib/api/fleet-library.test.ts` | EDIT |
| `ui/packages/app/lib/api/runners-edit.test.ts` | CREATE |
| `ui/packages/app/lib/api/runners.test.ts` | EDIT |
| `ui/packages/app/lib/events/event-row-grouping.test.ts` | EDIT |
| `ui/packages/app/lib/events/run-summary.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-facts.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-frames.figures.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-frames.live.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-frames.settle.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-frames.tools.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-held.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-registry-optimistic.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-registry.facts.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-registry.final-read.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-registry.held.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-registry.reply.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-registry.stall.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-registry.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/fleet-stream-required.test.ts` | CREATE |
| `ui/packages/app/lib/streaming/fleet-stream-tool-trace.test.ts` | EDIT |
| `ui/packages/app/lib/streaming/workspace-store.test.ts` | EDIT |
| `ui/packages/app/tests/api-keys-components.test.ts` | EDIT |
| `ui/packages/app/tests/api-keys-page.test.ts` | EDIT |
| `ui/packages/app/tests/e2e/acceptance/fleet-stream-transport.spec.ts` | EDIT |
| `ui/packages/app/tests/events-components.test.ts` | EDIT |
| `ui/packages/app/tests/fleet-thread/harness.ts` | EDIT |
| `ui/packages/app/tests/fleets-routes/detail-header.test.ts` | EDIT |
| `ui/packages/app/tests/helpers/event-details-dialog-fixtures.tsx` | EDIT |
| `ui/packages/app/tests/helpers/event-summary-fixtures.ts` | EDIT |
| `ui/packages/app/tests/helpers/fleet-stream-fixtures.ts` | EDIT |
| `ui/packages/app/tests/helpers/fleet-stream-hook-fixtures.ts` | EDIT |
| `ui/packages/app/tests/helpers/fleet-stream-registry-fixtures.ts` | EDIT |
| `ui/packages/app/tests/helpers/workspace-store-harness.ts` | EDIT |
| `ui/packages/app/tests/runners-actions.test.ts` | EDIT |
| `ui/packages/app/tests/use-event-detail.test.ts` | EDIT |
