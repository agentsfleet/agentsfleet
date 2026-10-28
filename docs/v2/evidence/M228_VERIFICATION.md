# M228 verification record

Recorded Oct 28, 2026. Branch: `codex/m228-webhooks-connectors`.
Comparison revision: `289d226856e65d459f1b24de7b60519f897ef2d3`.

## Environment and measurement

Baseline and feature used separate Cargo build directories and separate disposable Docker Compose projects, `m228-baseline` and `m228-feature`. Each project supplied its own Postgres database and four-node Dragonfly cluster. Development and test debug information and incremental compilation were disabled to fit the available disk. Test selections, assertions and timeouts were unchanged.

The exact baseline checkout was `/private/tmp/m228-baseline-wtnk10xn`. The feature checkout was the managed M228 worktree. Counts below are executions, including repeated Cargo runs, rather than unique test definitions.

## Exact comparison revision

| Command | Observed result | Local evidence |
|---|---|---|
| `make test-unit-all` | Pass. Rust: 4,782 passed, 0 failed, 1,450 ignored. Application: 453 files / 4,462 tests. Website: 20 files / 133 tests. Command-line interface: 1,967 passed / 16 skipped. Design: 68 files / 522 tests. All package coverage gates passed. | `/tmp/m228-baseline-unit-isolated.log` |
| `make test-integration-rustd` | Fail. 627 passed / 1 failed before the lane stopped. The authoring suite reported 261 passed / 1 failed / 2 filtered. Its concurrent stopped-history case exhausted the existing five-second API pool budget. Later binaries did not run. | `/tmp/m228-baseline-integration-isolated.log` |

The comparison integration failure was `draft::admission::history::stopped_history_uses_sequence_index_and_concurrent_sends_keep_unique_sequences`, with `DbPoolCapacity`. The same authoring suite passed all 262 cases on the feature attempt below. Neither result establishes that every integration failure is pre-existing.

## Feature verification

| Command | Observed result | Local evidence |
|---|---|---|
| `make harness-verify` | Pass on the staged snapshot: 80 source files, no unified-symbol violations; secret-scanner configuration, design tokens, spec template, logging, Rust error discipline and milestone references clean. Final staging requires revalidation. | `/tmp/m228-final-harness.log` |
| `make test-unit-all` | Pass. Rust: 4,793 passed / 0 failed / 1,451 ignored. Application: 454 files / 4,489 tests. Website: 20 files / 133 tests. Command-line interface: 1,967 passed / 16 skipped. Design: 68 files / 522 tests. All package coverage gates passed. Subsequent mechanical Rust test lint repairs require final revalidation. | `/tmp/m228-final-unit-retry.log` |
| `make lint-all` | Pass: `All lint checks passed`. Earlier deterministic Rust test lint issues were repaired without suppressions. | `/tmp/m228-final-lint-retry9.log` |
| `make test-integration-rustd` | First complete feature attempt failed: 659 passed / 1 failed before stopping. Webhook ingress: 65 passed; authoring: 262 passed. Billing report suite: 8 passed / 1 failed at its fourth connection-cut assertion. Later binaries and the exclusive lane did not run. | `/tmp/m228-final-integration-isolated2.log` |
| `make check-version` | Pass: declared release versions agree at 0.71.0. | `/tmp/m228-final-version.log` |
| OpenAPI regeneration | 107 paths; removed Svix route absent; generated output byte-identical to `public/openapi.json`. | `/tmp/m228-openapi-regenerated.json` |

The billing failure is `failures::connection_loss_at_each_report_statement_keeps_money_and_retry_intact`. It observed zero proxy cuts where one was expected at boundary four. Neither `afd_billing` nor `afd_db` has a source or test change in this workstream. Ten complete report-suite repetitions on the exact baseline and ten on the feature each passed all nine tests; this does not prove the earlier failure is pre-existing. A full run without competing heavy test jobs is in progress to test resource contention. The existing 400 ms budget and every assertion remain unchanged. Diagnostic logs: `/tmp/m228-billing-baseline-diagnostic.log` and `/tmp/m228-billing-feature-diagnostic.log`.

## Functional quality assurance

The completed local probe verdict is **pass** for application recovery and the two required router probes:

- `make test-unit-app`: 454 files, 4,489 passed.
- `cd rustd && cargo test --all-features -p afd_api --test ingress_plane a_declared_verifier_wakes`: 1 passed, 0 failed.
- `cd rustd && cargo test --all-features -p afd_api --test ingress_plane webhook_declared_svix`: 9 passed, 0 failed.

The initial five-minute smoke capture expired and is superseded by the complete required application run. Its deadline was not reset; the subsequent commands were finite required revalidations with individual command guards. Completed probe time totals 423,563 ms. Replay evidence, guarded output and the functional report are retained under `/private/tmp/m228-qa-review`.

- [checkpoint 003](/private/tmp/m228-qa-review/exploration-003.json) — application recovery result informed custom-source verification.
- [checkpoint 004](/private/tmp/m228-qa-review/exploration-004.json) — custom-source result informed the Svix identity and replay check.

These probes use test doubles and scripted provider replies. They do not establish deployed provider behavior or visual appearance.

## Diagnosis and deployment limits

Jira's live trace located failure after token exchange and before the persisted grant. A synthetic oversized initial token reproduces the vault-size refusal. The updated grant stores the refresh credential; the broker already obtains a fresh access token for use. A retry against deployed changed code is still required to confirm this explains the reported account's failure.

Slack's registered browser redirect was saved and reloaded as `https://app-dev.agentsfleet.net/api/connectors/slack/callback`, without a trailing slash. No Slack authorization round trip against changed code is claimed.

The workspace catalogue returns registered applications, configured state, saved authorization and public labels in one response, using two batched storage reads. It does not probe remote provider health. Other dashboard layout dependencies can still delay navigation. The shared chat Apps menu is outside this reliability change.

## Review and companion documentation

The full native adversarial implementation pass reported no production findings. Test and fixture bodies were reviewed as summaries in that pass; parent review read the changed assertions. Subsequent mechanical test lint repairs require a final zero-edit review. Applicable Rust guidelines included M-STRONG-TYPES, M-STRONG-TYPES-GUARD, M-FROM-ERROR and M-MOCKABLE-SYSCALLS, with the repository's named error-shell divergence from M-ERRORS-CANONICAL-STRUCTS. No outside Claude review was enabled.

Companion documentation: [docs pull request 231](https://github.com/agentsfleet/docs/pull/231), revision `b7261012ebfb3aa86109ce0a6258a70ca28c77b5`. Documentation tests: 25 passed. Lint passes against the proposed schema. The Greptile credential-shape finding is fixed and its thread resolved. Remote documentation lint still reads the old API `main` schema and reports the retired Svix operation missing from navigation; that dependency blocks the authorized conditional merge.

## Discarded setup attempts

An initial baseline attempt shared a Cargo output directory and failed compilation; it is not a baseline measurement. An initial feature attempt lost its default Compose containers and reported pool timeouts. The first dedicated feature attempt met a temporary Postgres initialization server before final startup. Those setup attempts are excluded from behavioral conclusions. No harness, timeout, test selection or scanner suppression was changed to hide them.
