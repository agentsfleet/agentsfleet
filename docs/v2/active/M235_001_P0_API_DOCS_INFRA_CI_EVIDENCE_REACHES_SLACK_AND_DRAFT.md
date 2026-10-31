<!--
SPEC AUTHORING RULES (load-bearing; the one comment that survives):
- Body order is the implementing agent's read order. Use orly-spec and spec.md.
- Keep 3 to 9 Sections and at most 500 lines. Delete template guidance.
- No time or effort estimates, percentage complete, assigned owners or implementation dates.
- Priority sets urgency. Depends on sets sequencing.
-->

# M235_001: CI evidence reaches Slack and a bounded draft

**Prototype:** v2.0.0

**Milestone:** M235

**Workstream:** 001

**Date:** Oct 30, 2026

**Status:** IN_PROGRESS

**Priority:** P0, fleet-owned job-log reads block M224's development demonstrations

**Categories:** API, DOCS, INFRA

**Batch:** B1, independent support for M224 development acceptance

**Branch:** `feat/m235-ci-evidence`

**Baseline revision:** 7b18cd4166b87b9289c9d94cd31000f96d500e15

**Test Baseline:** unchanged comparison revision fails two executor process-reaping tests; see baseline evidence. Full repository verification remains incomplete before the Pull Request (PR).

**Baseline evidence:** `playbooks/operations/acceptance/baselines/M235_001-ci-evidence.md`; distinguish baseline failures, changed-package tests and live fleet proof

**Depends on:** none; use existing native definitions, guarded tools, Slack mentions and development access

**Provenance:** drafted by Codex, GPT-6; exact model version is not exposed by this session

**Canonical architecture:** `docs/architecture/runner_fleet.md`, Egress model; `docs/architecture/scenarios/slack-incident-responder.md`, Evidence sources

---

## Overview

**Goal (testable):** The Continuous Integration (CI) responder reads logs and answers Slack; the explicitly addressed repairer opens a verified draft.

**Problem:** GitHub job logs redirect to storage that the fleet cannot read. The reference responder also requires Grafana evidence for CI failures that have no running workload. M224 records the real failed job, successful native log retrieval and fleet source gap.

**Solution summary:** Extend generic, guarded HTTP GET reads and tune native Markdown instructions. The model selects endpoints and interprets evidence; reviewed policy controls access. Use GitHub Actions evidence for CI and require a named stopping answer when required evidence is unavailable.

**Design flow:** Slack mention → existing admission and reviewed fleet revision → model-selected HTTP reads → evidenced Slack answer. A separate addressed repair request → fresh evidence/base reads → existing bounded draft writes → same-thread PR link and regression validation. M224 consumes these development results under its own launch rubric; M235 does not depend on M224 completion.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix: read CI evidence through guarded HTTP redirects
- **Intent (one sentence):** A customer receives an evidenced failure diagnosis and can explicitly request a bounded draft fix from the same Slack thread.
- **Authoring handshake:** Keep customer tailoring in `instructions_md`; add the generic transport capability needed by those instructions. ASSUMPTIONS I'M MAKING: source and instruction implementation follow this spec's review; authoring changes no runtime setup or authentication.
- **Implementer handshake:** Record the restatement in Session Notes at PLAN before source edits; reconcile any difference from this Intent.

## Implementing agent — read these first

1. `rustd/crates/afr_egress/src/egress/mod.rs`: admission and credential substitution before transport.
2. `rustd/crates/afr_tools/src/http_request/mod.rs`: public tool arguments, status rendering and truncation notice.
3. `docs/architecture/runner_fleet.md`: supervisor credentials, network boundaries and durable event authority.
4. `docs/architecture/scenarios/slack-incident-responder.md`: mention routing, original reply destination and bounded repair.
5. `docs/architecture/fleet_definitions.md`: native Markdown instructions, immutable review and explicit Save/Apply.

## Files Changed (blast radius)

Paths below are implementation scope; authoring changes only this spec, the M224 amendment, its launch baseline and the evidence-format document. All listed new source and test names describe proposed files, not existing implementations.

| File | Action | Why |
|---|---|---|
| `docs/v2/pending/M235_001_P0_API_DOCS_INFRA_CI_EVIDENCE_REACHES_SLACK_AND_DRAFT.md` → `docs/v2/active/M235_001_P0_API_DOCS_INFRA_CI_EVIDENCE_REACHES_SLACK_AND_DRAFT.md` → `docs/v2/done/M235_001_P0_API_DOCS_INFRA_CI_EVIDENCE_REACHES_SLACK_AND_DRAFT.md` | CREATE / MOVE / EDIT | author, open and close this focused source repair |
| `docs/v2/active/M224_001_P0_DOCS_INFRA_FIRST_PRODUCTION_LAUNCH.md` | EDIT | align CI-only prerequisites, dependency and actual-result acceptance |
| `playbooks/operations/acceptance/baselines/M224_001-launch.md` | EDIT | keep launch history and link new development proof without manufacturing a pass |
| `rustd/crates/afr_egress/Cargo.toml` | EDIT | use the existing workspace validation dependency for bounded redirect input |
| `rustd/Cargo.lock` | EDIT | record updated stable workspace dependencies and the validation/test edges |
| `rustd/crates/afr_egress/src/transport.rs` | EDIT | hold opaque redirect targets with safe formatting and explicit consumption |
| `rustd/crates/afr_egress/src/network/mod.rs` | EDIT | return single-hop metadata without exposing signed target details |
| `rustd/crates/afr_egress/src/network/tests.rs`; `rustd/crates/afr_egress/src/network/admission/tests.rs` | EDIT | retain resolver, decoding, exact-cap and single-hop tests |
| `rustd/crates/afr_egress/src/egress/mod.rs` | EDIT | admit each next request and scrub private redirect material |
| `rustd/crates/afr_egress/src/redirect/mod.rs` | CREATE | own one bounded GET redirect chain and safe terminal outcomes |
| `rustd/crates/afr_egress/src/redirect/target.rs` | CREATE | validate the bounded opaque target and its safe projection |
| `rustd/crates/afr_egress/src/redirect/privacy.rs` | CREATE | scrub known target echoes and redirect-derived failures |
| `rustd/crates/afr_egress/src/redirect/tests.rs` | CREATE | test target admission, headers, loops, budgets and private state |
| `rustd/crates/afr_egress/src/redirect/privacy/tests.rs` | CREATE | isolate signed-target output tests within source-size bounds |
| `rustd/crates/afr_egress/src/lib.rs` | EDIT | connect the redirect mechanism to its first caller |
| `rustd/crates/afr_egress/src/testing/mod.rs` | EDIT | record scripted hop requests and opaque target responses |
| `rustd/crates/afr_egress/src/admission/tests.rs` | EDIT | prove existing repository/path and URL refusals on later hops |
| `rustd/crates/afr_egress/src/admission/credential_tests.rs` | EDIT | prove credential binding and absence after origin changes |
| `rustd/crates/afr_agent/src/trace/mod.rs` | EDIT | return an explicit known empty trace for a completed zero-tool turn |
| `rustd/crates/afr_agent/src/trace/tests.rs` | EDIT | distinguish zero calls from unavailable trace data |
| `rustd/crates/afr_agent/src/ledger/mod.rs` | EDIT | carry known trace data through the existing completion boundary |
| `rustd/crates/afr_agent/src/ledger/tests.rs` | EDIT | preserve trace and record accounting through completion |
| `rustd/crates/afr_agent/src/loop/finish.rs` | EDIT | report a known empty trace without adding a wire field |
| `rustd/crates/afr_agent/src/loop/tests.rs` | EDIT | prove the zero-tool model reply keeps explicit trace provenance |
| `rustd/crates/afr_tools/src/egress/mod.rs` | EDIT | reuse masking/refusal handling without changing other tools' behavior |
| `rustd/crates/afr_tools/src/egress/tests.rs` | EDIT | protect single-hop behavior of unaffected callers |
| `rustd/crates/afr_tools/src/http_request/mod.rs` | EDIT | select bodyless GET following and render safe outcomes |
| `rustd/crates/afr_tools/src/http_request/tests.rs` | EDIT | exercise the composing tool, privacy and unlocked guard during network waits |
| `rustd/crates/afr_tools/src/http_request/redirect/tests.rs` | CREATE | isolate composing-tool redirect scenarios within source-size bounds |
| `rustd/crates/afd_fleet_runtime/tests/definition/corpus.rs` | EDIT | check GitHub-only responder reach and exact reviewed download hosts |
| `tests/fixtures/fleets/ci-responder/definition.json` | EDIT | select the exact attempt, read logs/source and stop on missing evidence |
| `tests/fixtures/fleets/ci-repairer/definition.json` | EDIT | reread required evidence and reconcile bounded writes |
| `tests/fixtures/fleets/github-log-reader/definition.json` | CREATE | verify exact GitHub job-log retrieval in a separate read-only fleet |
| `rustd/crates/agentsfleetd/tests/support/https.rs` | EDIT | return real redirect/text responses and simulate dropped write replies |
| `rustd/crates/agentsfleetd/tests/support/definition_responder.rs` | EDIT | script GitHub job text without a Grafana prerequisite |
| `rustd/crates/agentsfleetd/tests/support/definition_repair.rs` | EDIT | model remote ref/draft state and uncertain-write reconciliation |
| `rustd/crates/agentsfleetd/tests/integration_rust_runner_definitions.rs` | EDIT | prove actual hosted tool reads and private transcript/trace handling |
| `rustd/crates/agentsfleetd/tests/integration_rust_runner_definitions/redirect/tests.rs` | CREATE | compose downloaded text, error privacy and native full-call reads within source-size bounds |
| `rustd/crates/agentsfleetd/tests/integration_tool_trace.rs` | EDIT | prove empty known traces remain distinct from unavailable records at the API |
| `rustd/crates/agentsfleetd/tests/integration_rust_runner_repairs.rs` | EDIT | assert all-state reconciliation and no second external draft |
| `rustd/crates/afd_api/tests/integration_slack_mention/mod.rs` | EDIT | register the producer replay proof in the existing integration suite |
| `rustd/crates/afd_api/tests/integration_slack_mention/replay.rs` | CREATE | prove repeated and concurrent producer deliveries keep one admission identity |
| `rustd/crates/afd_api/tests/slack_mention_live/fixture.rs` | EDIT | share signed producer input and captured outcomes for replay |
| `tests/fixtures/fleets/ci-investigation/cases.json` | CREATE | pin the small manual behavior corpus and expected observations |
| `scripts/check-ci-drill-evidence.ts` | CREATE | validate complete evidence records and compute behavior thresholds offline |
| `cli/test/ci-drill-evidence.unit.test.ts` | CREATE | reject missing cases, stale digests, unsupported grades and mismatched source identities |
| `playbooks/operations/slack_incident_drill/001_playbook.md` | EDIT | describe exact setup, source failures, manual evaluations and development proof |
| `playbooks/deploy/dev/001_playbook.md` | EDIT | document the approved exact-ref prerequisite for pre-merge development proof |
| `playbooks/operations/acceptance/baselines/M235_001-ci-evidence.md` | CREATE | record measured baseline, review, evaluation and real-path evidence |
| `playbooks/operations/acceptance/baselines/M235_001-ci-evidence.json` | CREATE | store sanitized grades, result identities and every hashed witness in its inline named-record list |
| `docs/architecture/ci_drill_evidence.md` → `playbooks/operations/acceptance/ci_evidence_format.md` | MOVE / EDIT | keep proposed native witnesses, hashes, evaluation selection and runtime correlation with acceptance procedures |
| `docs/architecture/runner_fleet.md` | EDIT | define guarded tool redirects beside existing network authority |
| `docs/architecture/observability.md` | EDIT | document the safe redirect outcome event |
| `docs/architecture/scenarios/slack-incident-responder.md` | EDIT | describe CI-only evidence, stopping behavior and explicit repair |

The matching documentation branch belongs to the separate `/workspace/docs` checkout, using its own instructions and branch. Its scoped files are `fleets/tools.mdx` and `changelog.mdx`; update them with the implementation before the source PR becomes reviewable. The authoring amendment does not publish tool behavior or edit that separate repository.

| File | Action | Why |
|---|---|---|
| `rustd/Cargo.toml` | EDIT | update all workspace registry dependencies to their latest stable releases |
| `rustd/crates/afd_wire/src/policy/mod.rs; rustd/crates/afd_wire/src/policy/http_egress.rs; rustd/crates/afd_wire/tests/redaction.rs` | EDIT / CREATE | publish explicit HTTP destination scope and the shared package preset |
| `rustd/crates/afd_fleet_runtime/src/config/raw/policy.rs; rustd/crates/afd_fleet_runtime/src/config/policy/mod.rs; rustd/crates/afd_fleet_runtime/src/config/policy/http_egress.rs; rustd/crates/afd_fleet_runtime/src/config/policy/tests.rs; rustd/crates/afd_fleet_runtime/tests/definition/digest.rs` | EDIT | parse, validate and bind reviewed HTTP egress settings |
| `rustd/crates/afd_gate/src/policy/shape.rs; rustd/crates/afd_fleet/src/lease/answer/tests.rs` | EDIT | carry the reviewed settings into the lease |
| `rustd/crates/afr_egress/src/admission/mod.rs; rustd/crates/afr_egress/src/admission/http_egress/tests.rs; rustd/crates/afr_egress/src/fixture.rs` | EDIT / CREATE | admit public HTTPS or preset domains without relaxing other restrictions |
| `rustd/crates/afr_supervisor/src/egress/mod.rs; rustd/crates/afr_supervisor/src/egress/tests.rs; rustd/crates/afr_supervisor/src/holds/built_under.rs; rustd/crates/afr_supervisor/src/holds/built_under_tests.rs; rustd/crates/afr_supervisor/src/workspace_clone/fetch.rs; rustd/crates/afr_supervisor/src/workspace_clone/checkout.rs` | EDIT | reuse registry names and preserve sandbox network boundaries |
| `public/openapi.json` | GENERATE | publish the native API settings |
| `rustd/crates/afd_api/src/openapi/stability/mod.rs; rustd/crates/afd_api/src/openapi/stability/tests.rs` | EDIT | publish explicit beta classes for the new HTTP settings |
| `rustd/crates/afd_authoring/src/model/provider/schema.rs; rustd/crates/afd_authoring/src/model/provider/tests/proposals.rs; rustd/crates/afd_wire/tests/policy_shapes.rs` | EDIT | expose reviewed HTTP settings to authoring and prove wire defaults |
| `ui/packages/app/components/domain/chat/SetupSummary.tsx; ui/packages/app/tests/workspace-chat/setup-summary.test.tsx` | EDIT | describe reviewed HTTP egress accurately without adding settings controls |
| `make/acceptance.mk; Makefile; README.md; docs/development.md; scripts/local-dev-state.sh; deploy/local/acceptance.sh; deploy/local/bootstrap.sh; deploy/local/bootstrap_test.sh; deploy/local/orbstack.sh; deploy/local/codex.sh; deploy/local/codex-runner.Dockerfile; deploy/local/codex-runner.sh; deploy/local/codex-compose.yml; deploy/local/codex-certificate.sh; deploy/local/codex_test.sh` | EDIT / CREATE | share acceptance orchestration and add a Linux Docker runner adapter |

## Applicable Rules

- `.orly/standards/code.md`: GRD, NLG, HLP, PSR, ECL, PRI, FLL and CODE-MSID govern source grounding, parsing, errors and bounded helpers.
- `.orly/standards/languages/rust.md`: ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE, RS-NEWTYPE, RS-RACE and RS-FEATURE govern fallible interfaces and test seams.
- `.orly/standards/tests.md`: TCF, TEST-REDGREEN, TEST-NEG, TEST-OUTCOME, TEST-INJECT, TEST-ACKLOSS, TEST-CONTEND and TEST-LANE govern meaningful proofs.
- `.orly/standards/domains/architecture.md`: ARCH-GROUND, ARCH-LAND and ARCH-PUBLISHED govern internal and published behavior documentation.
- `.orly/standards/domains/logging.md`: LOG-PAIR, LOG-HOT, LOG-SECRET, LOG-REDACT and OBS govern bounded network outcomes without private URLs.
- `.orly/standards/domains/http.md`: HTTP-GARDE and HTTP-VALID govern bounded target validation before URL parsing or admission.
- `.orly/standards/docs.md`: DOC-02, DOC-03, DOC-14b and DOC-43 govern clear, source-grounded prose.
- `.orly/standards/spec.md`: SPEC-MECH, SPEC-PIN, SPEC-DIM, SPEC-FAIL, SPEC-MODEL and SPEC-INVARIANT separate behavior expectations from enforced boundaries.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| `orly doctor`; `orly gate work` | yes | keep installed rules intact and diff inside named scope |
| Rust clippy, coverage and declared make lanes | yes | exercise each new branch through existing unit/integration lanes |
| Native definition corpus | yes | update source fixtures and their reach assertions together |
| Documentation and secret checks | yes | inspect changed prose and scan staged changes without suppressions |
| File & Function Length: file ≤350, function ≤50, method ≤70 | yes | keep redirect logic and tests in focused modules |
| Authentication, release and workflow edits | no | change no login, token mint, grant, workflow or deployment mechanism |

## Prior-Art / Reference Implementations

- **Reference:** `afr_egress` admission, host-only `Outbound` formatting and guarded resolver; preserve these boundaries at every hop.
- **Reference:** `agentsfleetd/tests/support/https.rs` and runner definition tests; extend their real HTTP path and request capture.
- **Reference:** Slack admission uniqueness, event-derived repair branch and existing ref/PR reconciliation; add missing replay and acknowledgement-loss proofs.
- **Greenfield:** the small evidence checker has no established behavior-evaluation runner to mirror. It grades retained records and never chooses a diagnosis.
- **Pinned dependencies:** comparison source `7148b1d4ff9a5ad7da3c4ba3c562ec539abef7a1`, `reqwest` 0.13.5 and `url` 2.5.8. Dependency changes rerun admission, credential, privacy, resolver and budget proofs.

## Sections (implementation slices)

### §1: Admit each redirected GET

Follow 301, 302, 303, 307 and 308 only for bodyless `http_request` GET calls. Resolve one valid Location against the current URL, including relative references, before reusing the lease's admission for the next request. Missing, duplicate, invalid or oversized locations end safely without another send.

Keep the shared builder's redirect policy disabled; inference clients, other tools and non-GET calls retain their existing no-follow behavior. Every hop must satisfy reviewed HTTPS, URL, host-scope, address, repository/path and origin rules before any connection. The production resolver refuses a name with any blocked address, including a changed answer on a later connection.

Compare normalized URL origins, including effective port, when deciding whether request authority changed. Same-origin requests repeat admission with the original modeled headers; at the first origin change, discard every caller header and any body. Never restore those headers, mint another destination credential or widen the allowlist later in the chain.

The private target lives only for one tool call and has no raw Debug, display or serialized projection. Admission remains the request authority; release the lease guard before each upstream transport wait while preserving guarded credential minting. This change retains existing identity, login, token scopes and grant behavior.

- **Dimension 1.1** — An admitted GET chain returns actual upstream text through the hosted tool → Test `should_read_text_through_admitted_redirects`
- **Dimension 1.2** — Forbidden or malformed targets never receive a request → Test `should_refuse_redirect_before_transmission`
- **Dimension 1.3** — Origin changes permanently remove caller headers, including a return to the original origin → Test `should_strip_headers_across_redirect_origins`
- **Dimension 1.4** — Other clients and methods keep their single-hop behavior → Test `should_keep_unaffected_clients_single_hop`
- **Dimension 1.5** — Protocol failures never silently repeat a tool request → Test `should_disable_implicit_tool_request_retries`

**HTTP settings:** `network.http_egress` accepts `restricted` (default) or `public`; `package_manager_domains` defaults to false. Existing `allow` supplies additional exact domains. These settings govern HTTPS tools only. Public mode still refuses private/reserved or mixed DNS answers and retains credential, repository/path and method restrictions. Redirects repeat those checks. Neither new setting enlarges sandbox network access. UI controls are later work.

- **Dimension 1.6** — Reviewed HTTP scope reaches the lease without weakening other access restrictions → Test `should_preserve_boundaries_with_public_http_egress`

### §2: Bound the call and protect download targets

**Implementation defaults:** at most three followed redirects, one 30-second call deadline, a 10-second connect ceiling within remaining time, and the existing 1 MiB body ceiling. The deadline covers admission, minting, connects and reads; it never resets between hops. Validate raw Location and resolved target against an 8-KiB bound through `garde` before parsing or admitting the respective value.

Explicitly disable the pooled tool client's `reqwest` retries in `Network::build`; leave the shared builder and inference client's retry policy unchanged. This includes protocol retries for other tool methods, so a failed transport attempt ends instead of replaying a possible write. A followed GET therefore makes at most four physical request attempts; retain one pooled tool client and count wire requests in protocol-failure tests.

Discard intermediate response bodies without reading them; close those responses before continuing. The final decoded response body shares the existing cap; no archive extraction, implicit range reads or transport retry is added. A body exactly at the cap remains complete only when the upstream stream ends there.

Keep the visible HTTP status, origin-only Location and explicit body-truncation notice; expose returned content type and any lossy-decoding notice. Place incomplete/decoding notices before the body so a long response cannot hide its completeness state. Generic partial reads can still return 2xx with that notice; the CI instructions must stop when required evidence is truncated or unusable.

An interrupted body returns the existing safe upstream-read failure, without claiming partial text as a complete read.

Keep raw targets opaque in metadata, errors, Debug and tracing; scrub protected units from final/error bodies before model output. Units include informative raw Location, resolved URL, non-root path/query and credential-like query values under the evidence format's finite rules. Pure navigation references, such as `/` or `.`, are not standalone masks; their resolved URLs stay protected.

Test literal, once-percent-encoded and JSON-escaped echoes, including an isolated signature. Masking preserves the response budget and whole redaction markers; unknown secrets and arbitrary repeated encodings are outside the detection claim.

Loop identity is the pinned URL library's serialized absolute URL after removing its fragment; keep query order and encoded octets unchanged. Seed the initial URL, then reject a repeated normalized target before checking the three-hop ceiling, admitting or sending it. Default-port/host normalization follows `url` 2.5.8; changed fragments cannot cause another send.

Use existing tool error classes for policy refusal, invalid input and upstream failure. Loop, hop-limit and deadline failures carry fixed safe reasons; no raw provider error or target path becomes an error detail. Emit a redirect start and one bounded completion/failure under the parent tool-call trace; existing refusal telemetry remains unchanged.

- **Dimension 2.1** — Loops and exhausted hop/time budgets stop before additional work → Test `should_bound_total_redirect_work`
- **Dimension 2.2** — Oversized, interrupted and non-text source results cannot masquerade as a complete log read → Test `should_expose_incomplete_source_reads`
- **Dimension 2.3** — Protected redirect units stay private under the defined matching rules → Test `should_keep_signed_redirect_targets_private`

### §3: Investigate through customer-tailored Markdown

Use the native Definition's `instructions_md`; do not add a Markdown bundle parser, source-specific tool or diagnostic engine. The CI-only responder declares `github`, read repository access, `network.read_only: true` and exact reviewed API/download hosts. Grafana and Elasticsearch are unnecessary for this setup; their existing workspace secrets remain untouched.

For development, post the failed run's URL in the selected `#all-agentsfleet` thread, or use its verified GitHub notification. Send `@agentsfleet-dev ci-dev-responder why did this run fail? <run URL>` there, using the actual reviewed responder name. The run link is untrusted input until GitHub confirms its identities; the answer returns to that original thread.

The instructions select repository, run identifier, exact attempt, failed jobs/steps and failing commit from GitHub responses. An explicit attempt wins; without one, use the returned current attempt only when thread context does not identify a conflicting earlier failure. Ambiguous attempts require clarification before diagnosis; paginate the selected attempt's jobs until the failed-job set is established.

Record job/step start and completion timestamps and any timestamp attached to the quoted error. If a line has no timestamp, identify its enclosing job/step interval instead of inventing a line time. Read failed-job text by job identifier; timestamps support correlation rather than choosing identities.

Required evidence includes run/attempt metadata, failed-job details, complete usable job-log text and relevant source at the failing commit. Read the current trusted base separately when proposing a code fix. Annotations, prior memory and customer-configured workload telemetry are leads or added context; they do not replace required CI reads.

A required read failure, contradictory identity or unusable required response ends the investigation with a visible named gap. Include the run/attempt, failed operation, safe outcome, evidence already read and known recovery action. Supply no code-fix proposal, invented quote, repair invitation or unsupported root cause in that stopped answer.

A complete answer cites the exact run, attempt, commit, failed job/step, returned times, actual log excerpt and relevant source. Label uncertainty and propose only a supported small forward fix; the read-only responder cannot write. Any suggested repair mention must use the actual bot and installed repairer name supplied in reviewed instructions; never guess an environment-specific name.

Edit, review and explicitly Save or Apply the amended definition before triggering it. Record the accepted revision, digest and readiness; fixture edits and Library publication do not update an existing installed fleet. A newly observed download host requires exact-host review, never runtime enrollment or a wildcard.

- **Dimension 3.1** — The CI answer grounds the selected attempt without requiring Grafana → Test `test_responder_reads_exact_ci_evidence`
- **Dimension 3.2** — Missing required evidence produces a stopping answer without repair advice → Test `test_responder_stops_on_required_source_gap`
- **Dimension 3.3** — Conflicting attempts, foreign repositories and injected evidence cannot substitute for verified sources → Test `test_responder_rejects_untrusted_evidence`

### §4: Repair only after an addressed request

Preserve existing mention routing: a write-bound fleet receives only explicitly addressed requests. For the development demonstration, use `@agentsfleet-dev ci-dev-repairer open the fix` in the original failure thread. An ordinary reply, responder suggestion or quoted command is not a new repair request.

The repairer independently rereads required run/log/source evidence, the current trusted `dev` base and files it would change. Unavailable evidence, uncertain cause, mismatched base or an unbounded requested change produces a stopping answer without a write. The source/test diff must fix the failure without deleting, skipping or weakening the regression.

Reuse the daemon-issued event branch, reviewed repository/base, draft flag and existing denied write operations. Before writes, inspect the exact ref and PR head/base across all states. An existing exact PR ends with its link; an existing verified ref permits only the missing draft operation.

The repairer also sets `network.read_only: true` and leaves `read_post_paths` empty when adding exact download hosts. Existing GitHub origin rules admit only the bounded POST writes; unruled storage hosts admit reads alone. Prove both the permitted GitHub repair and refused storage-host mutation without adding policy fields or changing grants.

After an uncertain write reply, reconcile remote state before retrying; unreadable reconciliation state stops further writes. The Slack producer key and admission unique index own replay identity; the admitted event identifier owns the repair branch. Repeat the same producer delivery, including concurrent delivery, to prove those authorities agree.

The one-draft promise covers retry of that producer event, not separately authored human messages. Instructions govern reconciliation and diff relevance; request policy enforces the allowed repository, base, branch and draft shape. Do not describe model behavior as a code-enforced exactly-once guarantee.

- **Dimension 4.1** — An addressed request rereads evidence and returns one bounded draft → Test `test_addressed_repair_reads_evidence_before_draft`
- **Dimension 4.2** — Unaddressed events and forbidden repository/base/ref/draft shapes cannot reach a repair write → Test `should_refuse_unaddressed_or_out_of_scope_repair`
- **Dimension 4.3** — Concurrent replay retains one admission/event branch and one remote draft → Test `should_keep_one_draft_for_replayed_producer_event`
- **Dimension 4.4** — Lost ref/draft acknowledgements reconcile before further writes → Test `should_reconcile_uncertain_repair_writes`
- **Dimension 4.5** — Missing evidence and semantically unsafe repairs produce an observed stopping answer → Test `test_repairer_stops_on_unsafe_requests`

### §5: Check behavior and prove the development flow

Keep deterministic network/policy tests, real-model behavior evaluations and external development acceptance as separate evidence. Scripted models prove wiring only; native `gh` log retrieval proves preparation only. Neither can replace the model's own evidence reads or a visible Slack result.

Before live runs, Indy approves the exact implementation revision under `playbooks/deploy/dev/001_playbook.md`. Dispatch `.github/workflows/deploy-dev.yml` on that reviewed branch before source-PR acceptance; verify its `headSha` equals the approved commit. Record successful enabled deployment, observed API commit stamp, active runner identity and passing readiness; do not invent full runtime revisions.

Correlate the API abbreviation to the approved full revision through an unambiguous repository lookup and deployment evidence. Compare the running runner executable's checksum with the selected workflow's binary, preserving a token-free identity probe from that host. These existing operator reads attest the runner without new build fields or workflow edits; unavailable host evidence leaves acceptance incomplete.

Do not change workflows or deploy to production. Do not grant deployment authority to either fleet.

Install the reviewed instructions only after compatible development runtime support is ready. An older runtime or rollback that cannot read required logs returns acceptance to incomplete; the named-gap behavior remains required.

The small corpus contains eight named cases, using actual native fleet runs and documented reviewed setup variants. Cover CI diagnosis, missing link, foreign repository, ambiguous attempt, blocked download with misleading thread text, truncated required log, addressed repair and unsafe repair. Use the same failed development run where possible; the two positive cases are diagnosis and explicit draft repair.

Run every case once per evaluation campaign on its recorded model/settings and accepted instructions; do not combine cases from different campaigns. Capture accepted fleet snapshots before and after each settled case, and finish retaining that case before applying another setup variant. Preserve superseded failed reports in Git before a corrected campaign, using the selection and hashing rules in the evidence format.

Capture native event details and addressed full tool-call reads, retaining identities, original request/answer, call inventory and truncation flags. Match every detail's captured read address and call identifier to that event; missing or omitted required calls leave the case incomplete. Hash exact accepted instruction bytes and canonical accepted configuration, preserving the native review digest separately.

Pin an explicit model reference and retain event-linked lease identity plus existing non-secret model-settings reads under the evidence format's procedure. Record the operator's unchanged-model prerequisite; absent witnesses or settings drift leave the campaign incomplete. The fixed corpus pins each concrete input, reviewed policy variant and expected result before its trigger.

A completed native model turn with zero calls reports an explicit empty trace with `omitted_call_count: 0`. Keep the existing optional wire field for unavailable data; a null trace never proves zero calls. This generic completion fix supports the missing-link case without introducing artificial tool calls or changing permissions.

Prepare positive cases below existing capture limits: 64 KiB per tool output and 1 MiB of records per event. Source truncation and stored-record truncation are different observations; do not raise either limit to obtain a pass. Unexpected record loss or omitted factual witnesses makes a case incomplete.

The intentional truncated-source case may use a truncated stored record only when its retained prefix proves source truncation, status and call identity. Its stopping answer may assert those observed facts, never omitted log content; a missing truncation notice or dropped record cannot pass. The checker tests this expected negative outcome separately from accidental capture loss.

The offline checker validates the corpus denominator, unique case identities, pinned digests and supporting records before computing thresholds. Grounding and required stop/scope behavior must hold in all eight cases; useful outcomes must pass all eight. Manual semantic grading remains attributed review, not an automated factual-truth claim.

Supported review grades are `pass`, `fail` and `incomplete`; only a reviewed pass contributes to either total. Grounding passes when every factual claim has a source witness and all required stop/scope rules hold. Usefulness passes when the case's expected diagnosis, clarification, named stop or bounded draft is actually observed; a correct stop is useful.

Failures and incomplete cases stay in the fixed eight-case denominator and block shipping. An incomplete case lacks required proof; a failure has proof of an incorrect outcome, with that distinction retained in the report. The successful diagnosis/draft flow still requires its own positive evidence, regardless of stopping-case grades.

The development record includes runner/API revisions, accepted setup/digest, fleet/event identifiers and the exact GitHub run attempt/job/commit. Retain sanitized source-read evidence, original-thread trigger/answer permalinks, the addressed repair request, actual draft URL/base/head and reviewed diff. The same regression must fail before the fix and pass at the returned draft head, with command, executor, counts and exit status.

Verify the actual Slack answer and draft-link visibility separately from event processing and delivery scheduling. Indy reviews the externally visible result; the agent records evidence without inventing approval. M224 keeps all three development demonstrations, incident prerequisites and production gates; no result passes from this document alone.

- **Dimension 5.1** — Complete attributed behavior records meet the fixed thresholds → Test `test_ci_instruction_behavior_corpus`
- **Dimension 5.2** — Missing, contradictory or stale records fail the evidence checker → Test `should_reject_incomplete_ci_drill_evidence`
- **Dimension 5.3** — Real source reads, same-thread answer, bounded draft and regression result correlate → Test `test_development_ci_thread_has_verified_draft`
- **Dimension 5.4** — Zero-tool completion has an explicit empty trace, while unavailable data stays unknown → Test `should_report_known_zero_tool_calls`
- **Dimension 5.5** — A separate read-only fleet retrieves the exact GitHub job log through its own tool call → Test `test_separate_fleet_reads_github_job_log`

## Interfaces

- Keep `http_request` arguments unchanged: `url`, optional `method`, `headers` and `body`.
- Bodyless GET calls can follow the admitted redirect statuses under §1–§2; other calls keep their existing no-follow behavior.
- Tool-network protocol retries are disabled for every method; each admitted transport attempt sends once. Inference retry behavior is unchanged.
- The internal transport remains single-hop. Its opaque redirect metadata is separate from the origin-only model projection; update every internal constructor/caller together.
- Keep existing `ToolErrorCode` classes. Render safe origin, hop count, upstream status, returned content type, truncation/decoding notices and a fixed stopping reason.
- The evidence checker takes `--report` and `--section` (`grounding`, `behavior` or `development`); it reads the pinned corpus and exits nonzero on invalid or incomplete input.
- Reports use `schema_version: 1`, one selected `evaluation_id` and retained prior-report commit references; all eight active cases share that campaign's subject.
- Require eight unique case/event records, hashed prompt/tool/answer references, factual-claim witnesses, review identity/date and explicit findings, including an empty findings list.
- Development records add environment/runtime identities, run/attempt/job/commit, Slack permalinks, draft scope/head and before/after regression evidence.
- No new platform endpoint, trigger type, authentication flow or settings screen is introduced. The HTTP policy fields are the explicit exception described in §1.

The shared report format is pinned in `playbooks/operations/acceptance/ci_evidence_format.md`; changes amend that format and this spec together.
Require accepted-definition snapshots, exact instruction/policy hashes, native event/tool witnesses, a fixed evaluation selection and attributed runtime proof.
Keep every witness inline in the named JSON report; mismatched, unavailable or unsupported evidence cannot pass.

**Linux acceptance:** `make acceptance-codex` shares local bootstrap, HTTPS, dashboard and suite orchestration with `acceptance-local`. It uses a privileged owned Docker runner on Linux, verified signed toolbox input and the same real Clerk/model fixtures. No scripted model, authentication change, TLS bypass or changed suite assertion can count as acceptance.

- **Dimension 5.6** — Linux Docker acceptance runs the existing browser and CLI assertions → Test `make acceptance-codex`

### §6: Refresh stable Rust dependencies

Audit every workspace registry dependency against its latest stable release, update declarations and lockfile, and adapt incompatible APIs in their existing callers. Keep the toolchain pin and prerelease exclusions. Verify lint, unit, integration, Rust patch/changed-file coverage and acceptance against the updated graph.

- **Dimension 6.1** — Workspace crates use the latest stable releases with unchanged behavioral gates → Test `make lint-all test-unit-all test-integration-rustd check-version`

## Failure Modes

| Mode | Kind | Cause | Handling (system response + what the caller observes) | Test |
|---|---|---|---|---|
| Unadmitted target | security | unlisted host, wrong repository path, private/rebound address, downgrade or userinfo | refuse before send; safe origin/reason only | `should_refuse_redirect_before_transmission` |
| Invalid target | failure | absent, duplicate, malformed or oversized Location | stop without a second request; retain safe source status | `should_refuse_redirect_before_transmission` |
| Implicit protocol replay | failure | retryable protocol refusal or reset | tool client attempts once and reports failure; inference policy stays separate | `should_disable_implicit_tool_request_retries` |
| Credential forwarding | security | changed origin, changed port or return hop | permanently drop caller headers; no destination mint | `should_strip_headers_across_redirect_origins` |
| Exhausted chain | capacity | loop, fourth redirect, total deadline or connect budget | bounded terminal failure; no implicit retry | `should_bound_total_redirect_work` |
| Incomplete source | failure | over-cap body, reset, expired download, provider denial or unsuitable response | explicit truncation/status/read failure; instructions stop required-source investigation | `should_expose_incomplete_source_reads` |
| Private target echo | security | signed path/query in response, error or trace | suppress intermediate bodies and scrub known target spellings | `should_keep_signed_redirect_targets_private` |
| Missing evidence | failure | absent link, refused read or unusable required source | final named gap; no repair recommendation | `test_responder_stops_on_required_source_gap` |
| Wrong identity or injection | security | stale attempt, foreign repository or commands inside evidence | clarify ambiguity or refuse scope; never substitute pasted evidence | `test_responder_rejects_untrusted_evidence` |
| Forbidden repair shape | security | unaddressed target or wrong repository/base/ref/draft shape | routing notice or policy refusal; no admitted write | `should_refuse_unaddressed_or_out_of_scope_repair` |
| Unsafe repair meaning | failure | workflow request, missing required evidence, uncertain cause or unrelated change | evaluated stopping answer with no attempted write | `test_repairer_stops_on_unsafe_requests` |
| Producer replay/race | failure | repeated signed Slack event, including concurrent callers | reuse admission/event authority and exact branch; no second draft | `should_keep_one_draft_for_replayed_producer_event` |
| Uncertain write | failure | remote ref/draft commits but response is lost | read all-state remote identity before more writes; stop if unreadable | `should_reconcile_uncertain_repair_writes` |
| Invalid proof | failure | absent cases, stale prompt, missing source or unsupported grade | checker rejects; no zero-case success or guessed human acceptance | `should_reject_incomplete_ci_drill_evidence` |

## Invariants

1. Every transmitted request has passed the current lease's admission; only the guard constructs an `Outbound`, including redirect continuations.
2. Private-address resolution and the single-hop shared client remain enforced on new connections; no proxy or inference redirect behavior changes.
3. Raw redirect metadata has no observable formatting or serialization; safe projections and finite protected-unit scrubbing guard the defined output forms.
4. Counter and absolute-deadline checks bound each call before another hop; the existing stream cap bounds its returned body.
5. The admission unique key and deterministic event branch identify producer replay; request rules constrain repository/base/ref and `draft: true`.

Markdown stopping, interpretation, reconciliation and minimal-diff expectations are evaluated behaviors rather than additional runtime invariants.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| `http_redirect_started` | operator | the tool begins following an admitted redirect response | existing call/lease trace context | no path, query, header, body, credential or full URL | `should_keep_signed_redirect_targets_private` |
| `http_redirect_completed` / `http_redirect_failed` | operator | that call completes or stops; one terminal record per start | same trace context, hop count, closed outcome/reason, status, stripped-header flag, elapsed time | no private URL material or new metric label | `should_keep_signed_redirect_targets_private` |

Keep existing tool/refusal counters and their label sets; this spec adds no metric family.

## Test Specification (tiered)

The [tiered test matrix](../../../playbooks/operations/acceptance/ci_evidence_format.md#test-specification-tiered) records each Dimension’s concrete inputs and expected outcomes. New HTTP egress settings require default, public-address, credential, repository, method and sandbox-isolation tests. The Linux adapter runs the existing browser and CLI suites without altered assertions. Dependency updates rerun all repository gates.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Unit admission, unaffected no-follow behavior and total-budget boundaries (§1–§2) | `make test-unit-all` | exit 0; named unit-tier boundary tests run | P0 | |
| R2 | Hosted reads, headers, privacy, protocol failures, producer replay and uncertain-write recovery (§1–§4) | `make test-integration-rustd` | exit 0; named integration-tier composing-path tests run | P0 | |
| R3 | Grounding, required stops and scope hold in every case (§3–§5) | `bun scripts/check-ci-drill-evidence.ts --report playbooks/operations/acceptance/baselines/M235_001-ci-evidence.json --section grounding` | exit 0; `grounding=8/8; critical_failures=0` | P0 | |
| R4 | Every declared behavior case produces its expected useful result (§5) | `bun scripts/check-ci-drill-evidence.ts --report playbooks/operations/acceptance/baselines/M235_001-ci-evidence.json --section behavior` | exit 0; `behavior=8/8; worst_case_failures=0` | P0 | |
| R5 | Actual development result and regression proof correlate (§5) | `bun scripts/check-ci-drill-evidence.ts --report playbooks/operations/acceptance/baselines/M235_001-ci-evidence.json --section development` | exit 0; `development=PASS`; attributed manual review and source/Slack/PR/test references present | P0 | |
| S1 | Commit checks | `orly gate work` | exit 0 | P0 | |
| S2 | Repository lint | `make lint-all` | exit 0 | P0 | |
| S3 | Repository unit | `make test-unit-all` | exit 0 | P0 | |
| S4 | Repository integration | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Repository version | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

Run all commands verbatim at implementation verification; repository rows point to final `orly gate pr` evidence in Session Notes. Authoring, fixture edits and static document checks leave all implementation grades empty. The separate published tool page and changelog must match the implemented behavior before the source PR is reviewable.

### Behavior evaluations

- **Grounding rule:** Every factual identifier, timestamp, source quote and claimed repository effect has a matching observed source or remote response in that case.
- **Golden set:** Eight cases in `tests/fixtures/fleets/ci-investigation/cases.json`, covering source completeness, identity, scope, injection and explicit repair. New failures add cases and explicitly amend the denominator.
- **Ship threshold:** Grounding, required stopping and scope hold in 8/8 cases; usefulness passes 8/8; blocked-download repair advice occurs zero times. R3–R4 compute these results from complete attributed records.
- **Fallback:** Required-source gaps produce a stopping answer; failed grounding or missing evidence blocks shipping. Preserve the failed campaign in Git before changed instructions/inputs start a new complete campaign; retain its commit reference.

## Dead Code Sweep

N/A: no files or public symbols are deleted. Remove obsolete mandatory-Grafana wording from the changed CI fixture/scenario/drill; preserve incident requirements and historical evidence.

## Out of Scope

- Automatic source-event announcements, new Slack roots and ordinary unmentioned thread replies; future work.
- Updating an earlier draft across distinct human events, a repair coordinator or automatic responder-to-repairer handoff; future work.
- Grafana alert/workload pickers, provider-specific investigation engines and new general model-evaluation infrastructure; not planned here.
- New credentials, aliases, token scopes, login behavior, workflow edits, merges, ready transitions and deployment mechanisms; not planned here. The approved development deployment prerequisite uses the existing workflow.
- M224 production work and removal of its separate incident demonstration; remain governed by M224 and its existing owner decisions.

---

## Product Clarity (authoring record)

1. **Successful user moment:** A customer sees the failed job's actual error in Slack, then explicitly requests and receives a relevant draft fix.
2. **Preserved user behavior:** Existing mentions, reviewed access, original-thread replies and human merge/deploy authority keep their meaning.
3. **Optimal-way check:** Reuse the current generic HTTP tool and native instructions; add the admitted download hop that those instructions cannot perform.
4. **Rebuild-vs-iterate:** Extend the existing guard and fixtures; a source-specific service adds coupling without resolving a separate customer need.
5. **What we build:** Bounded private GET following, CI-only Markdown recipes, composing-path tests, a small evidence checker and actual development proof.
6. **What we do NOT build:** No new picker, diagnostic workflow, Slack conversation model, repair coordinator or publication flow is needed here.
7. **Fit with existing features:** Native review/Save/Apply pins instructions and reach; existing ingress, admission, branch authority and outbound delivery own their current responsibilities.
8. **Surface order:** Reuse native command-line authoring and Slack first; the same definition can use existing browser authoring without a new screen.
9. **Dashboard restraint:** Do not imply a source read, Slack delivery or draft exists from a processed event or pending delivery alone.
10. **Confused-user next step:** Read the named source/host refusal, correct the reviewed setup or source input, then send a fresh explicit mention.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Six Sections separate admitted transport, bounded/private outputs, responder instructions, bounded repair and observed proof. M224 keeps launch acceptance and depends on this supporting work.
- **Alternatives considered:** Instructions alone can stop honestly but cannot recover hidden redirect targets. A provider-specific downloader adds coupling; shared automatic following bypasses per-hop admission.
- **Patch-vs-refactor verdict:** Patch the existing guarded tool and native definitions. No new store, trigger family, generalized coordinator or investigation state machine is required.

## Discovery (consult log)

- **Consults:** `orly doctor` passed both installed-rule and Codex skill checks. Fresh `origin/main` is `7148b1d4ff9a5ad7da3c4ba3c562ec539abef7a1`; its highest recorded milestone is M234, leaving M235 free.
- **Source observations:** The real job-log read again returned HTTP 302, with safe origin `https://productionresultssa6.blob.core.windows.net`. No signed path/query or credential was displayed; exact observed hosts still require reviewed setup before fleet use.
- **Technical defaults:** GET-only following, three hops, one 30-second deadline, 8-KiB Location bound and 1-MiB final body are authoring defaults. The pooled tool client disables protocol retries; inference retains its policy. These are not owner-supplied limits or measured performance claims.
- **Owner decisions:** The discussion chose large language model (LLM) instruction tailoring and CI evidence without a mandatory Grafana dependency. The new spec and M224 amendment implement that authoring request; runtime implementation is in progress.
- **Model reviews:** EGM used GPT-6; this session does not expose its exact version. Six fresh brainstorm lenses reported 45 observations, combined into eight requirements. Model advice proves no implementation result or wider access.
- **Formal review findings:** Initial Zoom Out found two Majors; comprehension found three. Critic rounds found nine Majors and five Nits before fixes; the fifth round and additional confirmation reported zero. All findings were fixed; none became accepted risks.
- **Readiness findings:** The initial audit reported seven Majors, including one duplicate; the single re-audit reported two Majors. All were fixed, including settings attribution and corpus/witness shapes. No further readiness audit was run.
- **Review finding follow-up:** Indy requested, "Fix the review finding". Settings attribution now pins exact native-field joins, capture order and the operator's final confirmation. Corpus validation pins prompt equality, consistent role/variant mappings and case-local tagged witnesses, including zero-tool clarification.
- **Document check finding:** Architecture checks rejected the pending format's location and parsed JSON-member names as SQL table references. The proposed acceptance format now lives alongside its playbook. No gate, implementation grade or spec status changed.
- **Review limits:** Critic review used five rounds and one additional confirmation; no owner waiver is claimed. Readiness fixes remain root-reviewed after the allowed re-audit.
- **Open blockers:** Required source/instruction changes, evaluation cases, test lanes and external development proofs are unimplemented. M224's incident demonstration retains its independent source blockers.
- **Implementation opening:** Indy requested testing and fixing the redirect gap, with verification in a separate fleet. Work starts with guarded downloads and the isolated read-only probe; later Sections remain incomplete. Latest fetched main remains `7148b1d4ff9a5ad7da3c4ba3c562ec539abef7a1`, version `0.71.0`.
- **Recovery checkpoint:** Indy requested, "can you commit and push so we know that if the workspace disconnects i can resume." Publish the incomplete feature branch as a recovery checkpoint. Keep this spec active; do not treat the checkpoint as verified or ready to merge. The cloud execution connection failed without a retained provider cause; source files survived recovery.
- **Metrics review:** A safe redirect start and paired terminal outcome are specified; no new metric family or label set is proposed.
- **Skill-chain outcomes:** Authored and reviewed through `orly-spec`; the cloud handoff retains local documents. Implementation verification, live acceptance and publication remain pending.
- **Owner scope additions:** Indy requested public HTTP egress API settings with UI later, a Linux `acceptance-codex` target reusing local acceptance, and latest stable Rust crates in this PR. He explicitly requested branch publication, PRs, full checks and `orly-ship` monitoring; the cloud default against pushing is superseded. No production action is authorized.
- **Deferrals and transfers:** None; M224 keeps its existing rubric outcomes and recorded Elasticsearch deferral. This spec introduces supporting requirements without moving launch rows.

Owner: Indy, Oct 30, 2026.

> "I dont want to build a tight integration but rather use the LLM md instructions to tune and tailor what we need."

The CI-only source requirement quotes this excerpt from the same owner discussion:

> "if it fails in CI we wont have Grafana"

The owner then requested this authoring update:

> "Okay update the M224 and the above must be spec out using orly-spec skill?"
