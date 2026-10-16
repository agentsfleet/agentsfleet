# Workspace setup implementation checks

This ledger records focused checks in the existing `feat/website-completion` worktree. Repository and browser acceptance gates remain pending.
The owner authorized a checkpoint commit/push and an API/UI split between two agents on Oct 15, 2026.
That instruction supersedes the earlier commit deferral; implementation and final proofs remain unfinished.

Owner timing instruction, Oct 15, 2026: “I think the focussed verification can move to the end prior to the PR. Can you ensure the code is completed first?”
Complete implementation before running further focused verification. Run focused and repository checks before the Pull Request (PR).
Owner Section cadence: “you do cargo fmt, cargo check after every section is done, donot do cargo fmt, cargo check on every file change or dimenstion completeion”.
Run formatting and compile checks once per completed Section; keep behavioral verification at the final boundary.

## Required tests

| Changed behavior or boundary | Required proof | State and evidence |
|---|---|---|
| Native Definition parse, normalize and serialize | All supported metadata/policy fields survive storage; wrong shapes/unknown keys fail | Focused runtime suite passed all 35 tests after the bounds repairs. |
| Save validation | Empty instructions remain editable and fail Save/publication | Covered by `test_definition_empty_editing_is_valid_but_save_is_blocked`. |
| Exact budget units | Positive boundaries; null/numeric/dollar/sign/fraction/overflow refusals | Explicit monthly null failure reproduced, repaired and passed in the 35-test runtime suite. |
| Atomic edits | Valid changes preserve unrelated policy; invalid paths, Unicode boundaries and overlaps never mutate the base | Covered by focused Definition suite. |
| Replacement byte budget | Count every config value and instruction replacement before applying overwrites | Oversized overwritten values failure reproduced, repaired and passed in the runtime suite. |
| Reviewed content identity | Key order and equivalent numbers normalize; instruction, model, budget and array changes alter digest | Covered by focused native digest tests. |
| Removed Definition import/export and support manifest | No public adapter or manifest type remains; native serialization replaces portable tests | Code removed; final cross-layer orphan sweep pending. |
| Creator privacy and message replay | Real router and datastore: creator/workspace/revocation/cursor/stream, same-key replay and changed body | Draft/message operations and creator/cursor/replay refusals covered by the 35-test authoring suite and 8 focused router checks. Private stream and Review have focused proofs recorded below; browser acceptance remains pending. |
| Scoped model dispatch and proposal receipts | Every physical dispatch reauthorizes; immutable candidate base and replay receipt; stale/forged fields fail | Implemented daemon dispatch and native proposal activity. Focused checks cover capability/runner/attempt binding, live revocation, contention, exact replay, unknown usage, late Stop/deletion, incomplete proposals and immutable revision receipts. Authoring renewal and report settlement are also served. Daemon claim polling and runner execution are implemented; coordinated browser acceptance remains pending. |
| Renewal, capacity and money | Database-time expiry with shared host lock; wallet ordering; unknown obligations retained | Runtime renewal and shared host census implemented; two renewal checks and three capacity checks pass. Authoring renewal, wallet bounds and terminal accounting now have focused coverage. Claim admission checks are in progress. |
| Admission recovery | No-receipt admissions visible; append/receipt retry produces one event | Pending reads include durable receiptless admissions; five queued-message checks pass, including replay and delivery preserving one event identity. Resumable creation and initial admission have focused domain and router proofs; matching-build acceptance remains pending. |
| Terminal chat | Start/resume/send/review/Save/publication; partial setup exits nonzero; removed file commands fail | Pending implementation. |
| Library/setup revisions | Explicit publication; pinned instance; Improve/Apply; concurrent drift/replay and deletion | Native publication, receipt reuse, owned history, Apply and Restore have focused proofs below. Platform slug publication, operator cutover and removed-adapter sweep remain open. |
| Browser acceptance | Matching app/daemon/runner/CLI builds verified before fixture writes | Pending client implementation and matching-build acceptance. The independent schedule prerequisite has passed focused checks below. |

## Credential and trigger readiness required-test ledger

| Changed behavior | Required proof | Evidence |
|---|---|---|
| Mention purpose and provider binding | Generic objects and unknown providers cannot satisfy a Mention; Slack requires its bound handle | `afd_api/tests/authoring/readiness/mentions.rs`; generic Slack object reproduced as `ready` before repair |
| Slack handle shape | Missing, wrong-type or empty team/token and wrong bound name refuse | Real-router malformed-handle cases pass; focused handle unit cases pending |
| Routing install and scopes | Exact workspace/provider/account; missing install, insufficient/revoked scopes and changed install identity affect readiness/digest | Real-router missing/scopes/revocation cases pass; identity isolation cases pending |
| Keyed webhook purpose | Require the nonempty `webhook_secret` field and a supported source; inbound overrides need no outbound App handle | `afd_api/tests/authoring/readiness/webhooks.rs`; generic object reproduced as `ready` before repair |
| Raw Svix signing reference | Inspect `signature.secret_ref`; accept native raw format, refuse malformed or JSON-wrapped values; expose no secret | Native encryption fixture and real-router cases pending |
| Multiple uses of one name | One row must satisfy every declared use; signing-only checks do not add outbound connection/grant requirements | Real-router shared-name and inbound-only cases pending |
| Connection versus fleet approval | Refresh-only connections need no routing install and do not imply fleet grant approval | Real-router Jira case passes with credential/connection ready and grant missing |

The first focused readiness run used the real tenant router and local test Postgres.
`cargo test -p afd_api --all-features --test tenant_plane authoring::readiness -- --ignored` reported four passed and two failed.
The failures returned `ready` for a generic Slack object and a webhook object without its signing field.
The production repair is written; no passing rerun has been observed yet.

## Recorded commands

- `cargo test -p afd_fleet_runtime --test runtime_suite definition`: 12 passed, 22 filtered, after removal of the new portable adapter.
- `cargo test -p afd_fleet_runtime --test runtime_suite test_nanos_rejects_numbers_null_missing_daily_and_legacy_keys`: failed because `monthly_nanos:null` was accepted.
- `cargo test -p afd_fleet_runtime --test runtime_suite test_definition_replacement_budget`: failed because overwritten replacement text did not count toward the byte bound.
- `cargo fmt -p afd_fleet_runtime -p afd_billing`: passed.
- `cargo test -p afd_fleet_runtime --test runtime_suite`: 35 passed, 0 failed after both regression repairs.
- `make KEEP_TEST_STATE=1 _migrate-test-db`: passed; applied the registered migrations including 935–940 against the local test stack. Slots 933–934 remain reserved for the independent schedule work.
- `cargo test -p afd_runner --features test-util --test runner_suite capacity -- --ignored`: 3 passed, 20 filtered. The first run exposed missing runtime receipt fixture data; repaired before this passing run.
- `cargo test -p afd_events --features test-util --test events_suite queued -- --ignored`: 5 passed, 39 filtered, against Postgres and Dragonfly.
- `cargo test -p afd_fleet --features test-util --test fleet_suite lease_renewal -- --ignored`: 2 passed, 232 filtered. Covers each expired deadline and expiry during an observed database row-lock wait; no fixed-delay concurrency assertion. The first fixture lacked a capability heartbeat; repaired through the normal heartbeat path.
- `cargo fmt -p afd_runner -p afd_fleet -p afd_events -p afd_db`: passed.


- `cargo test -p afd_authoring --features test-util --test authoring_suite -- --ignored`: 29 passed, 0 failed, against live Postgres with real vault resolution and a provider transport stub. Attempts are directly seeded; this does not prove claim admission.
- `cargo test -p afd_api --features test-util,openapi --test tenant_plane authoring -- --ignored`: 8 passed, 356 filtered, against the real router and Postgres. Only the physical provider transport is stubbed; no paid model call was made.
- `cargo check -p afd_api_runner --features openapi`: passed after the activity/model-call route additions.
- `cargo clippy -p afd_authoring --all-targets --features test-util`: passed after correcting the new tests' reference-counted pointer clone style. This is a package check, not repository lint.

- `make KEEP_TEST_STATE=1 _migrate-test-db`: passed after registering migration 943; existing fixture state was preserved. The migration adds observed runtime and separate immutable accounting/private answer receipts.
- `cargo test -p afd_authoring --features test-util --test authoring_suite -- --include-ignored`: 35 passed after the renewal/report additions and helper refactors. Covers 34 real Postgres cases plus one strict wire case.
- `cargo clippy -p afd_authoring --all-targets --features test-util`: passed for the renewal/report implementation before admission was added.
- `cargo test -p afd_core --test core_suite error_code`: 17 passed, 43 filtered.
- `cargo test -p afd_fleet_runtime --test runtime_suite`: 35 passed on the current native Definition implementation. Legacy frontmatter checks remain until their runtime callers move.
- The new six admission checks passed once against Postgres. A subsequent capability-format correction is under recheck; this result does not yet prove the current claim payload.

The current authoring adapter accepts approved OpenAI stateless text models in the GPT-4o and GPT-4.1 families when the local tokenizer, owner metadata, context window and pinned integer prices are available. Audio, realtime, search, reasoning continuation and custom destinations are refused. Extending eligible transports remains implementation work; unsupported models never fall back to another model or key.

The conversation REST-guide exception was amended in its authorized owning source, `/Users/kishore/Projects/orly/docs/REST_API_DESIGN_GUIDELINES.md`. Orly's renderer and installation transaction narrowly materialized only the guide and its recorded digest into this worktree. The installed guide matches the owning render; hooks were not changed.

Full repository suites remain deferred until the workspace chat flow is complete, as requested. Package checks prove only their focused behavior.

## Terminal chat required-test ledger

| Changed unit | Required behavior or failure proof | Status |
|---|---|---|
| Native response schemas and transport | Reject malformed setup, acceptance and message replies; workspace and credential stay scoped | Pending focused tests |
| Retained request | Reuse exact key/body after lost answer; refuse changed intent; clear known refusals; late completions cannot clear another operation | Pending focused tests |
| Draft session | Atomic first message; resume send; fresh idle tag for model/name; Stop watermark; clear private state on access loss and delete | Pending focused tests |
| Setup session | Explicit Save/publication/Apply; exact reviewed digest and setup tag; resume partial Save with original key; no implicit publication | Pending focused tests |
| Stream and durable follow | Byte transport wakes paged reads; reconnect and disconnect reconcile; cancellation releases body; terminal failure is visible | Pending focused tests |
| Terminal actions and command | Review before mutations; script digest check; no file input; partial result exits nonzero; private buffers clear at invocation end | Pending focused tests |

`bun run typecheck` in `cli/` passed after adding the terminal command. This checks types only; no terminal acceptance claim is made.

## Save, Apply, schedules, and policy checks

All commands below ran in this worktree against the local test dependencies. They do not satisfy repository verification gates.

| Command | Observed result | Scope |
|---|---|---|
| `make KEEP_TEST_STATE=1 _migrate-test-db` | Applied migration 948; 87 migrations already current | Native inherited recipe storage |
| `cargo test -p afd_authoring --all-features --test authoring_suite draft::library -- --include-ignored` | 6 passed, 0 failed | Explicit publication, revision replay, visibility, 100 concurrent publication requests |
| `cargo test -p afd_api --all-features --test tenant_plane authoring::library -- --include-ignored` | 3 passed, 0 failed | Served Library publication/history, deletion and cursor audience |
| `cargo test -p afd_api --all-features --test tenant_plane integration_fleet_schedules_ownership -- --include-ignored` | 2 passed, 0 failed | Independent schedule ownership prerequisite |
| `cargo test -p afd_cron --all-features --test integration_sync_identity -- --include-ignored --nocapture` | 3 passed, 0 failed | Real scheduler identity, lost registration answer and stale pause versus fresh resume |
| `cargo test -p afd_api --all-features --test tenant_plane authoring::saving -- --include-ignored` | 10 passed, 0 failed, 368 filtered; 0.88 seconds | Current Save, Apply, inherited schedules and policy admission behavior |
| `cargo check -p afd_api --all-targets --all-features` | Finished successfully in 12.64 seconds | Current backend types after schedule module split |

The live scheduler checks used the local QStash service. They did not reach production or dispatch paid model calls.

### Required-test ledger for the current slice

| Changed boundary | Required proof | Evidence |
|---|---|---|
| Apply setup pointer and immutable revisions | Advance one setup; preserve original; close only creator draft; replay original identity | `authoring/saving/revisions/apply.rs` |
| Setup version versus fleet resource edits | Rename and Stop preserve setup; resume checks removed model | `authoring/saving/revisions/refusals.rs` |
| Stale Apply | Old tag returns 412; new tag cannot disguise an old base; changed replay body conflicts | `authoring/saving/revisions/refusals.rs` |
| Recipe identity after registration | Scheduler key replacement cannot allocate another inherited row or lose its overlay | `authoring/saving/revisions/schedules/restore.rs` |
| Restore and setup concurrency | Overlay survives Apply; old Restore tag returns 412 with current tag; fresh Restore clears overlay against revised base | `authoring/saving/revisions/schedules/restore.rs` |
| Conditional schedule input | Missing tag, mixed independent fields, empty overlay, unknown/null fields and invalid cron write nothing | `authoring/saving/revisions/schedules/refusals.rs` |
| Live scheduler hold versus Apply | Refuse with syncing state; roll back the complete setup and schedule change | `authoring/saving/revisions/schedules/refusals.rs` |
| Policy semantics versus new admission | Old accepted identity replays; new messages fail under an unsupported marker | `authoring/saving/revisions/policy.rs` |
| Independent schedule Restore and foreign schedule edit | Independent Restore fails; foreign rows remain hidden | Required focused cases remain to be added |
| Full client flow and coverage | Browser and terminal create, review, Save, resume, publication and Improve | Pending client completion; no coverage percentage claimed |
| Initial run refused before admission | Reload and explicit Resume retry the original derived event identity after capacity recovers; repeated Resume cannot duplicate it | Regression check added; reproduction and repair pending |

The policy admission test first failed on an incorrect setup-pointer column. The query now joins `current_definition_revision_id`, as migration 941 defines. Its passing rerun preserves immutable revisions in the test fixture.

## Browser chat required-test ledger

| Changed boundary | Required proof | State |
|---|---|---|
| Cookie-authenticated setup proxy | Exact whitelist, same-origin before credential mint, bounded/cancelled body, preserved status/Problem details/tags and no private caching | Focused cases being added |
| Browser request and mutation retention | Bound streamed reads; preserve 401/403/404/410; exact key/body/tag after lost answer; changed intent and late completion fencing | Focused cases being added |
| Private history and lifetime | Terminal-page cursor advancement, in-flight reconciliation, bounded ordered tail; clear on access loss, creator/workspace/account change and unmount | Focused cases being added |
| Review and setup | Display exact candidate before Save/Apply; no implicit publication; replay partial Save; pin Apply setup tag and publication revision tag | Focused cases being added |
| Native chat page | Authenticate first, resolve owning account, hide foreign workspace; no first-load mutation or bundle input | Replaces import-gallery page tests |
| Native Library inspection | Read a pinned revision before chat; no resource mutation on browse/open | Focused cases being added |

## Native platform Library required-test ledger

| Changed boundary | Required proof | State |
|---|---|---|
| Opaque Library identity | Published platform slug supports Inspect, history, draft source and reviewed Save; owned identifiers retain their existing meaning | Pending focused real-schema cases |
| Platform candidate staging | Platform scope plus current creator/workspace authority; exact replay; no bundle or repository input | Pending focused served-route cases |
| Platform curation | Draft entries stay hidden until explicit publication; stale tags write nothing; withdrawal hides discovery and new installs while existing fleet pins survive | Pending focused real-schema cases |

`cargo test -p afd_api --all-features --test tenant_plane authoring::catalogue -- --include-ignored`: 3 passed, 0 failed, 378 filtered. Proves current workspace-owner model eligibility, exact price strings, credential removal and invalid filter/cursor refusals against the real router and Postgres. Matching browser acceptance remains pending.

`bun test test/chat` in `cli/`: 29 passed, 0 failed, 103 expectations across 5 files. These focused checks cover retained requests, draft sessions, explicit setup actions and incremental progress hints. Script and interactive-terminal acceptance remain pending.


## Oct 15, 2026: focused terminal and build-binding checks

These results supersede earlier pending entries for the named behaviors only. Full repository verification and matched-build acceptance remain open.

| Command | Observed result | What it proves |
|---|---|---|
| `bun test test/chat test/services-http-client.unit.test.ts test/coverage-fill-patch.unit.test.ts test/sse-streamget.unit.test.ts` in `cli/` | 103 passed; 0 failed; 293 expectations; 13 files | Terminal chat retention, lifecycle preconditions, JSON line framing and safe refusal rendering. |
| `bunx vitest run tests/build-binding-io.test.ts tests/build-binding.test.ts tests/workspace-chat` in the app | 26 passed; 0 failed; 3 files | Real chat Save/publication controls; lost-answer retry; account clearing; identity mismatch ordering; bounded process-record reads. |
| `cargo test -p afd_api --all-features --test tenant_plane authoring::library::platform -- --ignored` | 5 passed; 0 failed; 362 filtered | Explicit platform publication, creator authority, current tags, revision pins and unchanged metadata retries. Uses isolated Postgres fixtures. |
| `cargo test -p afd_credential --all-features --lib vault::inspect` | 4 passed; 0 failed | Refresh-token presence, canonical connector binding and positive GitHub installation identity. |
| `cargo test -p afd_api --all-features --test operator_plane admin_plane::library` | 3 passed; 0 failed | Platform scope, removed upload shapes, exact body ceiling and missing precondition handling. |
| `make lint-cli` | Passed, including TypeScript and source audits | Current terminal source and acceptance fixture types. |
| `make lint-app` | Passed before the final process-reader path adjustment | App lint and types at that point; rerun required after further edits. |

`make harness-verify` reported green with no staged source files. That result does not establish conformance of the unstaged change set.

The Library metadata retry check first failed because its assertion expected an HTTP header on a 412 response. The REST guide requires the current `etag` in the problem body. The corrected assertion passes and the production query now preserves the version and timestamp for identical metadata.

Acceptance fixtures now create native drafts, use terminal Review and explicit Save, and publish Library revisions explicitly. Removed fixtures uploaded Markdown bundles through retired routes. These replacement live journeys have not run yet. Missing prepared Linux runner observations still prevent matched-build acceptance.

The refresh-token check keeps decoded tokens in a zeroizing string. The read-only response projection includes public binding metadata only. No credential value enters the review digest or logs.

## Oct 15, 2026: Dimension coverage map

This map comes from reading test source, not from a run. Integration rows carry `#[ignore]` and run only in `make test-integration-rustd`, which runs once, before the PR push. Only `test_definition_roundtrip_preserves_policy` exists under its spec name. The other rows name the closest behavior-named tests.

| Dimension | Closest existing tests | Tier | Coverage and gap |
|---|---|---|---|
| 1.1 | `afd_fleet_runtime/tests/definition/roundtrip.rs` `test_definition_roundtrip_preserves_policy`; `validation.rs` `test_definition_rejects_unsupported_metadata_versions_and_identity` | unit | Partial: no test refuses a `---`-fenced Markdown document |
| 1.2 | `afd_fleet_runtime/tests/budget.rs` `test_nanos_rejects_zero_signs_fractions_exponents_and_overflow`; `afd_db/tests/integration_migrate.rs` `test_migrate_parity_fresh_db` | unit, integration | Partial: no least-privilege role test; no populated-reset migration test |
| 2.1 | `afd_authoring/tests/draft/privacy.rs` `workspace_owner_cannot_read_member_draft_and_revocation_blocks_replay`; `afd_api/tests/authoring/stream/privacy.rs` `revoking_the_person_closes_an_existing_private_stream` | integration | Partial: foreign-creator 404 proven in the domain layer, not through the router |
| 2.2 | `afd_authoring/tests/draft/messages.rs` `stop_cutoff_preserves_new_messages_and_exact_operation_replay`; `afd_api/tests/authoring/lifecycle.rs` `private_draft_http_lifecycle_replays_stops_edits_and_purges` | integration | Partial: no model switch after disablement, queued-switch refusal or send-versus-Stop race |
| 2.3 | `afd_authoring/tests/draft/review/identity.rs` (three tests); `creation/identity.rs` `stale_review_allocates_neither_fleet_nor_replay_receipt` | integration | Covered in the domain layer. Open question: `afd_api/tests/authoring/readiness/svix.rs:37` asserts a credential rotation changes `review_digest`, against the spec's "readiness stays separate" |
| 3.1 | `afd_runner/tests/capacity/{concurrent,expiry,choice}.rs`; `afd_fleet/tests/lease_renewal/expiry.rs` | integration | Partial: steps proven apart; no expiry, admission and delayed-renewal sequence, no deadlock proof |
| 3.2 | `afd_api/tests/authoring/models/{refusals,invocation}.rs`; `afd_authoring/tests/draft/model_calls/authorization.rs` | integration | Partial: no credential-mint denial; no 32 KiB edit; token limits domain-only |
| 3.3 | `afd_authoring/tests/draft/model_calls/replay.rs`; `admission/capacity.rs`; `model_calls/reports/{uncertainty,replay}.rs` | integration | Partial: no overflow, concurrent differently priced turns or runtime-debit interleaving |
| 3.4 | `afr_supervisor/src/report_spool/authoring/tests/{retention,integrity,cleanup}.rs` (`one_unremovable_entry_never_holds_back_a_healthy_one`, `an_unwritable_spool_fails_the_whole_pass_and_reports_it`); `afd_api/tests/authoring/models/settlement.rs` | unit, integration | Partial: cleanup failure proven per entry and per spool; no never-received-report path |
| 4.1 | `afd_authoring/tests/draft/creation/{contention,identity}.rs`; `afd_api/tests/authoring/saving/lifecycle.rs` | integration | Partial: no fault injection after each setup stage |
| 4.2 | `afd_authoring/tests/draft/creation/{grants,cancellation}.rs`; `afd_api/tests/authoring/saving/purge.rs` | integration | Partial: no concurrent Kill-versus-activation race |
| 4.3 | `afd_authoring/tests/draft/creation/{activation,stages}.rs` (`an_unsaved_receipt_answers_the_save_pending_and_the_same_key_accepts_that_event`, `a_refused_append_or_a_held_row_answers_pending_and_the_same_key_accepts_that_event`); `afd_api/tests/authoring/saving/initial_run.rs`; `afd_admission/tests/receipts/confirmation.rs`; `cli/test/chat/command-recovery.test.ts` | integration, unit | Covered: append and receipt faults answer Save `201` `pending` under one event id, and the same key accepts it |
| 5.1 | `afd_api/tests/authoring/saving/publication.rs`; `library/publication.rs`; `library/platform/visibility.rs` | integration | Partial: no foreign-workspace delete 404 |
| 5.2 | `afd_api/tests/authoring/saving/revisions/{refusals,policy}.rs`; `revisions/schedules/restore.rs` | integration | Partial: no stale-digest Apply refusal; Restore proven across one Apply |
| 6.1 | `ui/packages/app/tests/e2e/acceptance/{fleet-execution,template-onboarding,install-fleet-cli}.spec.ts` | e2e | Partial: no browser chat send, joined member, Improve, keyboard or reload |
| 6.2 | unit stand-ins only (`draft-client.test.ts`, `workspace-chat/saving.test.tsx`) | unit | None at the e2e tier; `workspace-chat.spec.ts` does not exist |
| 7.1 | `afd_events/tests/queued/{receipt_recovery,paging,read}.rs`; `afd_admission/tests/receipts/confirmation.rs` | integration | Partial: no single retry, sweep and lease test reaching one event |
| 7.2 | `afd_api/tests/{route_inventory,route_meta_total}.rs`; `ui/packages/app/tests/build-binding.test.ts` | unit | Partial: the six analytics events are not implemented; no release smoke or failed-release recovery |

## Oct 15, 2026: Rust coverage gate before the Pull Request

Indy, Oct 15, 2026: "its 99% of the rust git diff patch files", and later the
same day: "it does hold good for the later PRs". Every Rust file the patch
touches reaches 99% across all of its instrumented lines, not only the changed
ones. The gate enforces it: `RUSTD_PER_FILE_FLOOR ?= 99`
(`make/test-integration-rustd.mk`) passes `--per-file-floor` to
`scripts/rustd_coverage.py` whenever a patch base is set, which means every
pull request in Continuous Integration (CI). It sits beside the changed-line
grade (`RUSTD_PATCH_FLOOR ?= 99`), and Codecov has no per-file status, so
this grade is the only one. Its tests are `scripts/rustd_coverage_test.py`,
run by `make lint-rustd`.

- Measure once, after the branch is committed:
  `make test-coverage-rustd RUSTD_PATCH_BASE=origin/main`. That run is also the
  single live-datastore run before the Pull Request.
- What counts: every `.rs` file the patch adds, modifies or renames, read from
  `git diff --name-only -M`. A pure move counts (Indy: "all renames must be
  graded for 99%"). Only files the report measures are graded; a crate's
  `tests/` target is outside the report, the same rule the patch grade applies.
- Below 99%: write tests for the listed unhit lines and run it again. Never
  lower a floor, add an exclusion, or baseline a file to get green.
- Linux-only lines: the macOS run cannot reach them, so they are graded from
  the kernel-lane report in CI. The kernel-lane exclusion was dropped on
  Oct 15, 2026.
- Paste the verdict lines into the Pull Request Session Notes.

## Oct 15, 2026: decisions on two flaws found during the agent fixes

- **Refused authoring reports: fix in M227.**
  - The flaw: `afr_supervisor/src/report_spool/authoring/drain.rs` retried a
    report the daemon refuses outright every second (`RETRY`) for its 24 h
    `PRIVATE_TTL_MS`. A `?` in the send loop also kept every later spooled
    report from being sent.
  - The decision: a permanent refusal now drops the spool entry with a
    structured event, and outages still retry. Indy chose "Drop and continue".
- **Unknown keys inside a trigger: deferred.**
  - The flaw: the Definition parser silently drops an unknown key inside a
    trigger (for example `"every"` on a cron trigger), because the tagged
    enum's variant is read from a buffered copy that `serde_ignored` never
    sees.
  - The decision, quoting Indy: "Leave it, we could use jev to make this
    deterministic later".
  - What holds the gap in place: the test
    `an_unknown_trigger_key_is_accepted_by_both`
    (`afd_fleet_runtime/tests/definition/schema_parity/`) pins it, and the
    OpenAPI trigger variants stay open until it is fixed.
