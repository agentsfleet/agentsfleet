# M236 implementation evidence

## Current continuation point

### Source corrections after approved-gate verification — Nov 06, 2026

The first full integration run after approval reached db_suite with 48 passes and
two failures. The approved foreign-key check passed; the literal check then exposed
the event-history JSON envelope, and the instant check found the older lease
deadline column. Source corrections are complete and independently reviewed:

| Finding | Correction | Evidence boundary |
| --- | --- | --- |
| Lease deadline naming | Persist task_execution_deadline_at; update all six schema/query/test references while preserving public execution_deadline | Full rerun pending |
| History schema vocabulary | Store the entries as a JSONB array and sequence/truncation as typed columns; keep 128 entries and enforce the writer's 120,000-byte payload bound within 128 KiB public ceiling | No additional gate exception; live boundary regressions added |
| History reads/writes | Preserve slot/lease/event lock order and canonical sequence; reconstruct the existing public items/truncated response in detail/thread reads | Add nonempty readback and truncation/sequence regressions; pending execution |
| Exact-head substrate fixture | Replace stale completion_id with task_completion_id | Corrected ingress scenario passed in the full integration run |

The approved gate changes match their reviewed patches after formatting. Source
review found no remaining stale persisted SQL names or concrete history/concurrency
defect. Full `make lint-all` passed after the corrections. The required final PR
gate is next; no full integration or
coverage pass is claimed for this new batch. Keep the milestone IN_PROGRESS and
durable_tasks_v1 false. Existing exact ce815fe5 CI remains red as recorded below.

### Approved gate corrections — Nov 06, 2026

Kishore explicitly answered **“Approve both reviewed corrections”** for the two
independently reviewed proposals, which are now applied:
[composite foreign-key naming](M236_001-fk-gate-proposal.patch) and
[structural SQL literals](M236_001-literal-gate-proposal.patch).
This satisfies .orly/AGENTS.md's explicit session approval requirement for the
named afd_db integration_migrate/names.rs and literals.rs gate corrections.
The proposals retain ordinary identity-pointer naming, status/provider rejection,
database identifier-shape constraints, immutability triggers and ownership FKs.

Subsequent live integration passed the original decision lifecycle, tenant task
reads/controls/resume, authoring paging/concurrency and revocation cases. It then
stopped in db_suite (43 passed, seven failures): historical migration ranges,
SQL parameter typing, persisted names/timestamps, mutable person-link tracking,
and structural gate false positives. The two new API-role lock tests **passed**.
Source corrections address the implementation/fixture issues; only the two gate
proposals are now approved. The latest committed source batch passed `make lint-all` and `make check-version`
(all versions0.73.0), but has not passed full runtime verification. Full Rust diff coverage above99.5%, exact-head green CI and all final
PR-boundary checks remain required; no merge and durable_tasks_v1 remains false.

Exact ce815fe5 CI passed lint, unit, release/dry builds, runner/daemon coverage,
toolbox and Greptile (six threads, all resolved). Substrate failed on a stale
integration fixture reference to completion_id after the persisted column became
task_completion_id; that exact reference is corrected. Kernel reproducibility
failed while fetching pinned Debian packages with HTTP 503 backend errors; no
snapshot or transport policy was changed. These failures leave overall CI red.
Main was fetched again and remains 66059667, already an ancestor. Approved gate
regressions and the corrected fixture are proceeding through full integration.

### Latest live result — Nov 05, 2026

The tenant task-read, control and all three resume scenarios now pass after the
valid api_only target and served-ETag fixture corrections. The full integration
run then stopped in afd_authoring: 261 passed, seven failed. Source investigation
found edit-intent fixtures mutate the global model catalogue after pinning an
attempt, invalidating both that attempt and concurrent shared-database scenarios.
Fixture isolation/order correction is pending; production generation fences must
remain strict. The new lock migrations applied and the original API decision
lifecycle passed, but new lock-specific tests were not reached before this stop.
Full integration, Rust merged coverage, final PR gate and exact-head green CI remain
outstanding. No merge. This is a source checkpoint, not a verification claim.

### Live verification continuation — Nov 05, 2026, 21:00 UTC

Current pushed head remains 11b7def1f639fa3afd32522c777cfb9d59a351ae. Uncommitted
corrections add migrations 2630/2640 and shared production SQL lock constants.
Full lint passed on the lock correction. Local integration proved the original
decision lifecycle now succeeds under the API role, then exposed two fixture
issues: invalid `{}` task targets (now canonical `api_only`) and a hardcoded old
ETag (now obtained from the served GET). Strict production validation and stale-tag
rejection remain unchanged. Full integration is rerunning; no overall pass claimed.

Local integration requires the managed session CA mounted read-only into QStash.
The task-local override is `/workspace/scratch/m236-compose-ca.yml`, selected with
`COMPOSE_FILE=/workspace/agentsfleet/docker-compose.yml:/workspace/scratch/m236-compose-ca.yml`.
TLS remains enabled; repository/user setup files were not changed. Use command-local
`tini -s`, `umask 0022`, unset the three inherited nonexistent AGENTSFLEET_*_ENV_FILE
pointers and NEXT_PUBLIC_POSTHOG_ENABLED/HOST for gates. One local verification lane.

Next: finish integration and correct remaining findings, repeat affected lint,
run version/required orly PR boundary, staged secret scan, commit/push existing803,
then inspect exact-head CI and Greptile. The pinned snapshot original URLs recovered
without source changes. Full merged Rust coverage remains unproven. Do not merge
unless exact-head CI is green and every Greptile thread resolved. Feature stays off.

### Latest verification update — Nov 05, 2026

This table supersedes older pending results below. The current pushed source is
`11b7def1f639fa3afd32522c777cfb9d59a351ae`; subsequent native database-lock
privilege corrections are being verified before the next push. Main was fetched again and
remains `66059667bb8d2b12b9f6fb3a63a65a660789dca7`, already integrated.

| Work / evidence | Result | Limits and next action |
| --- | --- | --- |
| Implementation | Dedicated chat/CLI evidence, deduplicated byte accounting, ownership-safe reconnect history and bounded native handoff are implemented | Feature remains disabled; original approved HTML visuals unverified |
| Native deadline | Persisted lease ceiling reaches issuance, replay and recovery; stop new action groups at 180 seconds, bound drive and reply drain to reserve handoff time | Large capture can still fail truthfully; no guarantee of successful publication |
| Reuse / abstraction | Moved shared decision fixture into existing afd_admin test-util feature, replacing cross-crate test-file imports; typed error sources preserved | No new runtime crate or second execution path |
| Target validation | External wait-target integer overflow retains TASK_INVALID_TARGET; corrupt stored counters remain INTERNAL_DB_QUERY | Regression asserts error code, detail and source |
| Live decision fixture | Runner now reports required durable_tasks_v1 capability before leasing decision work | Test runner capability only; production feature stays false |
| Full local lint | make lint-all passed after shared-fixture, API wording, target-error and publication corrections | Full source rules, format, Clippy, dependencies, TypeScript, scripts and deploy checks; no suppression or threshold edits |
| App coverage | 483 suites, 4,702 tests; 100% statements, branches, functions and lines | Working-tree result, not browser acceptance |
| Website coverage | 20 suites, 133 tests; 100% in all four categories | Passed at 11b7def1f |
| Design-system coverage | 68 suites, 522 tests; 100% in all four categories | Passed at 11b7def1f |
| CLI coverage | 2,025 passed, 16 existing live-target skips, zero failures; 100% lines and functions | Skips are missing acceptance evidence, not passes |
| Full local unit | make test-unit-all passed at 11b7def1f: Rust runner 1,633, daemon 143, libraries 3,428 (5,204 total); all package coverage gates passed | Command-local tini and umask 0022 address container reaping/permissions; no test edits for those environment failures |
| Exact 11b7 CI | Lint, Rust unit jobs, runner and daemon coverage, app/CLI/website/design-system tests, dry lanes, bundles, governance and Greptile passed | Substrate plus toolbox/kernel failed; aggregate CI red, not mergeable |
| CI source failures | API wording, fixture dependency and durable capability corrections verified; live substrate then exposed workspace parent locking without required UPDATE privilege | Real production role mismatch; do not grant UPDATE on immutable rows |
| Native privilege correction | Exact-key workspace lock and bounded expired-completion selector use restricted security-definer functions; preserve workspace/fleet ordering and completion-parent before fanout-child locking | Independent source review passed; migration and API-role regressions pending runtime execution |
| CI toolbox / kernel | Repeated pinned-package 404s traced to cached snapshot HTML errors despite existing package bytes; all three original sampled URLs recovered at 20:22:34 UTC | No snapshot, transport or gate changes; next exact-head run must still verify kernel coverage |
| Greptile at 11b7 | Six threads, all resolved; review status SUCCESS | Recheck every new pushed head |
| Gate integrity | Independent focused configuration review found no weakened checks, new production exclusions or test suppressions | Rust patch floor 99.51%; TypeScript floors remain 100%; formal full gstack workflow is incomplete |
| Adversarial publication review | Fixed unchanged empty base gitlink blocking unrelated edits; preserve exact base gitlink/executable and refuse populated/deleted/replaced/staged-changed submodules or executable edits | Independently reread; all 11 real-Git cases passed within runner unit suite; tests execute the actual production capture program |
| Formal review boundary | gstack checklist/preamble and source subsets reviewed; audit records mark completed:false | Full 1,196-file diff, all specialists and runtime probes were not completed; no whole-diff certification claimed |
| Companion docs | 437f9c1952dd5203ab2e48cdcec5249786038676 pushed; docs make lint passed | Existing docs PR 241 predates continuation; no new PR created |
| Merge / milestone | No merge; M236 IN_PROGRESS | Requires green exact-head CI and all Greptile threads resolved; full Rust patch coverage still unproven |

Historical acceptance on main remains separate: dev acceptance-e2e and
acceptance-cli passed on 66059667; production acceptance failed before scenarios
with api.agentsfleet.net DNS resolution. Neither verifies this branch.

### Active continuation — Nov 05, 2026 (supersedes execution restrictions below)

Kishore explicitly authorized gates after implementation, updating from main before
gates, pushing existing PR #803, and merging only on green exact-head CI and all
Greptile threads resolved. Acceptance inability may be reported without blocking
that conditional merge. No new PR; keep M236 IN_PROGRESS and durable_tasks_v1 false
until verification. The older no-gates/no-merge handoff below remains historical.

Existing clones and setup were retained. Feature ancestry was verified for
9d826de4d3b4e449fc1fee17ff5ca173d1c13708,
6bb180dcd9851ed6952ebcfbcb2acef9e09a8892 and
6cacc80c8cb28ca3863faa31793697838fd8712b. Companion docs ancestry was verified
at b535f3aed85d981446f666a701b62c5c7d5b89c5. Main integration includes
66059667bb8d2b12b9f6fb3a63a65a660789dca7 (#808, with #807/#806 ancestors);
setup admission SQL comes from main without local edits. Fetch again before gates.

| Area | Implementation and review outcome | Evidence boundary |
| --- | --- | --- |
| Chat/CLI presentation | Dedicated local diffs, published-head receipt, active/saved check results; existing steer command only | Source complete; runtime verification pending |
| Transfer progress | One concrete operation owner deduplicates missing/acknowledged/reused ciphertext bytes; terminal counters survive partial failure | Regression fixtures authored, not yet run |
| Reconnect | Daemon observation sequence, canonical live frames, bounded saved history, shared CLI stream/poll printers | Ownership race corrected in write SQL; integration fixtures pending execution |
| Publication/JSON | Private Selection method call replaced with public tool lookup; all CLI live progress suppressed in JSON output | Both initial Greptile P1 threads resolved on GitHub; exact-head gates still required |
| Refactoring | Reuse jsdiff9 and Node terminal sanitizer; existing crypto, capability filesystem and native receipt helpers retained | Codex review found no justification for broad trait/generic indirection |
| Native review | Decision receipts, execution, recovery, publication and authority reviewed; partial download successes now counted before returning error | Source review does not prove runtime behavior |
| UI design | Existing separate setup/work chat, Save done/Open fleet, inline expanding Current work preserved | Original HTML approval visuals remain unverified |
| Documentation | Architecture and companion CLI/fleet docs describe evidence semantics | Companion commit 437f9c1 pushed; docs lint passed |

Current continuation verification (working tree; not exact-head CI):

| Check | Observed result | Follow-up |
| --- | --- | --- |
| orly doctor / gate work | Passed; installed rules match, four verification commands declared | Repeat relevant final gates after final source changes |
| CLI build | Passed | Full CLI coverage remains pending |
| source rules | 44 rule tests passed; repository scan clean after replacing test SQL file reads with shared production helpers | No gate or suppression edits |
| app/design-system/CLI lint | All three passed, including TypeScript checks and CLI runtime/constant audits | Repeat after latest fixtures and command-scope correction |
| Rust formatting | Completed; also formats previously unformatted checkpoint source | Repeat after compile corrections |
| Rust all-feature workspace check | Passed cargo check --workspace --all-features --all-targets after validation, visibility, borrowing, dependency, fixture and ETag corrections | Compilation only; execution and coverage pending |
| dependency locks / OpenAPI | CLI and native locks generated; daemon all-feature build passed and --no-banner openapi generated 123 paths | Runtime acceptance pending |
| directory secret scan | Findings only in generated dependency .rmeta files under rustd/target | Stage-source scan is the commit boundary; no allowlist edits |

App coverage rerun with command-local environment corrections: 477 suites and
4,676 tests passed; coverage remains below the unchanged 100% floor (lines
99.41%, branches 98.88%). Added regression fixtures remain pending rerun.
The first run's 30 failures came from inherited nonexistent environment-file
pointers and disabled analytics; no user setup files were rewritten.

Command-tree audit found older checkpoint task subcommands still registered.
They and unused control handlers/arguments were removed: the command tree now
matches main, and only existing steer receives richer presentation.

Independent standards review found three SQL findings: two statements used a
volatile clock despite SKIP LOCKED, and retention budgets were SQL literals.
All were corrected and independently rechecked; no gate was weakened.

Historical evidence, not verification of this branch:
- Initial head 9d826de: Vercel statuses passed, smoke workflows skipped; no green
  exact-head CI proof. Greptile 3/5 with two open P1 threads.
- Main 66059667 dev run 42059734556: acceptance-e2e job127362832634 and
  acceptance-cli job127362844747 succeeded.
- Production run 42059935851 job127358457696 failed before scenarios in
  fixtures/bootstrap.ts:79: getaddrinfo ENOTFOUND api.agentsfleet.net.
  This is DNS setup evidence, not a tested M236 product defect.


### Verification follow-up — Nov 05, 2026

Implementation and main integration were committed and pushed as
`8e013b683b87fb70f4a2b9aa67e46d9302b51cf0` to existing PR #803.
A later fetch confirmed no additional main commits at that point. Companion docs
were committed and pushed as `437f9c1`; `make lint` passed its 25 tests, OpenAPI
drift, documentation, Mint validation and broken-link checks. Docs PR #241 was
already merged before these continuation commits; no new PR was created.

| Evidence at source checkpoint 8e013b6 | Result | Correction / remaining work |
| --- | --- | --- |
| Governance and staged secret scan | Passed | No bypass, allowlist or gate-threshold changes |
| Exact-head continuous integration | Failed; do not merge | Working-tree corrections below need a new pushed head and green results |
| CLI tests | Old fixtures failed after saved-work readback | Preserve exact POST and subsequent GET assertions; fixture corrections authored |
| App route bundle | New-fleet route 102.84 kB, limit 102.4 kB | Load optional instructions editor dynamically; size rerun outstanding |
| Event history integration | Tool-trace history request failed | Both admission UNION arms lacked task_activity; add empty history plus first/subsequent-page assertions |
| Runner ordering regression | Reply unavailable before provider call | Successful fixture omitted resolved model context; supply it and separately prove zero-context fail-closed behavior |
| App ingress unit / integration fixtures | Full workflow began reading real task store; stale Slack actor expectation | Use genuine pull-request fanout fixture, explicit completion-store outage and live replay evidence; assert team-scoped Slack actor |
| Native Clippy | Source lint findings | Correct imports, typed errors, validation callbacks and focused abstractions without suppressions; rerun pending |
| Greptile | Prior two threads resolved; new long-task handoff P1 open | Carry persisted execution deadline through delivery, binding and recovery; native reviewer implements bounded handoff reserve |
| TypeScript coverage follow-up | 4692 passed, six failed before coverage emitted | Five editor-readiness fixtures and mutation-origin fixture corrected; rerun ongoing |

The app rerun subsequently passed all 483 suites / 4,698 tests. Coverage reached
100% lines and functions, 99.95% statements (12,866/12,872) and 99.92% branches
(8,936/8,943). Added regressions target the remaining repeated-activation,
subscriber-ownership, terminal-work and activity-order branches. A later run exposed
one direct editor test still awaiting the dialog shell instead of the editor;
that assertion now awaits the editor itself. No thresholds changed.

The independent deadline review caught final reply delivery running outside the
agent timeout. The final reply drain now uses the same remaining execution budget,
keeps prior answer text and clears completion authority when acknowledgement fails.
Release CI also exposed a test-feature-only export of the production checkpoint
guard; its one public export now works in release builds without duplicated SQL.

Latest native verification: `cargo clippy --workspace --all-targets --all-features
--keep-going -- -D warnings` passed (12.50s). The daemon all-feature build and
OpenAPI generation also passed after deadline wiring. The generated document
still contains 123 paths. These are working-tree checks, not green exact-head CI.

App coverage now passes every 100% threshold: 483 suites / 4,702 tests,
12,872 statements, 8,941 branches, 3,430 functions and 10,622 lines.
Website coverage also passes every 100% threshold: 20 suites / 133 tests.
CLI's next stage had 2,019 passes, 16 existing live-target skips and two failures
because its stream fixture counted the new saved-work readback as polling;
the fixture now asserts separate readback and fallback-poll counts. Targeted
regressions address the remaining 99.85% CLI line coverage; rerun is pending.

The previous exact-head kernel coverage job 127443119458 was cancelled at its
40-minute timeout during toolbox restore/build, before kernel measurement.
That is missing verification, not a passing acceptance run.

The deadline work has compiled and passed Clippy; runtime behavior is not yet verified. Exact-head
CI, 100% TypeScript coverage, Rust diff coverage above 99.5%, generated OpenAPI
after the new wire fields, full gates and Greptile resolution remain outstanding.
The feature remains disabled. No merge has occurred.

### New cloud-agent handoff — Nov 05, 2026

Kishore requested a committed context save and a prompt for a new cloud session.
The complete source checkpoint is **`6bb180dcd9851ed6952ebcfbcb2acef9e09a8892`**,
already pushed to `agentsfleet/agentsfleet`, branch `feat/m236-durable-tasks`.
It descends from the required earlier checkpoint
`6cacc80c8cb28ca3863faa31793697838fd8712b`. This context-only commit follows it.
Use the existing open, undrafted PR **#803**; no new PR and no merge.
The source checkpoint contains 172 changed files, including the new source and
unrun fixtures; nothing needs recovery from an archive or another worktree.

Companion docs are pushed at **`b535f3aed85d981446f666a701b62c5c7d5b89c5`** on
`agentsfleet/docs` branch `chore/m236-durable-tasks-changelog`. Both checkouts were
clean after those pushes. The prior session used `/workspace/agentsfleet` and
`/workspace/docs`; locate the actual clones in the new environment and preserve
their setup. Ordinary Git commits/pushes were used; no hook configuration was
changed or bypassed. No gates ran for this continuation.

**Execution order remains code first.** Do not run builds, tests, lint, Clippy,
formatters, coverage, browser acceptance or other gates, including narrow checks,
until implementation is finished and explicit signoff is received. Read existing
exact-head CI and Greptile results first; do not launch or rerun workflows. Earlier
passing evidence below is historical and does not verify this checkpoint. Keep
M236 `IN_PROGRESS` and `durable_tasks_v1` false. When code is complete, give Indy the
detailed implementation report, identify remaining gaps, and request explicit gate
signoff from Kishore, the owner named by the latest delegation. Do not infer that
checkpoint permission approves gates. Do not merge even after gates without new
authorization; the latest delegation explicitly prohibits merging.

**Immediate remaining implementation:**

1. Finish the agreed Codex-style activity presentation in both chat and CLI:
   dedicated file-diff, check-result, exact published-head and wait/dependency
   presentation. Current source has decision/recovery stages and command details;
   tool output alone does not complete the richer agreed presentation.
2. Complete transfer progress with acknowledged bytes, known missing bytes and
   reused bytes separately; preserve distinct preparation/scanning/upload/
   verification/restore stages and bounded update cadence. Current frames report
   object counts. A saved state requires native manifest commit, and retries must
   not inflate progress or billing.
3. Preserve the approved chat design. Setup and work are separate conversations;
   keep the centered `Save done vN → Open fleet` pill and `Current work` inline
   above the composer, expanding in place on desktop and mobile. Use existing
   Instrument Sans/tokens and 14px conversation, 13px work labels, 12px secondary
   text. References: `docs/architecture/durable_tasks.md` → “Work panel and
   notifications”, `docs/DESIGN_SYSTEM.md`, and the approval history below.
   Written approval is retained; this session did **not** locate/verify a copy of
   the original approved HTML mock visuals or render the new UI. Locate the
   originals for the eventual comparison; do not call a fresh mock approved.
4. Finish manual source compatibility and adversarial review across native task
   coordination, physical provider receipts, R2 capture/restore and publication.
   Review source-reply ordering, cancellation/acknowledgement loss, unknown effects,
   retained bases after a new head, current Slack membership/installation, and
   hostile file/link graphs. Review bounded activity-history merging when live and
   saved windows differ: old entries must not displace newer durable progress.
   New fixtures are unrun, and full-flow provider/R2/replacement/publication proof
   is still outstanding. Do not report fixture authorship as acceptance.
5. Keep optimizing PostgreSQL within the existing admission/lease/report path.
   Sender checks now deduplicate identities, manifest layout avoids repeated
   full-page serialization, and uploads batch up to 256 small objects within a
   four-chunk byte budget with four physical requests at once. Actual query plans,
   timing and concurrency evidence await approved verification.
6. Complete corresponding documentation as the remaining activity behavior lands.
   Cargo.lock and generated `public/openapi.json` remain outstanding; OpenAPI must
   be generated by the native binary after build approval, never hand-edited.

**CLI status:** this checkpoint changes the output of
`agentsfleet steer <FLEET_ID> [MESSAGE]`; it adds no command names or flags.
It prints selected decision provider/model, command arguments/status/output/exit
code/duration, recovery progress, saved-event readback and the final reply.
Reconnect reads that exact event without resending the user's request. Private
reasoning and recovery secrets remain outside this public activity.

**Preservation boundaries:** retain user-authored setup. Do not restore the five
deleted `afd_authoring/src/model/provider/prompt/*.md` files, relocate their prose
into replacement static templates, or edit setup-chat admission SQL owned by
`fix/soak-findings`. Keep existing M236_002/M236_003 and M239 work and the M240
signal-audit scope recorded in the active spec; do not reopen M237 ownership.
Use one Rust lane only after approval, with the resource settings in `.orly/LOCAL.md`.
No weakened gates: eventual Rust diff coverage must exceed 99.5%, TypeScript must
reach 100%, acceptance must cover full scope, and exact-head CI and Greptile must
be green/resolved. The retained-base Greptile thread `PRRT_kwDOReIoss60FGFr` was
still unresolved at handoff; the source fix has not been verified.

### Continuation source work — Nov 05, 2026

**Requested Git checkpoint:** Kishore asked to commit and push the current files
after a status report. This checkpoint saves the entire continuation source,
including unrun fixtures. It is not implementation signoff or permission to run
gates. Continue on `feat/m236-durable-tasks` and existing undrafted PR #803;
M236 remains `IN_PROGRESS`. Do not merge. No archive restoration is needed.
Companion documentation is checkpointed at `b535f3a` in `agentsfleet/docs`, on
the existing `chore/m236-durable-tasks-changelog` branch; no new PR was created.

**Latest instruction: finish all implementation before running any gates.** The
latest delegation names Kishore as the explicit gate-signoff owner. Indy must
receive the detailed implementation report. Tests, builds, lint, Clippy, formatting
and coverage remain paused; no signoff has been received.
Do not mistake the source inventory below for completion or passing evidence.

Implementation source added since the previous checkpoint:

- Physical decisions now reserve through native authority, persist encrypted
  uncertainty before dispatch, and retain known receipts before exposing an answer.
  The existing report drainer replays original-owner receipts. A bounded 429/529
  retry retains the original logical deadline and native quote.
- The root coordinator prepares complete intent before sandbox creation, recovers
  accepted plans, applies queued input in order, acknowledges original sources,
  reconciles publication receipts, and assesses complete action groups. Unavailable
  initial intent reads back native binding before returning clarification without
  tools. Large groups use the existing private context owner and a bounded coding
  summary with a complete-group digest; native completion checks remain independent.
- Root-only `propose_change` validates a complete candidate tree in the sandbox,
  compares it before and after validation, and uses the existing native exact-head
  publisher. Children cannot select publication authority. Subsequent publication
  waits for original-effect reconciliation at the next complete action boundary.
- Supervisor recovery is wired into exact-target checkout, frozen initial-base
  capture, fresh-workspace restore, private conversation restoration and final
  snapshot handoff. Uploads share native authorization for up to 256 small objects
  within a four-chunk byte budget; physical concurrency and restore batches stay at four.
  Capture verifies native acknowledgements. Failed runs can save recovery before
  teardown; failed freeze/history/capture cannot retain completion authority.
  Durable workspaces are destroyed after capture, rather than retained as warm holds.
- Retained bases remain usable after publication advances the native head. A
  reused base supplies its authenticated manifest paths and dimensions. Adopted
  goals retain authenticated task history as data while current native criteria
  control subsequent execution. Current secret policy scrubs restored history.
- Subject-checked Slack OAuth links a proved external person to a current workspace
  member. Signed sender text stays separate from quoted thread context. Native
  execution checks membership, current installation and retained source authority.
  Disconnect cascades person links; provider display names and quoted text grant
  no authority. Slots 2610 and 2620 are registered; the role inventory has 79 rows.
- Public decision/recovery activity has typed bounded frames, bounded persistence
  on the existing event, chat expansion and CLI command/result/readback output.
  Native private reasoning, prompts, credentials and checkpoint keys are excluded.
  Reconnect merges saved progress while preserving newer live counters.

The final source review also changed these boundaries:

- The root rereads the oldest unsettled source page until mixed-item replies are
  acknowledged; advancing its message cursor could previously strand a reply.
  The separate task-input incorporation sequence still advances monotonically.
  A final reply-acknowledgement failure clears any goal completion request.
- Slack task authorization deduplicates original and incorporated senders before
  consulting current membership/installation. Existing task/sequence indexes scope
  that read. Actual PostgreSQL query plans and measurements remain unrun.
- Manifest construction counts each encoded entry once per layout pass instead of
  repeatedly serializing the growing page. R2 transport failures retain their
  causal error after removing the presigned URL; failed PUTs still require native
  reconciliation. Receipt directory and parent fsyncs precede physical dispatch.
- Coalesced stream-gap recovery reads running-event detail even when its event
  predates the list watermark or resumed traffic prevents the silence sweep.
  It shares in-flight detail reads and preserves newer live progress.
- Added unrun regressions for uncertain intent, original-source reply ordering,
  decision replay without redispatch, crash receipts, scoped checkpoint encryption,
  large manifests, hostile file/link replacement, retained bases after publication,
  Slack revocation/reinstall, and chat/CLI progress readback.

Implementation review and fixture work are still in progress. Remaining work includes
source-level compatibility review, adversarial scenarios, companion documentation,
and verification-only generated artifacts (Cargo lockfile/OpenAPI) after approval.
Do not report this inventory as completion, resolve Greptile, enable the capability,
or merge. No build, test, lint, Clippy, formatting or other gate has run.
Latest delegated instruction names Kishore as the explicit gate signoff owner and
prohibits merging. Indy still needs the implementation report once the source batch
is complete. Preserve all historical evidence below independently of this batch.

The current checkout is `/workspace/agentsfleet` on `feat/m236-durable-tasks`.
Fetched the branch and verified the starting HEAD was `6cacc80c8cb28ca3863faa31793697838fd8712b`.
The requested continuation checkpoint descends from that commit.
`orly doctor` passed: installed rules match and Codex agent tooling is present.
The runtime initially refused shell access while starting; it is now connected.
The existing clones and user-authored setup were preserved. No archive was restored.

The starting head's `gh run list` returned only three skipped post-deploy smoke runs:
`42032502470`, `42032424354` and `42032388350`. This is not green full verification.
Greptile reviewed that starting head and left unresolved thread `PRRT_kwDOReIoss60FGFr`
on retained-base selection after publication. The two historical threads remain resolved.
PR #803 is attached to this chat and remains open and undrafted.

Local source now separates retained-base selection from the current checkpoint head.
Selection retains workspace, repository, format, toolbox, readiness and retirement checks;
the existing descriptor still returns the independently stored `base_commit_oid`.
Plan creation now requires the current native repository head for incremental captures too.
The existing report head predicate is unchanged.

An unrun native storage scenario under `integration_tasks/decisions/checkpoint.rs`
allocates and commits an empty retained base, simulates a confirmed publication's head
update, commits/replays an incremental checkpoint and checks the real report predicate.
It also checks missing/incompatible bases and missing/stale captured heads, with no
checkpoint allocation on refusals. This fixture does not prove physical publication,
file transfer or restore. Its bounded HTTP client is configured but makes no storage call.

These checkpoint edits are uncompiled and unverified. The Cargo lockfile still needs
regeneration for the added test dependency alongside the checkpoint's other dependency changes.
No broad lint, formatter, build, unit or integration lane has run. PostgreSQL review,
meaningful runtime proof and the remaining implementation below are still due.
Do not resolve the Greptile thread or treat this source correction as passing evidence yet.

### Branch checkpoint for a new cloud session — Nov 05, 2026

Indy requested all files committed and pushed so a new agent can start from this branch.
This checkpoint saves **unfinished, uncompiled and unverified source**. M236 remains `IN_PROGRESS`.
Indy also directed: "undraft it. Indy knows its a draft". Remove draft status after pushing this checkpoint.
Full source, acceptance, coverage and final checks remain due.

**Resume from the branch checkpoint containing this record.** Use `feat/m236-durable-tasks`, existing
PR `https://github.com/agentsfleet/agentsfleet/pull/803`. Fetch and confirm the latest branch head before editing.
The predecessor published head was `ebfb3b722a911600bb00e9677e4f2b003e2169c7`.
This checkpoint includes all local implementation files and the pending merge from
`65372f50551c1585afdc499e2db7a833c58d98c9`.
The merge's sole documentation conflict was resolved before this checkpoint.
Keep its M239 spec and fleet-to-fleet scenario alongside the M236 changes.
The old worktree was `/workspace/agentsfleet-m236-design`; use the new session's actual checkout path.
A fresh checkout of this checkpoint needs no source recovery archive or unfinished-merge restoration.
The earlier archive remains an optional backup of predecessor source and historical logs.

**Read next:** `AGENTS.md`, `.orly/LOCAL.md`, `.orly/AGENTS.md`, the active
`docs/v2/active/M236_001_P0_API_DOCS_INFRA_OBS_UI_GOAL_DRIVEN_DURABLE_REPAIR.md`,
this continuation point, and the latest source inventories at the end of this file.
Read the owning Rust/error/SQL/HTTP/auth/architecture standards and `orly-build` skill before edits.
Read the managed cloud environment runtime skill before environment work in a new session.
Then check exact-head Continuous Integration (CI) and Greptile without re-running unchanged tests.

**Owner instructions that remain binding:**

- Finish the full approved M236 scope in PR #803. One existing admission/lease/report path,
  one Rust lane; no new PR, duplicate task queue, weakened gate, hook bypass or force push.
- "i need the code complete first" and "Yes don't runt he gates now, run it after the PostgreSQL is done."
  Broad lint/unit/format/integration loops remain paused. Complete the source batch before final gates.
- After each fix, adversarially reconsider correctness, simplicity, performance, concurrency,
  ownership, traits/trait objects, callbacks, tracing and causal `error_shell!` errors.
  Avoid adding shared mutexes. SQL performance requires actual statistics and
  `EXPLAIN (ANALYZE, BUFFERS)` evidence; source/index inspection is not a benchmark.
- User-authored setup: the five `afd_authoring/src/model/provider/prompt/*.md` files are deleted.
  Do not relocate their full prose into static Rust strings or another template directory.
  User text remains in immutable accepted setup revisions; Review pull requests supplies an
  editable, unsent starter in the chat composer. Minimal native protocol/schema guidance remains.
- Setup questions must not propose edits. The native configured decision model asks the exact
  newest-message yes/no edit-intent question; no rejects through `ProposalRejection`.
  Unavailable/stale/malformed decisions retain chat behavior and record the fallback/SetupChat usage.
  The decision model never authors setup questions. Existing source/soak regressions are unrun.
- Do not edit setup-chat admission SQL: `fix/soak-findings` owns its usage-reconciliation correction.
  Fleet admission SQL under `afd_admission` is a different owner and is in scope here.
- Chat and CLI must show detailed readable activity like the Codex app: progress explanations,
  expandable commands/output/diffs, checks, publication/waits, transfer/recovery and reconnect state.
  Source for that complete activity presentation is still missing.
- Merge only after full scope/acceptance, Rust diff coverage **above 99.5%** (configured gate 99.51),
  TypeScript **100%**, green exact-head CI and resolved Greptile. Deliver Indy a detailed final report.

**Exact source boundary at handoff:**

| Owner | Written source | Still missing / unverified |
|---|---|---|
| PostgreSQL and admission | Slots through 2600 are explicitly registered; role inventory has 78 rows. Native generations, cumulative usage, current lease deadlines, report transitions, wait ownership, immutable receipts and bounded completion delivery are written. | All new migrations/queries are unexecuted. Historical generation-null row recovery must not guess authority. Real plans, contention, cancellation and financial cases remain due. |
| Automatic task runtime | Native binding/input/receipt/decision/effect APIs and typed supervisor calls exist. | No automatic root intent/action/goal coordinator. `afr_agent/src/engine/mod.rs::RunOutput` carries no task outcome; supervisor report still sets `task: None`. |
| Decision execution | Existing `afr_providers::decision::DecisionClient` implements four guarded native protocols; native reservations and original-owner receipt accounting exist. | `FencedVerbs` has no physical decision implementation; its trait default refuses. Known receipt durability/replay and root connection remain unfinished. `afr_providers` is still a supervisor dev dependency, not a normal dependency. |
| GitHub | `afd_fleet/src/task/{github,effects,waits}/` contains native exact existing-PR authority, reviewed workflows, tested-tree/expected-head publication, retained effects, read-only uncertain-write reconciliation and signed completion matching. | Automatic runtime integration and adversarial execution/acceptance proof. Preserve human pushes, one physical mutation, one wake and no restart after stop. |
| R2 | `afd_fleet/src/task/{storage,checkpoint}/`, `afd_wire/src/task/checkpoint/` and runner APIs contain SDK transport, encryption, registration, quotas, manifests, verification, pins, retention and collection. | No actual supervisor capture/restore. No live R2 activation, hostile restore, large-repository or performance acceptance. |
| Inputs and authority | Mixed-message binding, accepted interpretation recovery and per-item reply/application receipts exist. Goal adoption and controls retain native generations. | Slack person-link authorization still missing. Use existing identity/link approval storage; do not fabricate a person or add keyword routing. |
| Browser and CLI | Conditional task controls and explicit immutable-goal adoption are written. | Detailed activity, updated decoding/fixtures and current TypeScript coverage/acceptance. |

The runner deliberately still advertises `durable_tasks_v1: false` in
`afr_supervisor/src/capability/mod.rs`. Keep it false until its real coordinator and recovery work.
New API source does not prove a running durable task.

**Latest reviewed corrections; source-only evidence:**

- Admission slot 2590 freezes task generation. Discovery, claim and issue require it; the retained
  marker prevents a deleted task's old admission becoming ordinary work. Never-issued obsolete
  native continuations retire atomically. Backlog budget/telemetry queries exclude issued obsolete
  native continuations while retaining their unsettled admission marker for final billing/report
  settlement. Human reply sources remain counted. The reviewer confirmed why retiring all issued
  markers would break ordinary final metering.
- Setup mismatch blocking re-runs `installed_on` under existing parent/slot locks. Actual setup
  writers hold the corresponding parent lock; a restored compatible setup avoids stale blocking.
- Slot 2600 retains a progress cursor over immutable completion evidence. Signed HTTP ingress only
  inserts receipt/cursor. Maintenance examines at most 64 scoped waits per pass and locks one wait
  per transaction. It advances over irrelevant workflows/fleet scopes too. Scope predicates apply
  to the signal mutation. Its index includes the equality prefix plus `id`; keyset range uses the
  saved wait identity or UUID's lower bound. A busy wait cannot be skipped and lose its signal.
- Wait claims use a separate `claim_expires_at`, not the due time a signal can move. Signal and
  observed-signal counters are monotonic, avoiding same-millisecond wake loss. New workflow run
  numbers and same-run non-older attempts match; native independent reads remain authoritative.
- Effects now retain the complete private canonical request whose exact serialized bytes form
  the immutable digest. No arbitrary acknowledgement means a second write is safe.
- `afr_supervisor/src/client/task/{mod,checkpoint}.rs` adds the 17 approved typed task method/path
  calls through existing `ControlPlane` / `RunnerApi`; the closed `Verb` list has 41 entries.
  `FencedVerbs` implements inputs/bindings/input-receipt/publication/effect-list with its own fence.
  `LeaseVerbs::task_decision` remains a refusing default. `DecisionEvaluation` carries no key.
  Input/binding `into_owned` helpers preserve native recovery fields after the private body drops.
- The read-only standards reviewer found **no remaining actionable findings in those corrections
  and typed-call additions**. That is a limited source review, not a whole-diff or runtime pass.
  No build, formatter, lint, unit, integration or acceptance command ran in this final source batch.

**Resume implementation in this order:**

1. Inspect the actual source at the listed boundary, keep the reviewed PostgreSQL corrections,
   and finish any concrete schema/query/constructor issues without broad gate loops.
2. Implement physical decision execution plus durable original-owner receipt delivery through
   existing supervisor boundaries. Reuse `DecisionClient`, guarded transport, private wiping
   buffers and existing report-spool conventions; no second decision SDK or runtime interface.
3. Connect root intent before checkout/sandbox creation and action/goal checks at safe boundaries.
   `afr_supervisor/src/lease_loop/mod.rs::work` currently admits tools, claims the fleet turn,
   hydrates memory and builds/checks out beside the agent. Routing solely inside `Loop::run`
   is too late to avoid a sandbox for conversation. Extend the actual existing engine/LeaseVerbs
   seams. Children never own task state. Recover accepted meanings/replies without reclassification.
4. Implement quiescent incremental capture and verified supervisor restore. Reuse the existing
   executor capability-directory/no-follow protections. Preserve Git/index, staged/unstaged and
   untracked changes, deletes, modes, safe relative symlinks and compatible history as one state.
   Check all roots/format/toolbox/repository/base/digests, authenticated encryption and plaintext sizes before writable
   attachment. Existing file RPCs are whole-file and listing truncates at 4096: a safe trusted
   bounded snapshot/transfer seam still needs implementation. Recover native effects beyond the
   saved watermark before another publication. Prove partial/stale/hostile restores fail safely.
5. Close Slack linked-person authority and chat/CLI activity gaps, propagate actual task reports,
   route/OpenAPI/fixture inventories and source conformance. Update the owning public docs.
6. Complete meaningful adversarial coverage and declared acceptance. Then run final repository
   commands with complete exit codes/counts: `make lint-all`, `make test-unit-all`,
   `make test-integration-rustd`, version/dry/browser checks and exact-head hosted coverage/review.
   Preserve every historical result below; repeat unchanged successful lanes only for a relevant
   source change, concrete failure or required final gate. No readiness or merge claim beforehand.

**Historical evidence to carry into the new environment:**

- Exact published head `ebfb3b722` still has successful governance `41948469226`, gitleaks
  `41948469131`, lint `41948469170`, dry-smoke `41948469255`, dry `41948469204` and test
  `41948469238`. Rust unit `41948469205` is red because daemon-library startup found an occupied
  daemon port; runner/daemon/lint/secret-wiping shards passed. Integration `41948469274` is red
  because the kernel builder received Debian Snapshot HTTP 504; runner/daemon/substrate jobs
  passed, but no complete combined exact-head Rust grade exists. Do not call this green CI.
- Both existing Greptile inline threads are resolved. Latest recorded summary is for `c8937f369`,
  not proof of review on unpublished work. Fresh review is required after the eventual push.
- Recovered historical coverage is `/tmp/m236-coverage-c8937f369/`: patch 5506/5623 = 97.9193%,
  117 unhit changed lines; workspace 83137/83452 = 99.6225%; 32/194 files below their file floor.
  All four revision sidecars name `d12a443729916f22f8c667afca8062974a8c8f0b`; its tree matches
  `c8937f369`. The original parser reproduced the grade. Current source is ungraded.
- Historical TypeScript 100% is preserved; it does not grade current changed TypeScript.
- `/tmp/m236-controls-lint-final.log` passed on older source; `/tmp/m236-controls-unit.log`
  records 3415 passed. The focused core run `/tmp/m236-controls-focused-unit.log` had 76 passed,
  1 failed from adding a duplicate 428 code; that duplicate was removed in favor of the existing
  shared precondition code, without weakening the registry. No passing rerun is claimed.
- `/tmp/m236-controls-full-rust-unit.log` has runner 1604 passed, daemon 143 passed, then a
  daemon-library linker disk-full failure. `/tmp/m236-prompts-lint-rustd.log` was deliberately
  stopped on Indy's instruction: partial output is not a check result. Kernel failure log is
  `/tmp/m236-head-kernel.log.gz`. Recovery archives retain the available files separately.

**Environment and process handoff:**

- Shell commands use `login:false`. One Rust lane: `CARGO_PROFILE_DEV_DEBUG=0`,
  `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`.
  The approved workflow resource changes are already published; do not broaden CI edits.
- Current process inspection found no live cargo/make/rustfmt/clippy/test command. Only existing
  datastore/dev containers remain. There is no build or background implementation to await.
  The source reviewer is idle/completed and holds no implementation lane.
- The old environment has 32 GiB disk. Targeted worktree `cargo clean` reclaimed disk earlier;
  preserve logs and dependency caches. Do not assume caches/datastores survive a new session.
- Before final gates, follow the repository's Make targets and ports script. The last local
  infrastructure used base port 27836. Source `scripts/test-infra-ports.sh` once in the actual
  gate shell; do not print secrets. The QStash CA workaround is an environment-only compose
  overlay `/tmp/m236-compose-ca.yml`; it is not a repository compose change. Use umask 022.
  The child-reaping helper is `/workspace/.cloud-tools/reap-command.py`; cargo-shear 1.14 was
  installed at `/tmp/m236-tools/bin/cargo-shear`. Recreate environment tools if missing.
- Root/CLI/UI Bun installs/build were done in this environment. A fresh environment must follow
  `.orly/LOCAL.md` setup; do not count dependency bootstrap as a product test.
- `/workspace/docs` is clean on its own `chore/m236-task-controls-changelog` branch. No new public
  docs were written in this continuation. Preserve the docs repository's own-branch rule.
- No resolved platform/GitHub/R2 secrets belong in a handoff archive. The private development
  bucket is `agentsfleet-checkpoints-dev`; resolve authorized secret references at runtime.
  Native activation and acceptance remain unproved. Re-check tools/credentials in the next cloud.
- Attaching PR #803 with the app tool has encountered transport failures; no successful app
  attachment is claimed. GitHub metadata/status/review reads work through the connector.

The rest of this continuation point and historical sections describe earlier local checkpoints.
Their uncommitted-state and running-command statements are historical; the branch checkpoint above owns the current resume instructions.

The current user direction is **finish source before broad gates**. Do not restart unit/lint/format/integration loops for each edit.
Indy subsequently specified: "Yes don't runt he gates now, run it after the PostgreSQL is done."
Complete PostgreSQL schema/query/index and concurrency corrections before any gate. Each completed item receives adversarial source review for simpler ownership, performance, recovery, traits/callback boundaries, tracing and causal errors; avoid adding shared mutexes.
The last lint process was deliberately stopped on that instruction; its partial log is not a check result.
All historical passing evidence below remains intact and does not grade the current uncommitted implementation.
The checkout has an uncommitted merge from `origin/main` (`65372f505`) with its sole documentation conflict resolved.
New source now includes deleted setup prompt markdown, the native edit-intent veto and setup usage journal,
goal-revision adoption, conversation input reads, chat/CLI task controls and reviewed-goal adoption.
Native GitHub publication, task-aware lease/report transitions and signed wait source are being implemented under `afd_fleet/src/task/` with wire/API resources.
It is **uncompiled and unverified**. Native mixed-input binding/receipts and R2 storage/manifest APIs now have source; automatic execution and supervisor capture/restore remain unfinished; native wait/report code is also unverified. The latest source-only reviews found no remaining actionable findings in generation/backlog/setup/completion corrections or the 17 typed supervisor task calls. Physical decision execution and durable receipt recovery still need connection to the root coordinator. Read the newest source inventory at the end before resuming.
Continue the code; preserve one Rust lane and run the complete required checks after the implementation batch is complete.

Continue branch `feat/m236-durable-tasks` in `/workspace/agentsfleet-m236-design`, updating existing draft PR #803 only.
The published production correction is `c8937f3692b7b3691fdd6e87e1858c94c9477871`; it contains current main `27deff642`.
Subsequent recovery scenarios and their verified results are recorded below; read the branch's latest commit before restarting work.
The opening check in the resumed environment confirms published head `ebfb3b722a911600bb00e9677e4f2b003e2169c7`; see the resumed control API section below.

The implemented foundation includes native task binding/control, authenticated task reads, customer-owned question revisions, runner decision profiles and four provider adapters.
Decision reservation, fresh platform-admin Secrets, original-owner accounting, fair financial maintenance, metrics and privacy corrections are also implemented.
The approved runner deadline and schedule activity changes remain in the same PR.

M236 remains `IN_PROGRESS`; the automatic intent/goal loop, production binding/input APIs and task-aware lease/report transitions remain unfinished.
Native GitHub effects, signed completion matching, durable waits and incremental R2 capture/restore also remain unfinished.
Checkpoint encryption exists; object registration, quotas, manifests, authorizations, pins, collection, supervisor recovery and live R2 acceptance do not.

Public task controls/revision adoption, confirmed PR/file results and CLI/browser decision/transfer progress still need their runtime integration.

Resume by checking exact-head CI and Greptile, then completing those runtime owners through the existing admission/lease path.
Keep one Rust lane, finish each crate slice before gates, and repeat checks only after relevant source changes or concrete failures.
Do not mark the spec done, make the PR ready or merge until its full scope and acceptance evidence pass.

Final delivery includes Indy's detailed change report, verification results and observability coverage.

Indy requires Rust diff coverage above 99.5% and TypeScript coverage of 100%, without reducing existing file/workspace floors.

On `c8937f369`, Rust patch coverage is 97.9193%: 5,506/5,623 changed lines, with 117 unhit lines.
Workspace coverage passes at 99.6225%; 32 of 194 touched files remain below their 99% whole-file floor.
All four measurement jobs pass after retrying the upstream Debian HTTP 503 failure; the combined grade correctly remains red.

The deciding coverage result is workflow `41945042540`, job `126984193594`; it supersedes the earlier 96.8894% patch result.
All ordinary workflows pass on that head; the newer local recovery scenarios require their own hosted measurement after publication.

The four matching reports and complete 117-line gap list are retained in `/tmp/m236-coverage-c8937f369/` for this workspace.
Their recorded merge revision is `d12a443729916f22f8c667afca8062974a8c8f0b`; its tree matches `c8937f369` exactly.
The native report parser reproduces 5,506/5,623 without substituting reports from another revision.

All four TypeScript coverage jobs pass at 100% on `c8937f369`; app 4,669, website 133 and design-system 522 tests pass.
The CLI log records actual function/line coverage of 100.00%; its count remains separate from earlier local results.
Both existing Greptile threads are resolved; request and check review on every later pushed head.

Indy explicitly approved the three daemon-library build-resource settings; only debug symbols and compilation concurrency changed.
Docs PR #241 is merged; public API changes still require their matching documentation flow.
The development R2 bucket is private `agentsfleet-checkpoints-dev`; use the supplied 1Password references securely and never print resolved credentials.

Crates.io still reports `object_store` 0.14.2 as latest stable for this continuation.
Its `Signer::signed_url_opts` and `SignedUrlOptions::with_signed_header` support binding create-only headers without another signing implementation.
The native R2 rejection and readback proofs remain mandatory; inspecting that SDK is not a live storage result.

## Opening dependency verification

Indy approved implementation and latest stable dependencies on November 3, 2026.
The comparison baseline is `2f8c878c09e9c4048da1724385ec58746d348ed4`; opening documentation commit is `cd8245dbf`.
The measured runtime changes are only the Rig 0.46.0 manifest and lockfile upgrade, including transitive rig-http 0.46.0.

Crates.io metadata confirmed rig-core 0.46.0, reqwest 0.13.5, object_store 0.14.2 and aes-gcm 0.11.1 as latest stable.
No additional decision SDK or R2 runtime dependency has been introduced yet.

Run the repository's `make test-unit-rustd` with the pinned Rust 1.99.1 toolchain and all features selected by its existing shards.
The cloud host used two Cargo jobs, disabled debug information/incremental compilation, a child-reaping init wrapper and umask 022.

| Shard | Decisive command output |
|---|---|
| Runner | `unit: runner — 1490 passed` |
| Daemon | `unit: daemon — 139 passed` |
| Daemon libraries | `unit: daemon libraries — 3343 passed` |
| Combined Rust unit | 4972 passed; command exit 0. Datastore-dependent tests remain ignored in this lane. |

Full output remains at `/tmp/m236-rig-rust-tests.log` in this cloud environment.
This proves existing Rust unit compatibility after the dependency upgrade; it does not prove DT functionality or full repository verification.

Initial runs exposed host assumptions: orphaned process reaping and published-file permissions under umask 077.
The completed run used upstream Tini 0.19.0 as a subreaper and umask 022, without source edits or skipped tests.
The downloaded static binary matched upstream SHA-256 `c5b0666b4cb676901f90dfcb37106783c5fe2077b04590973b885950611b30ee`.

## Decision-client component

`afr_providers::decision` contains one configured client behind `DecisionEvaluator`.
The client owns its profile and credential; callers supply a validated request.
Four native decision adapters share guarded transport, a streaming reply bound and physical timeout.

Nine modular scenarios exercise the production client against owned local TLS servers.
They cover native protocols, exact bounds, invalid answers, retained usage, missing results, redirects, cancellation and private diagnostics.
Fixtures use channels, atomics and bounded shutdown without a new mutex or paid provider calls.

The first complete provider/telemetry component run used the existing make target:

```text
make test-unit-rustd-runner _RUSTD_UNIT_RUNNER='afr_providers afr_telemetry'
```

Its deciding output was 144 passed: provider unit 44, decision scenarios 9, existing provider scenarios 49 and telemetry 42.
The log is `/tmp/m236-decision-affected-unit.log`.

This run preceded the final credential-ownership refinement.
The affected decision scenarios then passed nine tests in `/tmp/m236-decision-final-unit.log`.

`make lint-rustd` passed after mechanical source and fixture corrections.
It covered source rules, formatting, workspace Clippy with all targets/features, and separate default-feature builds of both binaries.
Full output is `/tmp/m236-rust-lint.log`; review corrections below require another affected lint check.

Decision metrics record evaluations and elapsed time by bounded provider, purpose and outcome.
Logs and spans carry native call/profile identifiers; the scenarios assert that credentials, evidence and private endpoints remain absent.
Billing settlement and user-visible activity still require native lifecycle integration.

## Astra implementation review and corrections

Requested `gpt-6-astra` with extra-high reasoning reviewed the native code and existing dependency implementations on November 3, 2026.
The review covered the decision client, wire types, scenarios, telemetry, secret ownership and task/schema foundation.
The serving revision was not independently observable; this review supplied no benchmark or live activation evidence.

| Finding | Correction and remaining proof |
|---|---|
| Duplicate JSON keys could hide conflicting decisions. | One parser records ambiguity before replacement. Semantic answers are rejected; independent usage survives, while ambiguous usage remains unknown. |
| Lock waits inflated displayed wall limits. | Native insertion derives creation and expiry from one post-lock clock instant. Full task binding integration proof remains pending. |
| Client cloning duplicated the profile and secret. | Removed the configured client's `Clone` implementation; runtime composition can share the existing evaluator trait. |
| Response decoding copied entire JSON trees. | Deserialize raw protocol records from borrowed values. Owned result strings retain their required allocations. |
| Byte validation allocated rejected serialized buffers. | A bounded counting writer measures complete escaped JSON and stops at the ceiling. The provider question check shares it. |
| Credential validation created an extra plaintext header copy. | Build the actual authenticated request once; preserve builder failures as `NotSent` before executing it. |
| Three tables and a cyclic key shared one oversized slot. | Split unshipped slots 2190–2220 and add supporting foreign-key deletion indexes. Query-plan measurements remain pending. |

After this correction batch, the affected provider/wire gate passed 253 tests:

```text
make test-unit-rustd-runner _RUSTD_UNIT_RUNNER='afr_providers afd_wire'
```

The deciding output was `unit: runner — 253 passed`; full output is `/tmp/m236-astra-fixes-unit.log`.
This includes the nine modular decision scenarios with duplicate-key and exact escaped-byte-boundary coverage.

The correction batch passed `make lint-rustd`; full output is `/tmp/m236-astra-fixes-lint.log`.
The requested Astra extra-high follow-up verified the original findings closed, with no material residual issue in this batch.
It inspected the corrections without running tests, builds or git mutations.

`SecretString` wipes its current owned allocation on normal drop.
An abort can skip destructors; request headers and transport buffers remain outside the owned-buffer guarantee.
No production latency, allocation-count or large-repository transfer benchmark is claimed.

## Catalogue configuration component

The existing catalogue now stores immutable reviewed decision definitions and separate coding/decision price schedules.
Decision-only entries need no fabricated coding rates; omission during an update preserves saved decision settings and prices.
List reads use one database snapshot, and current or historical revisions cannot acquire different definitions.

Astra's requested extra-high review found two validation defects: metadata-only PATCH rejection and PostgreSQL NUL rejection after shape validation.
The corrections allow shape-valid empty PATCH schedules and reject NUL in decision strings before saving.
Native validation still checks whether the saved coding settings require coding rates.

The first affected-package unit command passed 293 tests:

```text
make test-unit-rustd-daemon-libs _RUSTD_UNIT_DAEMON_LIBS='afd_admin afd_api_wire afd_api_operator afd_inference afd_billing afd_tenant'
```

The log is `/tmp/m236-profile-unit.log`; this run preceded the review corrections.
The corrected wire/inference selection passed 141 tests in `/tmp/m236-profile-wire-unit.log`.
The corrected operator/credential selection passed 172 tests in `/tmp/m236-secrets-unit.log`.

`make _migrate-test-db` rebuilt the owned local schemas through slot 2250 and established the `api_runtime` login.
The log is `/tmp/m236-profile-migrate.log`.
All eleven `afd_admin` database scenarios passed, including both new multi-invariant decision scenarios.

Those scenarios prove revision preservation, both rate protocols, rollback, decision-only entries and invalid-definition refusal through production owners.
They ran through the existing make lane with `cargo test --all-features -p afd_admin -- --ignored`.
The log is `/tmp/m236-profile-integration.log`; this scoped run does not satisfy the full repository integration gate.

Astra found no remaining production blocker in the catalogue/pricing component after the corrections.
It identified the existing PATCH test expectation that also needed updating; that correction and static decision-specific refusals are now written.
No activation, latency or allocation benchmark follows from this source review.

The broader native database run exposed an old three-column price conflict key in shared `ModelSeed` SQL.
Seventeen credential scenarios failed during fixture setup; all three new decision-key scenarios and eleven admin scenarios passed.
The log is `/tmp/m236-catalogue-secrets-integration.log`; this run is a failure, not a completed integration gate.

An intermediate correction updated that conflict key, but the full upgrade test exposed its wider incompatibility.
The old coding daemon's writers and readers also needed to remain valid while a release migrated.
That shared-table design is superseded by the additive decision-price table below.

The importer check passed TypeScript compilation, 76 unit tests and its curated-data check through `make _model_allowlist_check`.
Twenty-six database tests were skipped there; the Rust-owned live suite must grade them.
The log is `/tmp/m236-catalogue-import-unit.log`.

The first clean full-workspace gate failed in the historical authoring-upgrade fixture: 262 passed and one failed in that suite.
Its current reader/seed referred to a price column absent from the older schema; the log is `/tmp/m236-current-workspace-integration.log`.
Pinning a test-only writer would not repair deployed coding writers or prevent old readers from mixing price protocols.

Unshipped slot 2250 now adds `model.decision_prices` while preserving the original coding table, uniqueness and statements.
Both sources share price values, row decoding, catalogue transactions and the existing biller; `PriceReplacement` borrows its values and selects fixed statements.
The imported model-identity protections remain, including clearing removed coding settings and excluding decision-only rows from coding diffs.

Astra's extra-high source review verified the additive correction without another material finding.
Decision writes use 15/4 parameters for upsert/prune; coding writes retain their original 14/3.
Creating decision profiles still requires upgraded catalogue writers/importers.

The next clean full-workspace run passed the historical-upgrade scenario, live catalogue importer and Secrets scenarios, then failed seven database schema checks (40 passed).
The log is `/tmp/m236-additive-workspace-integration.log`.

The correction uses UUIDv7 revision identities, model-scoped current selection, immutable revision triggers and scoped decision-price references.
New price instants use `_at` names; original coding SQL remains unchanged.
Explicit table, role, immutability and migration inventories now include the five new tables; no checker logic or exception list changed.

The clean rerun passed all 47 database checks and reached the later daemon HTTP suite.
Repeated lease-based scenarios failed there; the remaining run was stopped by its scoped process group rather than continuing the same failures.
The log is `/tmp/m236-schema-correction-integration.log`; exit 130 is not a completed repository integration gate.

Astra's extra-high source recheck found no material residual defect in the corrected schema/catalogue batch.

## Platform-admin decision keys

The existing `Providers` resolver opens fixed API bags in the trusted platform-admin workspace for each read.
It keeps no key cache and supplies no tenant or environment fallback.
The typed parser owns one guarded key string, and the existing vault reader guards decrypted bytes.

Three native scenarios cover four APIs, workspace isolation, rotation/deletion, malformed fields, damaged ciphertext and unavailable storage.
They drive real encrypted rows and the production Secrets write, replace and delete owners without a new mutex.
Astra found no material issue in this helper slice; reservation and lease-handler composition remain pending.

Parser scratch/error buffers and HTTP buffers remain outside the owned-allocation wiping guarantee.
The resolver's tracing span records its API without exposing plaintext; no production timing benchmark has been measured.

## Fleet question ownership refinement

Indy selected editable fleet questions and criteria through setup chat or CLI, without platform ownership of their wording.
He confirmed that each Apply must create another immutable revision rather than edit an old one.
The proposal now includes separate question/criteria views, revision history and the version each DT has accepted.

`instructions_md` supplies business context; structured `decision_policy` supplies the runnable questions.
Existing DTs require explicit revision adoption, which invalidates older assessments without erasing history or resetting spend.
The native parser now owns typed routing/completion policy under `config["x-agentsfleet"]` and the proposal tool exposes its editable criteria.

UI and CLI review projections are written, including the primary fleet setup screen and proposed/saved revision labels.
Task snapshots retain named questions with full criteria; malformed task JSON uses the existing value-free `Unreadable` cause.

Astra found and guided corrections for the validation-leaf dependency, UTF-8 bounds, primary-screen wiring, excess CLI fields, published criterion shapes and private JSON causes.
The first native component check failed at compile time on a helper-name collision and a generic serde bound; both are corrected.

The native unit shards passed before the latest dependency updates: runner 1499, daemon 139 and daemon libraries 3352.
Across the initial and resumed Make invocations, all 4990 tests passed.
Question rendering/dispatch, explicit task revision adoption and end-to-end DT integration remain pending.

Indy rejected displaying internal routing rules: "What is the value the user gets by seeing that?"
UI and CLI setup now show `What counts as done?`, owner-editable yes/no criteria and the saved revision; routing categories, model terminology and internal statement IDs are omitted.

The validated routing policy and machine-readable setup remain intact. The HTML preview follows the same display rule.
Frozen installs passed at the root and all three UI packages, preserving the existing assistant-ui patches and overrides.

`AGENTSFLEET_UI_ENV_FILE=/tmp/m236-ui-unit.env NEXT_PUBLIC_POSTHOG_ENABLED= make test-unit-app` passed all 4640 tests across 464 files on Vitest 5.1.0.
The 162.86-second run used an explicit public test API origin; it proves the app suite rather than the repository or DT performance.

The first app lint passed Oxlint and type checking, then refused the missing configured environment-file path during source checks.
Using the explicit public test environment, all app, design-system, website and CLI lint targets passed.

The remaining Make unit targets passed: CLI 1989 with 16 skips, design-system 522 and website 133; the combined invocation exited zero.
The four CLI question scenarios cover readable success criteria, unchanged JSON, malformed policy, ordinary fleets, saved revisions and UTF-8 bounds.
These package gates do not complete the repository's remaining native integration, runtime or live acceptance criteria.

The first scrollable HTML walkthrough combined ten setup messages and a work request/reply; its transcript, PR number and task activity were illustrative.
Browser checks verified section navigation, copy/composer controls, omitted routing internals, desktop/mobile layout and zero page errors.

Indy identified that the combined example placed PR work beside a setup composer.
The corrected setup mock contains ten onboarding messages, inline success criteria and the saved revision's `Open fleet` link.
The separate fleet-chat mock contains ten work messages and three DTs, with an optional CI event and four steering/pause follow-up messages.

Browser checks passed for both mocks on desktop and mobile: separate composers, saved-setup transition/spinner, three work items, one running item and selected-PR replies.
The CI follow-up preserves the same work items and fleet conversation; page errors and HTTP requests were both zero.
The original walkthrough now contains setup only; the DT panel remains a proposed runtime UI addition.

Indy approved both mocks and specifically accepted the centered `Save done v1 → Open fleet` pill and work-panel typography.
The implementation reference retains bundled Instrument Sans, current design-system tokens, 14px conversation text, 13px work labels and 12px secondary text.
Native work-panel implementation and its browser comparison remain pending; the mock approval does not complete runtime acceptance.

Indy explicitly signed off both chat visuals.
The approved mock places `Current work` inline above the composer, with details expanding within the same fleet chat on desktop and mobile.

Conditional fleet handoffs were inspected separately: caller steering already supports authorized targets, while native message and schedule tools stay thread/fleet scoped.
Indy authorized a separate-session prompt if this was not a small change; no cross-fleet runtime code is added to M236.

## Integration probe and ship sequence

One clean runner-message reproducer returned 403 before reaching the daemon: its local `[::]:port` address was routed through the inherited HTTP proxy.
Extending `NO_PROXY` only for local fixture addresses made the same clean case pass: one passed, zero failed, with no source changes.
The diagnostic uses the existing Make reset/migrate environment and preserved proxy/CA settings; `/tmp/m236-lease-probe{,-local}.log` holds both results.

Indy: "And if everything is green (CI jobs green, greptile resolved) merge the PR".
Before ship, verify PR #798 is merged, integrate current main, finish all declared gates and push the resulting revision.
Review all architecture pages and all docs-repository Markdown/MDX pages, publish both PRs, and check CI/review status against the actual pushed commit.

## Durable Task acceptance

The task schema and lifecycle service are partial, without completed runtime integration.
Task APIs, physical decision reservations, native accounting, R2 recovery and task UI behavior remain unfinished.
Provider activation, storage, browser, performance and combined live-scenario criteria remain ungraded.

Component tests and SDK compatibility are separate from those acceptance criteria.

## Runner decision selection checkpoint

Existing enrollment, edit, heartbeat, self and operator reads now carry an optional reviewed catalogue selection.
The shared operator form preserves the selection across policy edits and loads enabled models from the current catalogue.
Platform Secrets remain the only decision-key source; assignment carries no key material.

Generic setup-authoring guidance lives in Markdown beside its native assembly code.
Customer instructions and completion questions remain accepted revision data; linkable starter copies use existing immutable Library revisions.

Astra reviewed this source slice at extra-high effort on November 3, 2026, without running builds or tests.
Its corrections cover saved enrollment binds, accurate deletion blockers and catalogue revision ownership under row-lock contention.

`make lint-app` passed; `make test-unit-app` passed 4,641 tests across 465 files.
`make lint-rustd` passed Clippy and both production feature checks; the native unit shards passed 1,499 runner, 139 daemon and 3,353 library tests.

An unchanged toolbox permission test first failed under inherited `umask 0077`; process-local `umask 0022` restored its expected read-only mode.
No source or assertion changed for that environment correction; local proxy exclusions remain required for network fixtures.

Clean `make test-integration-rustd` passed 1,465 regular and three exclusive cases, including catalogue ownership, native secret resolution and both assignment races.
The deferred owner check retains immutable-table privileges and satisfies schema naming and literal rules without changing their checks.
This checkpoint does not establish completed task runtime or measured performance.

## Lease decision snapshot component

PR #798 merged and main was integrated in `28bc628aad363d18498c1a15d5c0a9d85b0a9c37` before the foundations checkpoint was pushed.
`orly gate verify` and the branch history secret scan passed for that pushed checkpoint.

Lease issue stores and delivers one reviewed model/profile/threshold snapshot; assembly moves its decoded strings instead of cloning them.
An unconfigured host skips the catalogue read; a disabled selection supplies no decision profile and cannot select a substitute.
Keys remain outside the snapshot and still require a physical-call reservation before dispatch.

Astra's extra-high source review found a requested-model length mismatch and a stale expiry check after slow profile reads.
The shared 256-byte model bound matches catalogue validation; the issue statement checks a fresh clock after acquiring the fleet slot lock.
The new expiry test blocks the real profile read, observes database expiry and checks lease, audit and tally outcomes together.

After both corrections, `make lint-rustd` and clean `make test-integration-rustd` passed: 1,469 regular and three exclusive cases.
Three cohesive cases cover native issue/render/storage, 256-byte model names, revisions, disablement, deletion, lock ordering and expiry without issuance side effects.
The logs are `/tmp/m236-lease-expiry-{lint,integration}.log`; no new mutex or paid provider request was involved.

The affected unit check passed 655 tests through the existing Make target:

```text
make test-unit-rustd-runner _RUSTD_UNIT_RUNNER='afd_admin afd_api_wire afd_db afd_fleet afd_runner afd_wire afr_providers afr_telemetry'
```

The log is `/tmp/m236-lease-decision-affected-unit.log`; this checks eight affected packages, not the whole repository.
The daemon regenerated OpenAPI with 107 paths and 933,655 bytes, including the lease assignment and its 256-byte requested-model bound.

This component supplies a frozen selection. Physical reservation, key dispatch, task decisions and full DT acceptance remain pending.

## Current work read component

Authenticated collection/detail routes now use the daemon's shared task owner, keyset cursors, immutable accepted criteria and native resource tags.
Collection reads distinguish missing scoped fleets from owned empty fleets; both scope and task SQL run inside one five-second deadline with transaction-local statement cancellation.
Timeouts use a closed metric outcome and bounded reason, alongside the existing task operation counters and duration measurements.

The inline work panel uses the existing fleet chat, fonts, components and stream.
It reads confirmed rows, expands saved criteria lazily and retains explicitly labelled progress during transient refresh failures.
Identity changes and access refusal clear its cached work; the first stream hello closes the initial read/subscription gap.

The native bind/control owner publishes read hints after commit; no extra queue, chat or EventSource was introduced.

Astra reviewed this slice at extra-high effort and verified the subscription-gap, scoped-fleet lookup and SQL cancellation corrections in source.
That review ran no gates. Real publisher-to-browser delivery and forced client-transport deadline behavior still require live proof.

Changed executable TS/TSX lines require 100% coverage; Rust patch coverage must strictly exceed 99% against latest main, per Indy's instruction in the active spec.
The existing Make and Codecov Rust patch floors now agree at 99.01%; per-file and whole-workspace checks retain their separate floors.
Scenario tests exercise several observable invariants together; the broader test cleanup remains with the other agent.

`make lint-rustd` passed after the final scenario corrections, including workspace Clippy with all targets and features.
The log is `/tmp/m236-task-read-component-lint.log`.
The clean `make test-integration-rustd` run passed 1,471 regular tests and all three exclusive tests; `/tmp/m236-task-read-component-integration.log` records both tallies.

The revised native scenarios prove scoped paging, independent timestamp ordering, accepted revision pinning after a newer save, resource revalidation and connected SQL cancellation.
Their guards dispose owned rows and isolated databases after assertion failure; no new mutex was added.
The daemon regenerated `public/openapi.json`: 109 paths, 957,320 bytes, including both task reads and their response schemas.

`make test-coverage-all` passed on the final UI source; `/tmp/m236-task-component-ts-coverage.log` records the package gates.

| Package | Tests | Measured coverage |
|---|---|---|
| App | 4,653 passed across 468 files | 100% statements, branches, functions and lines; 10,412 executable lines covered. |
| Website | 133 passed | 100% statements, branches, functions and lines. |
| CLI | 1,989 passed, 16 skipped | Existing enforcement passed 100% function and line coverage; Bun does not report branch coverage. |
| Design system | 522 passed | 100% statements, branches, functions and lines. |

The production-component browser recheck passed all eight desktop/mobile cases after the final copy refinement.
It verifies typography, lazy criteria, the shared conversation and usable composer geometry; `/tmp/m236-task-component-browser.log` records the 6.2-second run.
Controlled read fixtures supply its data; this is not full live DT acceptance.

The [native coverage attempt on `bf0684c97`](https://github.com/agentsfleet/agentsfleet/actions/runs/41723739970) passed runner, daemon and kernel shards.
The substrate shard failed on a concurrent catalogue seed primary-key collision; the complete Rust patch percentage remains unverified.
Raising the floor is not a measured result; only a complete report at the tested revision can satisfy it.

## Mixed-model settlement recovery

The existing billing sweeper now reconstructs provider/model/posture groups under each original work identity.
It selects the oldest bounded batch first, sorts borrowed keys in place, and moves owned headers without allocating cloned map keys.
Recovery counters count settlement groups; their existing names and closed work-type labels remain unchanged.

One isolated live scenario verifies complete headers, six unique usage lines, cumulative per-work amounts, native wallet settlement and unchanged repeated recovery.
It uses four model/posture groups under one work and another work with the same model.
Stored charges are synthetic inputs; the scenario does not measure provider rates.

The same scenario failed against the previous implementation at its native reconstruction assertion: two groups appeared instead of five.
The fixed source was restored before its verification batch; `/tmp/m236-billing-group-parent-red.log` records the assertion failure.
Astra's extra-high review found no material product defect and verified the grace-period and independent sequence-ordering corrections in the test.

| Component check | Result and retained output |
|---|---|
| `make lint-rustd` | Passed; `/tmp/m236-billing-groups-checks-lint.log`. |
| Affected native units | 331 passed through `make test-unit-rustd-runner` for `afd_billing afd_fleet afd_observability`; `/tmp/m236-billing-groups-unit.log`. |
| Clean native integration | 1,472 regular tests and all three exclusive tests passed; `/tmp/m236-billing-groups-integration.log`. The original Make process completed with exit zero. |

The new scenario passed in that clean integration run.
No new mutex, billing queue or metrics family was added.
Physical decision reservations, frozen call pricing, receipt accounting and hold-release guards remain pending; Rust patch coverage remains unmeasured.

## Usage sequence component

Coding, runtime and native decisions use disjoint sequence ranges under the same lease work identity.
Runtime allocation rejects negative, exhausted and overflowing counters; renewal and expiry SQL exclude decision sequences from their runtime counts.
The cohesive native scenarios compare complete boundary settlements and exercise decision usage, renewal, abandonment, expiry billing and replay together.

The failed remote coverage attempt exposed a fixed catalogue seed identifier shared by concurrent tests.
Each seed now mints a candidate and lets the existing provider/model uniqueness rule choose the logical row.
A bounded isolated scenario releases 100 callers across two models, then checks their complete identities, inference settings and quotes together.

It proves concurrent callers over the configured pool, not 100 simultaneous SQL statements.
Exercise and teardown deadlines begin after fixture construction; they do not prove bounded migration startup.

Astra reviewed this component at extra-high effort and found no material source or test defect.
Its documentation correction is applied in the billing architecture page; the review ran no gates.

`make lint-rustd` passed after correcting two fixture string assignments to reuse their allocation.
The log is `/tmp/m236-usage-ranges-lint-final.log`.

`make test-unit-rustd-runner _RUSTD_UNIT_RUNNER='afd_fleet afd_wire'` passed 291 tests; `/tmp/m236-usage-ranges-unit.log` records the scoped result.
The initial clean integration run caught a prepared-plan fixture missing the new twelfth metering argument; both its declaration and invocation now match production.

The clean `make test-integration-rustd` rerun passed 1,473 regular tests and all three exclusive tests.
The log is `/tmp/m236-usage-ranges-integration-final.log`; the complete Rust patch coverage measurement remains pending.

`orly gate work` and `orly gate verify` passed, including `make lint-all` and version agreement at 0.72.0.
The verification log is `/tmp/m236-usage-ranges-verify.log`; these checkpoint checks do not close the full milestone.

## Complete native coverage measurement

The [next remote measurement](https://github.com/agentsfleet/agentsfleet/actions/runs/41732479873) tested `6c5392015`.
Runner, daemon and kernel reports completed; the substrate worker exhausted disk before uploading its report.
The failed check annotation identifies `System.IO.IOException: No space left on device`.

Indy approved the three-line resource proposal on Nov 04, 2026: "Approve the resource settings".
The shard worker now disables development/test debug information and limits Cargo to two jobs, matching the local measurement settings.
No coverage floor, source exclusion, test selection or required check changed.

A clean verification checkout at exactly `6c5392015` completed `make test-coverage-rustd RUSTD_SHARD=substrate` with the documented cloud Cargo settings.
Its deciding output was 5,108 regular tests and three exclusive tests passed.
The log is `/tmp/m236-coverage-6c5392015-substrate-local.log`.

The original remote reports and all four original revision stamps were retained.
Only the local report's workspace `SF:` prefix was aligned with the remote reports; a byte comparison verified every non-path counter record remained unchanged.

The unchanged `make test-coverage-rustd-merge` gate then measured 99.0137% whole-workspace coverage and 79.6834% patch coverage: 2,114 of 2,653 changed executable lines.
Thirty-eight of 111 touched files were below the separate 99% file floor.
The log is `/tmp/m236-coverage-6c5392015-merged-canonical.log`.

This fails the required floors; no coverage requirement or exclusion was relaxed.
The largest gap is the native task binding owner, followed by negative paths in decision adapters and reviewed configuration.

The next component adds cohesive scenarios through `TaskLifecycle` for concurrent whole-plan acceptance/replay, source/revision qualification, capacity rollback, cumulative limits, controls and unreadable saved state.
A real blocked Postgres query tests the client deadline and pool-permit recovery; it does not prove literal packet loss.
The fixture retains database cleanup ownership outside guarded setup and checks.

Astra's extra-high review identified and checked the revision, wall-duration and connection-lifetime corrections, then closed the guarded-setup cleanup finding with no remaining material finding.
`make lint-rustd` passed after two empty-page assertions were changed to retain the returned values in failure diagnostics.
The log is `/tmp/m236-task-lifecycle-lint-final.log`.

The first integration run found invalid fixture names; all three definitions now use valid slugs and include the required budget.
After a clean schema reset, the scoped `cargo test --all-features -p afd_fleet --test fleet_suite integration_tasks:: -- --ignored` batch passed all ten scenarios.
The log is `/tmp/m236-task-lifecycle-focus.log`; this scoped pass does not replace the repository gate.

The clean `make test-integration-rustd` rerun then passed 1,483 regular tests and all three exclusive tests.
The log is `/tmp/m236-task-lifecycle-integration-final.log`; its recorded exit status is zero.

## Native lifecycle coverage result

The [four-shard measurement](https://github.com/agentsfleet/agentsfleet/actions/runs/41751320421) completed successfully at `b5db2b455` with the approved worker resources.
Its unchanged merge gate measured 99.5583% workspace coverage: 78,893 of 79,243 executable lines, with 350 missed.
All four raw reports and revision stamps were verified against that commit.

Manual workflow dispatch leaves the patch base empty, so that green run did not grade patch or touched-file coverage.
The unchanged merge gate was rerun with explicit base `fde5859717c73ea52d02d84d2408b8f00efcae82` in a clean checkout of the same commit.
It measured 96.0133% patch coverage: 2,601 of 2,709 changed lines; 30 of 113 touched files missed the separate 99% floor.

The log is `/tmp/m236-coverage-b5db2b455-vs-fde585971.log`.
Those results fail the required patch/file floors; they precede the subsequent main integration and cannot grade that integrated source.
No coverage floor, assertion, exclusion or report counter was changed.

A temporary source mutation inverted the native machine-source guard in the isolated verification checkout.
The qualification scenario failed at its authority assertion when the mutated owner allocated bindings; the original source was restored byte-for-byte.
The deciding output is retained in `/tmp/m236-source-authority-mutant.log`; the verification checkout is clean.

Main advanced to `11883fdab` while coverage ran; merge `205addaa2` integrates it before the next feature push.
Astra found no material merge defect in the combined decision/memory lease payload, parallel context reads or gate registration.
The integrated provider-component gates below subsequently passed.

## Provider validation with integrated main

Decision encoding now checks each question before collection, removing an unnecessary temporary reference vector and unreachable representation errors.
Score validation walks borrowed, ordered entries instead of repeatedly looking them up.
The duplicate-key parser retains ambiguity detection while removing JSON visitor paths its decoder cannot call.

Three cohesive HTTP scenarios exercise 54 exchanges across the four protocols, alongside the existing transport, deadline and preflight cases.
They compare complete outcomes, reported identity, retained usage, credential placement, physical-send counts and scoped measurements together.
The scenarios add no mutex or process-wide capture lock.

Astra found masking errors in the first duplicate fixtures and an incomplete model-identity assertion.
The corrected fixtures append duplicates to complete valid distributions and compare the entire `DecisionResult`.
The shifted-score fixture also preserves a matching weighted score, so removing contiguity validation would incorrectly accept it.

The first lint pass requested a borrowed measurement slice; that mechanical correction is applied.
`orly doctor`, `orly gate work` and `orly gate verify` passed; verification includes `make lint-all` and version agreement at 0.73.0.
The final verification log is `/tmp/m236-main-provider-verify-final.log`.

`make test-unit-all` passed 1,593 runner, 141 daemon and 3,377 library tests, with all four TypeScript coverage gates at 100%.
The app passed 4,653 tests; website 133, CLI 1,997 and design system 522; 16 CLI live-acceptance tests remained explicitly skipped.
The log is `/tmp/m236-main-provider-unit-all.log`; these results do not establish live acceptance.

Clean `make test-integration-rustd` passed 1,494 regular tests and all three exclusive tests.
The log is `/tmp/m236-main-provider-integration.log`; the enclosing batch recorded exit zero.
A fresh fetch confirmed main remained `11883fdab`, an ancestor of the integrated branch.

The companion documentation branch also integrates its latest main in `9a70538` and is pushed.
Its work gate passed 25 checks; full documentation verification and the branch secret scan passed.
The logs are `/tmp/m236-docs-main-work.log`, `/tmp/m236-docs-main-verify.log` and `/tmp/m236-docs-main-secrets.log`.

Current-main Rust coverage remains below the required patch floor; the next section records the complete measurement.
Physical decision reservations, runtime integration and R2 recovery remain unfinished.

## Current-main native coverage

The [four-shard measurement at `37c177bb8`](https://github.com/agentsfleet/agentsfleet/actions/runs/41761357273) passed every shard and the workspace floor.
All four downloaded report archives, checksums and revision stamps matched that exact commit.
Manual dispatch left the patch base empty; its green result did not grade changed lines.

The unchanged merge command ran in a clean checkout with explicit base `11883fdab`.
It measured 99.6178% workspace coverage, covering 79,498 of 79,803 lines.
Patch coverage was 97.3743%, covering 2,633 of 2,704 changed lines; 26 of 114 touched files missed their separate 99% floor.

`make test-coverage-rustd-merge RUSTD_PATCH_BASE=11883fdab` therefore failed the required patch/file checks.
The log is `/tmp/m236-coverage-37c177bb8-vs-11883fdab.log`; no floor, report counter or exclusion changed.

## Editable pull-request starter

Indy clarified that New fleet's Review pull requests button must load the full customer-editable starter, including proposed completion questions.
The canonical public JSON supplies all three existing buttons through the existing suggestion component with `send={false}`.
It adds no loader, request route or dependency.

The review starter asks for repository, timing and destination instead of assuming them.
It includes two completion questions with yes/no criteria; owner edits travel in the ordinary setup message.
Selection makes no write or save request, and submitting sends exactly one edited message.

The Rust source audit found no production PR-review canned reply to delete.
Authoring response text comes from the model response; accepted revisions supply customer instructions and questions.
Generic authoring guidance remains separate; the earlier claim that a PR-review canned reply remained in Rust was incorrect.

The replacement regression failed on the parent UI's one-line starter at its completion-question assertion.
The log is `/tmp/m236-starter-parent-red.log`; the fixed flow passes in the full app suite.
Astra's source review found no material issue and ran no builds or tests.

`make lint-app` passed, including type checking and source rules.
`make test-coverage-all` passed all four 100% coverage checks: app 4,653, website 133, CLI 1,997 and design system 522 tests.
Sixteen CLI live tests remained explicitly skipped; `/tmp/m236-starter-coverage.log` records the complete results.

The production-component browser suite passed ten desktop/mobile cases, including keyboard selection, retained edits, zero writes, 14px text and usable composer geometry.
The log is `/tmp/m236-starter-browser.log`; the batch recorded exit zero.
This controlled browser proof does not establish live model authoring or full DT acceptance.

The companion setup guide is pushed in `0b2bc39`, after its work gate, complete site validation and secret scans passed.

## Native decision accounting checkpoint

The native component stores one physical-call intent, obtains confirmed whole-work credit, resolves fresh platform Secrets and commits one credential issuance.
Replays contain saved request/results without the key.
Typed receipts retain unknown reservations or record normalized usage at the frozen price under the original lease and payer, including after fleet deletion.

The shared biller and SQL helpers remain the owners; no second task queue or executor is introduced.

The requested `gpt-6-astra`, extra-high source review found an intent-receipt/issuance race.
Receipts now take the existing optional fleet-slot lock before task/call locks, serializing actual overages with further physical issuance.
Astra verified the correction and reported no residual material component findings; it ran no tests.

Its serving revision and effective reasoning setting were not independently observable.

`make lint-all` passed after merging main `bf3baa126`, including workspace/all-target/all-feature Clippy with warnings denied.
`orly gate verify` then passed in `/tmp/m236-decisions-post-main-verify-2.log`, including changed prose and product-version checks.
The earlier enclosing gate reported documentation paragraph and wording findings; their correction changed no runtime behavior.

The new scenarios cover 100 concurrent replays, uncertainty/retry accounting, source/task budget ownership, deletion, funding recovery, cancellation and slot ordering.
On integrated main `bf3baa126`, all eight pass within clean `make test-integration-rustd`: 1,502 regular tests and all three exclusive tests passed.
The enclosing batch exited zero; `/tmp/m236-decisions-post-main-integration.log` retains its results.

The funding scenario now also proves missing coding prices and unconfirmed credit preserve the same intent before fresh-key recovery.
It checks the distinct bounded funding reasons alongside retained rows, one later charge and revoked-model disposition.
The tests add no mutex; existing trace capture is acquired once per scenario before creating a lease.

After merging main `bf3baa126`, `make test-unit-all` passed 1,594 runner, 141 daemon and 3,378 library tests, totaling 5,113 Rust tests.
All four package coverage gates remain at 100%: app 4,653, website 133, CLI 1,997 and design system 522 tests.
Sixteen CLI live tests remain explicitly skipped; `/tmp/m236-decisions-post-main-unit.log` records the complete results.

The provider fixture now accepts an intact eighteen-option compiled question above 1 KiB, then rejects individually valid fields whose escaped representation exceeds 16 KiB.
The authored-question limit remains separate and unchanged.

The first integration build exhausted worker disk before tests ran.
Targeted `cargo clean` reclaimed 7.8 GiB, then another 2.7 GiB after the complete unit run.
No source, coverage report, floor or test exclusion was removed to recover space.

Two further targeted cleanups reclaimed 1.3 GiB each around post-merge verification; `/tmp/m236-decisions-post-main-clean.log` records the second cleanup.

Subsequent failures exposed missing migration expectations, nonstandard private column names and unregistered assembled SQL.
Their fixes preserve every on-disk statement check and add both assembled queries to the existing positive preparation inventory.
The native fixture also now prices its coding model and uses the actual steer `message` field.

The Resume adjustment passed ten desktop/mobile production-component browser cases in `/tmp/m236-ui-polish-browser.log`.
The same Resume geometry check fails against the previous component in `/tmp/m236-resume-parent-red.log`.
These checks use controlled data and do not establish live DT acceptance.

### Component file responsibilities

| Owner | Change |
|---|---|
| `schema/2280_runner_decision_calls.sql`, `2290_runner_decision_payloads.sql`; migration, role and upgrade inventories | Retain financial call identity after live-owner deletion; cascade private request/results with their owners; enforce operation, retry and sequence uniqueness. |
| `afd_fleet/src/task/decision/{ownership,prepare,compile,request,context}.rs` | Resolve trusted lease/source/task state and accepted criteria; compile complete bounded questions and native assessment identity. |
| `afd_fleet/src/task/decision/{allocate,attempt,intent,fund,dispatch,row,replay,sql}.rs` and statements | Commit intent, bound retries, confirm whole-work credit, resolve fresh keys, issue once and return credential-free replays. |
| `afd_fleet/src/task/decision/{account,journal,record,semantic}.rs` | Normalize usage, price the frozen quote, atomically record the original payer's charge and separately judge current answer usability. |
| `afd_fleet/src/task/{transaction,fetch,observe}.rs`, error shell and existing lease owners | Bound database waits, release cancelled sessions and pool permits, record closed outcomes, include unresolved calls in holds and preserve those holds until reconciliation. |
| `afd_billing/src/{rates,budget}.rs`, `{price,sql}/`; `afd_credential/src/{provider/decision,vault/mod}.rs` | Reuse existing rate, budget, biller and encrypted-vault facilities on the caller's bounded connection. No second ledger or secret store. |
| `afd_wire/src/{decision,task}/`, `paths.rs`; `afd_api_runner` decision handlers; `afd_http` runner routes; `agentsfleetd` plane | Share typed answer validation, expose the two lease-scoped decision routes and supply only server-owned administrative credential context. Regenerated OpenAPI preserves every existing path. |
| `ui/packages/app/components/{layout/DraftSidebarItem,domain/SteerComposer}.tsx`; browser and unit owners | Align Resume and its reachable Delete action; shorten guidance to “Change fleet defaults in Edit setup.” Shared typography and theme remain. |
| Native lifecycle scenarios, HTTP refusal matrix, provider limits and SQL inventories | Prove complete flows and residual state together; retain negative controls and use actual native implementations and API-role databases. |
| Owning architecture pages, metrics census, active spec and this ledger | Record actual boundaries, bounded funding diagnostics and operation cardinality, measured evidence and remaining work. |

## Checkpoint encryption component

`afd_crypto::checkpoint` defines borrowed native object scope, wrapped key metadata, temporary key ownership and encrypted chunks.
The daemon wraps each new object key with the existing envelope; transfer code consumes that temporary key during object encryption or decryption.
The code moves the existing object allocation and checks scope through authenticated associated data.

The byte format separately binds wrapping purpose/version/nonce and data purpose; `sandbox_evaluation.md` records its exact fields.
The AES-GCM `zeroize` feature clears AES and authentication state on drop.
The shared envelope now guards its entropy destination before filling it, preserving its stored format.

The final scoped command `make test-unit-rustd-runner _RUSTD_UNIT_RUNNER=afd_crypto` passed 68 tests after the key-consumption correction.
`/tmp/m236-checkpoint-crypto-final-unit.log` records the result; this is component evidence, not a repository-wide verification result.
Four new scenarios combine an independent format vector, allocation checks, empty/small/8-MiB chunks, ownership changes, corruption, entropy failure and malformed keys.

Node 24.19.0 with OpenSSL 3.5.7 independently produced the fixed ciphertext/tag vector.
The scenario also checks a reused buffer's previously initialized tail, then decrypts the same allocation.
A documented test-only observation reads that initialized range; production crypto adds no unsafe block or mutex.

Astra's requested extra-high source review found the spare-capacity retention issue and the envelope's inherited unguarded entropy buffer.
Both corrections were verified closed against the actual worktree; no material component finding remained.
The reviewer ran no builds or tests.

An inner-loop mutation changed the full spare-capacity wipe to an empty slice.
The complete vector/allocation scenario failed at its retained-plaintext assertion, then passed after byte-exact source restoration.
Logs are `/tmp/m236-checkpoint-crypto-mutant.log` and `/tmp/m236-checkpoint-crypto-restored.log`.

Conformance identified explicit key consumption and two mechanical test-helper findings; their corrections add no lint suppression.
The source now drops the transferred key immediately after constructing the guarded cipher.
`make lint-rustd` passed, including formatting and workspace Clippy over all targets/features; `/tmp/m236-checkpoint-crypto-lint-3.log` retains its results.

No R2 SDK dependency or live transfer is claimed by this component.
Native registration, quotas, manifests, transfer, restore and the live private-bucket proof remain pending.
No complete recovery Dimension is marked done.

## Published-head coverage result

The coverage run for published head `2ace1f3a1` completed all four Rust coverage shards successfully.
The [merge job](https://github.com/agentsfleet/agentsfleet/actions/runs/41815241326/job/126531170305) failed the existing floors.
Its merge checkout is `06156bdfe677d47d752c33a79e378b48c55ba4ae`, combining that head with main `bf3baa126`.

| Measurement | Observed | Required |
|---|---|---|
| Workspace line coverage | 81,517 / 82,006; 99.4037%; 489 missed | 99.5% |
| Changed Rust lines | 4,678 / 4,932; 94.8500%; 254 missed | 99.01% |
| Touched files | 48 / 159 below the per-file floor | Every touched file at least 99% |

This is the measured result for the published head, not the later local maintenance work.
No floor, exclusion or coverage report has been changed to hide the misses.
Missing native decision, HTTP and configuration behavior remains work before readiness.

## Abandoned-decision maintenance component

The same `Tasks` owner now supplies `TaskLifecycle::maintain_decisions` to the existing billing loop.
The pass bounds candidates to 100 and total time to ten seconds; individual statements retain their existing three-second bound.
The financial journal records proved-unsent expiry under the shared parent/slot/task/call locks, preserving issued uncertainty and original ownership.

Five complete database scenarios cover released/future/unknown reservations, deletion, cancellation, delayed issuance commit and competing expirers.
The query-plan check uses the exact static production statement and a test-local planner setting; it requires an indexed deadline range.
Native metrics add `decision_sweep`, bringing each operation/outcome family to 88 bounded series.

Astra first found a volatile-clock range scan, an unqualified text tie-breaker and missing stale-candidate race coverage.
The query uses `statement_timestamp()` and orders the qualified source UUID.
Initial transaction races observed blocking; corrected maintenance cases now expect contention skips.

Per-item expiry logs use debug, with aggregate counts and overdue age at info.

`make lint-rustd` passed after the final SQL qualification; `/tmp/m236-maintenance-lint-5.log` records formatting and workspace Clippy success.
The scoped changed-crate unit selection passed 429 tests before that SQL qualification; `/tmp/m236-maintenance-unit.log` retains its output.
The first integration attempt stopped during compilation for the query correction; it supplies no live test result.

The second build failed before tests after exhausting disk space; `/tmp/m236-maintenance-integration-2.log` records the linker and filesystem errors.
`cargo clean` removed 23,775 build files, recovering 19.3 GiB; the following `df` reported 19 GiB available.

The clean `make test-integration-rustd` gate passed: 1,507 regular tests and three exclusive tests; exit zero.
`/tmp/m236-maintenance-integration-3.log` retains that pre-fairness result, including all five initial recovery scenarios.

A focused Astra follow-up confirmed a further liveness defect: one blocked or malformed oldest candidate can starve every later fleet.
The correction keeps a typed keyset cursor in the existing billing loop, advances before attempts, skips contention and isolates row failures.
Expected contention, failed rows and recovery age need separate bounded diagnostics.

The starvation regression failed against the prior implementation at its one-second unrelated-recovery deadline.
`/tmp/m236-maintenance-fairness-red.log` records that assertion failure with zero passing tests and one failing test.

The corrected `make test-integration-rustd` gate passed: 1,511 regular tests and three exclusive tests; exit zero.
`/tmp/m236-maintenance-fair-integration-2.log` records the full rerun after the fixture correction and exact SQL restoration.

`orly gate verify` passed after resolving the local UI fixture path and evidence paragraph formatting.
The changed-crate unit selection passed 430 tests, including the capturing-exporter scenario.

The loop owns `DecisionMaintenanceCursor`; the `TaskLifecycle` trait borrows it mutably for each bounded pass.
Shared `locks::Mode` retains ordinary waiting for receipts and uses `NOWAIT` for maintenance without changing lock order.
No mutex or new worker is added; the cursor moves each candidate identity without copying it.

Astra also identified indefinite postponement when new arrivals keep every continuation nonempty.
Each scan cycle now retains its initial database-time cutoff across pages and cancellation; short or exhausted pages reset the cycle.
Arrival-between-passes cases cover both short and full pages, without backdating new work into the earlier cycle.

The aggregate failure warning retains the first registered error code, alongside its counts.

Astra's correction review reported no remaining material finding in this maintenance component.
This was source review under the requested extra-high configuration; the reviewer ran no tests.

Nine recovery scenarios cover mixed financial state, erasure, deadlines, competing passes, pending issuance, a blocked earlier fleet, malformed state and cancellation.
The backlog case creates 101 native unissued calls from distinct admitted sources; another fleet must recover beyond that entire busy page.
The exact production query's planner check covers both initial and continuation bounds.

The busy-issuer case proves contention safety; it does not independently mutation-test the fresh issuance/accounting recheck.

The initial live backlog case failed because earlier fixture inputs remained unsettled and correctly blocked later claims.
The fixture now completes each preceding claimed source; production input-receipt settlement remains pending.
The scoped recovery selection passed all nine scenarios after this correction; `/tmp/m236-maintenance-fair-focused-2.log` records nine passed and zero failed.

Astra's final fixture review found no material issue and confirmed that native FIFO guards remain unchanged.
The seeded source-completion state proves financial recovery; it does not establish production input settlement or the runner loop.

Removing the cycle cutoff made the complete backlog scenario fail at `(4, 3, 1, 0)` versus `(2, 1, 1, 0)`.
The mutation driver restored the exact original SQL bytes; `/tmp/m236-maintenance-cutoff-proof.log` records the result and restored digest.
`/tmp/m236-maintenance-cutoff-mutant.log` retains the assertion failure, not a compilation failure.

`agentsfleet_task_decision_recovery_attempts_total` has four bounded outcomes; the overdue histogram records candidate age beyond its original deadline.
A capturing-exporter scenario checks counts, age totals and operation latency under exact label sets.
Structured log checks verify aggregate severity/counts, expected contention and absence of fixture credentials or question text.

The component is verified locally; combined-main verification and pushed-head checks remain pending.

## Runner ending deadline and additional activity scope

Merge `0e31bc918` integrates main `1920ff345` and PR 802 without rewriting the feature branch.
The merge retains both provider dependencies and restores the tenant plane's required native task dependency.
The latest freshness check found main unchanged.

Shell and Git now bound their ending wait by command time plus shared kill/drain grace and named reply slack.
Compile-time assertions compare the default wait, maximum command and ending allowance with the existing lease/runtime/shutdown bounds.
Spawn retains its separate native reply deadline; no independent whole-turn deadline is claimed.

Missing endings retain partial output and return `interrupted`, with no exit code or claim of remote death.
Expiry cancels the existing executor link, interrupts pending calls and processes, and refuses later calls immediately.
Confirmed endings and ordinary caller cancellation preserve connection reuse.

Owned output snapshots make each read finite without copying buffered bytes or holding the channel lock during consumption.
Ready notifications consume Tokio cooperative budget; empty frames add no buffer entries.
The existing `Executor` trait and admission/lease path remain unchanged.

The warning `process_ending_deadline_exceeded` records safe identities and timeout durations.
Computed fields are bound before logging; existing tool-duration accounting records the returned failure.
No command, output, credential or unbounded metric label is added.

Astra's extra-high source review found indefinite draining and notification-loop starvation; both were corrected.
Its final native-link review found no material production issue and requested a one-poll test for immediate refusal.
That proof now uses `now_or_never`; source review ran no tests or kernel probes.

The initial silence regression failed before the guard; the cooperative-budget regression also failed at its expected assertion before budgeting.
Final native scenarios cover shell/Git silence, retained output, confirmed cleanup endings, cancellation, sibling processes, pending RPCs and post-loss refusal.
Snapshot scenarios cover concurrent later writes, missing endings, empty frames and retained terminal output.

`orly gate verify` passed again on the final runner checkpoint: formatting, workspace Clippy, dependency checks, prose and version synchronization.
Its final log is `/tmp/m236-runner-checkpoint-verify.log`; the earlier component log is `/tmp/m236-runner-native-verify.log`.

The three Rust shards passed 1,602 runner, 141 daemon and 3,400 library tests, totaling 5,143.
`/tmp/m236-runner-native-unit-all-3.log` records those results before the logging-field cleanup.
The aggregate command then failed on 29 analytics tests because the inherited environment disabled analytics.

`make test-coverage-all` then passed after unsetting only that inherited switch; no test or application source changed for this correction.
App 4,653, website 133, CLI 1,997 and design system 522 passed; the CLI retains 16 explicit live skips.
All four coverage gates reported 100%; `/tmp/m236-runner-native-ts-coverage.log` retains the result.

The final logging-field cleanup also passed the complete runner shard: 1,602 tests, exit zero.
`/tmp/m236-runner-final-unit.log` records that fresh result.

The clean `make test-integration-rustd` gate passed 1,513 regular and three exclusive tests; exit zero.
`/tmp/m236-runner-final-integration.log` records fresh schema/cluster reset, migration and both passing selections on the integrated source.

Disk pressure during integration was relieved with `cargo clean` in the separate, inactive coverage checkout.
It removed 3,658 cached build files and recovered 1.2 GiB; the active target and saved coverage reports were preserved.

Earlier attempts identified container preconditions: PID 1 left killed children unreaped, and the shell inherited a restrictive file-creation mask.
The unchanged process-group tests passed with `docker-init -s`; the complete Rust rerun used that reaper and `umask 022`.
No assertion, timeout, coverage floor or production sandbox policy was weakened.

Indy superseded the separate-PR request with “I want all this in this PR” and “no new PR”.
The runner correction and schedule activity remain in implementation PR 803; public pages remain in existing docs PR 241.
The separate signal-isolation audit remains pending, with no measured exploit or reviewed runtime mechanism.

The schedule activity specification records atomic ordinary/setup mutations, sync outcomes, queued fires, native run details and replay limits.
Astra found and closed missing recipe writes, deletion/expiry distinctions, offline visibility and run-link assumptions in the design.
It reported no remaining material specification finding; schedule implementation and its storage/browser proof remain pending.

The existing unsigned schedule-header boundary remains unresolved and separate from activity.
Architecture now states acceptance-time database provenance without claiming signed schedule/revision identity.
Other missing activity types remain inventory only, as Indy requested.

## Schedule implementation and documentation publication

Schedule mutations and fenced synchronization now record typed receipts on their owning transactions.
Setup recipe receipts join the authoring commit; accepted fires retain their original message, description and admission identity.
Queued schedule fires appear in the existing mixed history before leasing, without admitting work from receipts.

The app decodes each new receipt kind, renders plain lifecycle/fire rows and opens the selected native run record after leasing.
Current schedule controls retain retry actions and server-computed timing; activity and actions use the shared 14px text token.

Unknown receipt fields stay hidden in both single and grouped rows.
Open run details refresh on native status/update changes, preserving identity and cancellation guards.

Astra found grouped payload disclosure and stale open details; both were corrected with regression scenarios.
The first storage run also exposed missing-parent lookup bypassing the existing lost-lease guard; the transaction now preserves that refusal.

The next run passed that regression and the new schedule scenarios, then failed in the once-run history fixture on mixed text/JSONB cached bindings.
The shared statement's callers now bind SQLx JSON consistently.
The corrected checkpoint passed `make test-integration-rustd`: 1,523 regular and three exclusive tests in `/tmp/m236-schedule-integration-final.log`.

Astra subsequently found a parent/lease lock cycle in the new history scope.
The added real-row regression failed at native finalization with Postgres `40P01` using `FOR UPDATE`.
`/tmp/m236-schedule-lock-parent-red.log` records the database assertion, rather than a generic scenario timeout.

With `FOR NO KEY UPDATE`, `make test-integration-rustd` passed 1,524 regular and three exclusive tests.
`/tmp/m236-schedule-storage-verified.log` records the corrected result.
It proves native finalization and session foreign-key progress; it does not show a terminal report or runner guard ran.

Existing native decision scenarios now verify complete typed wire round-trips and compact/pretty diagnostics for dispatches and saved replays.
They preserve call/model/revision identity while checking actual credentials, endpoint, question, evidence and answer-key values stay hidden.
Astra closed both proof gaps in those checks; the real storage suite runs them without adding field-only test cases.

`make test-coverage-all` passed after the final details-freshness correction: app 4,669, website 133, CLI 1,997 and design system 522 tests.
All four package coverage gates reported 100%; the CLI retains 16 explicit live skips.
The app reports 12,650 statements, 8,703 branches, 3,379 functions and 10,449 lines covered in `/tmp/m236-schedule-ui-coverage-verified.log`.

All three new freshness scenarios fail against the former state/body selection, with 4,666 other tests passing.
`/tmp/m236-details-freshness-parent-red.log` records that assertion failure; the corrected source was restored before final coverage.
`make lint-app` passed in `/tmp/m236-schedule-app-lint-final-2.log`.

`make dry-app` passed all 4,669 app unit tests and 58 desktop/mobile browser checks in `/tmp/m236-schedule-browser-final-pass.log`.
The selected-event read uses the documented local HTTP boundary, not a live scheduler or authenticated production read.

`make lint-rustd` passed after the diagnostic assertion-style correction in `/tmp/m236-schedule-rust-verified-2.log`.
The subsequent unit build exhausted disk while linking, before tests could run.
The failed build is recorded in `/tmp/m236-schedule-unit-libs.log`; no assertion or test threshold changed.

Indy's approved `cargo clean` removed 17,902 files and recovered 19.4 GiB from this worktree's target directory.
Completed storage results and saved coverage reports were retained; another worktree's target was untouched.
The cold `make test-unit-rustd-daemon-libs` rerun passed 3,398 tests in `/tmp/m236-schedule-unit-libs-clean.log`.

Docs PR 241 merged at `aa0fa1c5277ffe311d98148cf6145ee4bd1e941b`, consuming head `ed83c75`.
Lint, secret scanning and Greptile Review succeeded on that head; consecutive final reads found no unresolved review threads.

Orly doctor/work/verify/PR gates passed; 25 documentation checker tests passed, and Astra reported no residual finding across 81 manual-review rules.
Pending current-work, completion-question and schedule features carry coming-soon notes.
The docs checkout is clean on main at that merge; no further draft hold applies to that closed PR.

Indy's merge instruction:

> “PR241 - If this is green, greptile resolved in docs, merge the PR.”

Implementation PR 803 remains draft and incomplete.
The earlier `6db32ee2d` build and kernel coverage retries failed during toolbox package setup; they did not produce a passing Rust grade.

The latest “995” Rust request is conservatively interpreted as above 99.5% of changed lines.
Make and Codecov now enforce 99.51% for the patch; existing workspace/per-file floors and exclusions are unchanged.

## Owner work-item status

| Requested work | Current state and evidence |
|---|---|
| Integrate merged main before shipping | PR 798/799 ancestry is retained. Merge `9641c3cb2` adds PR 800/main `bf3baa126`; merge `0e31bc918` adds PR 802/main `1920ff345`. Published merge `e90db793e` adds PR 801/main `27deff642`; its full unit and clean storage gates pass after the corrections below. Repeat the freshness check before each push. |
| Complete DT runtime | Schema, decision adapters/catalogue/assignment, owner-controlled questions, task reads and mixed-model settlement recovery are foundations. Native physical-call accounting passes unit/integration gates; full agent/lease integration, waits, publication and recovery remain incomplete. |
| Keep approved UI and communicate work | Inline task rows and saved criteria use native reads. Ten production-component browser checks passed, including the editable Review pull requests starter, typography, disclosure and short/mobile viewports; progress/controls and full runtime demonstration remain pending. |
| Align unfinished setup actions | `components/layout/DraftSidebarItem.tsx` uses shared grid columns: title, reachable Delete, then Resume. Desktop/mobile measurements, app lint and 100% coverage pass; the same geometry assertion fails against the previous component. Logs: `/tmp/m236-ui-polish-browser.log`, `/tmp/m236-resume-parent-red.log`. |
| Shorten fleet chat guidance | `SteerComposer.tsx` now reads “Change fleet defaults in Edit setup.” The existing action, hint and theme remain; controlled browser cases, app lint and 100% coverage pass. |
| Private R2 bucket and secure keys | Non-secret bucket metadata resolves to `agentsfleet-checkpoints-dev` in the accessible `ZMB_CD_DEV/cloudflare-r2` item. No key value is recorded here. The scoped encryption component passes 68 tests and Astra review; native storage, transfer, recovery and live bucket checks remain pending. |
| Performance, concurrency and Grafana | Task reads are bounded and observable; cursor paging avoids per-task queries. Financial recovery retains progress across busy pages and cancellation. Exporter and structured-log scenarios check recovery attempts, age, latency and bounded labels. No benchmark result or complete DT performance trace is claimed. |
| Architecture and companion docs | The prior 39-page architecture and 39-page public-documentation reviews remain attributed above. [Docs PR #241](https://github.com/agentsfleet/docs/pull/241) merged as `aa0fa1c5`, including schedule activity guides and explicit future-feature notes. All local docs gates, both CI jobs and Greptile passed; no unresolved threads remained. New API pages follow the lead merge. |
| Runner hang and same-PR scope | Native ending guard, finite snapshots, cooperative waits and shared-link invalidation are implemented and reviewed. The prior runner checkpoint passed 1,602 tests; all Rust shards and all four 100% TypeScript gates have passing evidence. Clean integration passed 1,513 regular and three exclusive tests; final checkpoint verification passed. All changes stay in PR 803. |
| Schedule activity and other event inventory | Commit `69b8b8da7` publishes backend transactions, fire snapshots, queued history, readable UI and native run details. All four TypeScript package coverage gates passed at 100%; 58 browser checks passed. Corrected storage passed 1,524 regular and three exclusive tests; the cold library unit gate passed 3,398. Other event types remain inventory only. |
| Orly lifecycle and PR merge | Local Orly 0.28.0 matches the pin. [Implementation draft #803](https://github.com/agentsfleet/agentsfleet/pull/803) retains the authorized single-PR scope and remains incomplete. Docs #241 merged under Indy's explicit green-check instruction. Full implementation, Rust coverage, CI and final review conditions still gate #803. |

## Main integration and decision evidence

Published `69b8b8da7` includes schedule lifecycle activity, run-detail freshness and structured decision-diagnostic checks.
Main advanced to `27deff642` through PR 801 during its checks; local merge `e90db793e` retains both telemetry implementations.
The merge preserves bounded, repeatable-read catalogue snapshots, schedule fire descriptions and timestamps, and both metric families.

Astra found a new daemon test fixture missing the required `ModelUpdate.decision`; `decision: None` closes that compile failure.
The first merged `make test-unit-all` then exposed two daemon task histograms still declaring delta totals.
`/tmp/m236-main801-unit-all.log` records the failed cumulative-family assertion; no test threshold changed.

Both task histogram declarations now select cumulative daemon export in `docs/metrics.census.tsv`.
Runner decision histograms retain delta totals and their existing collector conversion.
Astra checked the selected providers, producer paths and census declarations and found no remaining inconsistency.

The corrected `make test-unit-all` passed 5,159 Rust tests: runner 1,604, daemon 143 and libraries 3,412.
All four TypeScript package coverage gates passed at 100% in `/tmp/m236-main801-unit-all-fixed.log`.
The formerly failing cumulative-family assertion passes in that same run.

A bundled native accounting scenario now covers absent cache-write prices and a valid price/token combination whose charge exceeds signed 64-bit capacity.
It checks retained usage/reservations, canonical replay, rejected contradictory receipts and retries, frozen prices and complete durable evidence.
Overflow diagnostics must identify the exact native call and registered error code; Astra closed the initial correlation gap.

The accounting helper calculates uncached input once and passes the checked count onward.
This removes an unreachable recomputation error without adding a new abstraction or a Mutex.

Clean `make test-integration-rustd` passed 1,530 regular and three exclusive tests in `/tmp/m236-main801-integration.log`.
The bundled unpriceable-usage scenario passes in that full storage lane.

Greptile completed its review of `69b8b8da7` with one documentation finding.
`docs/AUTH.md` and `billing_and_provider_keys.md` now distinguish the implemented reservation/receipt routes from unfinished runner integration.
Commit `f8fac4a14` publishes that correction; thread `PRRT_kwDOReIoss6zwAM0` is answered and resolved.

All ordinary CI workflows passed on `69b8b8da7`; the kernel coverage job failed before tests during pinned toolbox package setup.
Its single retry also failed during package setup, in job `126796864947` of run `41889908716`.
The other three reports are diagnostic evidence only; no complete Rust grade or merge readiness is claimed.

## Private task content and coverage diagnostics

Greptile's review of `f8fac4a14` found task titles and expanded criteria outside the app's analytics exclusions.
`TaskPanel` now uses both existing PostHog privacy classes on the containing section.
The existing three-repair browser scenario checks collapsed titles and expanded objectives, questions and criteria on desktop and mobile.

Before the correction, `make dry-app` passed 4,669 unit tests and 56 browser checks; the two intended privacy assertions failed.
`/tmp/m236-task-privacy-parent-red.log` records those failures against the original panel.
After the correction, all 58 browser checks passed in `/tmp/m236-task-privacy-browser-green.log`.

`make lint-app` and `make lint-shell` passed in `/tmp/m236-task-privacy-{lint,shell}.log`.
Astra's source review found no remaining issue with the containing privacy boundary or the bootstrap output change.
The browser proof checks rendered exclusion boundaries; it does not send private text to a live analytics service.

`make test-coverage-all` passed at 100% across the app, website, CLI and design system.
Counts remain 4,669, 133, 1,997 and 522 tests respectively; the CLI retains 16 explicit live skips.
`/tmp/m236-task-privacy-coverage.log` records every package's result without changing coverage exclusions or floors.

The `f8fac4a14` kernel coverage job `126826577076` failed during package download before measuring Rust tests.
The builder now requests native verbose bootstrap diagnostics on stderr, preserving its image-path-only stdout and original failure status.
No package, signature, image policy, retry or gate threshold changes for this diagnostic correction.

GitHub cancelled macOS job `126826581967` before assigning a runner, with no steps recorded.
Its annotation reports hosted-runner acquisition failure and macOS capacity constraints.
The other three Rust coverage shards passed; the absent kernel report still prevents a complete coverage grade.

## Native assessment recovery and source compilation

The concurrency scenario now runs both shared-operation retries and independent operation identifiers through separate owned fixtures.
Each race has 100 callers, one credential response and one original-owner charge.
Every losing response must equal the complete credential-free saved-response envelope.

The funding-outage scenario checks distinct operations before issuance, contradictory evidence, premature receipts and unchanged complete call/payload records.
Recovery retains the original call, resolves the rotated platform key and refuses a second credential response.
The completed first checkpoint passed 1,530 regular and three exclusive storage tests in `/tmp/m236-independent-decisions-integration.log`.

Native request compilation now retains `garde::Valid` instead of discarding its nonnegative-offset proof.
Standard unsigned conversion and a compile-time width assertion replace unreachable conversion errors; checked slicing still rejects actual range and UTF-8 violations.
The separate unproved plan-range conversions remain fallible.

The new Unicode scenario requires four refused ranges to leave the complete admitted source and claim unchanged, with no call, payload or charge.
It then checks the exact selected text, complete source, native questions, settlement and saved replay.
It uses native admissions and storage; it makes no live provider or full-task acceptance claim.

New credit scenarios use the actual ledger biller, native coding estimates and an existing hold before creating insufficient-credit or closed-work conditions.
Independent retries must retain one call and unchanged complete wallet/hold records.
A top-up must recover the same call and debit its original payer exactly once.

Astra found an initial fixture with no hold to close, incomplete losing-response assertions and oversized helpers.
All were corrected before the final gates, without a new suppression or Mutex.
Related timeout, outage and credit scenarios now occupy separate modules; their shared native snapshot query has one owner.

`make lint-rustd` passed on the final batch, including workspace Clippy and the unchanged lint configuration.
`make test-unit-rustd-daemon-libs` passed 3,412 tests in `/tmp/m236-decision-native-final-unit.log`.

Clean `make test-integration-rustd` passed 1,532 regular and three exclusive tests in `/tmp/m236-decision-native-final-integration.log`.
Both new native scenarios and the expanded replay/outage scenarios pass in that complete storage lane.

Published `543c6f2e0` passed Greptile review with no new blocking finding; both earlier review threads are answered and resolved.
Its first kernel job exposed Debian Snapshot HTTP 503/504 failures before testing.
The bounded retry, job `126855474806`, again failed downloading pinned packages with HTTP 503/504 responses.

GitHub also cancelled macOS job `126845858048` without assigning a runner or running steps.
Its annotation names hosted-runner acquisition failure and macOS capacity constraints.
Neither upstream failure establishes a complete Rust coverage grade; all existing coverage floors remain unchanged.

Subsequent local bounded downloads retrieved both failing packages; the next GitHub job must still confirm availability from its host.

## Decision HTTP lifecycle and current CI diagnosis

The new HTTP scenario uses native runner registration, admission, lease issuance, platform Secrets and original-owner billing through the assembled router.
It checks the first credential-bearing response, key deletion, credential-free retry, foreign-runner refusal, canonical receipt replay and model revocation.
Response bodies, retained call counts, the original wallet and the complete decision billing line must agree throughout the scenario.

The scenario reuses the existing native model catalogue fixture and provider-receipt generator.
Cleanup removes owned retained records and runners before the decision model, including unwind paths.
Astra's source review found no remaining material issue; this is component review, not whole-M236 approval or live provider evidence.

The first clean storage run passed the new scenario's response and financial assertions, then failed at cleanup with `23503`.
The named constraint, `fk_decision_payloads_call_scope`, requires deleting private payloads before their retained call records.
Cleanup now registers that owned child deletion first; `/tmp/m236-decision-http-integration.log` preserves the original failure.

`make test-unit-rustd-daemon-libs` passed 3,412 tests in `/tmp/m236-decision-http-unit.log`.
Only ignored integration cleanup and documentation changed afterwards; the following storage gate compiles and runs the corrected fixture.

Clean `make test-integration-rustd` passed 1,533 regular and three exclusive tests in `/tmp/m236-decision-http-integration-final.log`.
The new HTTP scenario, including explicit cleanup and native model deletion, passes in that complete run.
This is native HTTP/storage proof, not a live provider request, R2 recovery or full DT runtime acceptance.

Published `80fdd1b99` passed Greptile review with no new blocking finding; both earlier inline threads remain answered and resolved.
All ordinary TypeScript and browser CI jobs passed, as did the three datastore coverage jobs and macOS build.
Its daemon-library unit job exhausted hosted disk after clearing restored build files; the job annotation records the worker's disk-write failure.

The four TypeScript CI logs for that exact head report 100% coverage in the app, CLI, website and design system.
They pass 4,669, 1,996, 133 and 522 tests respectively; these counts remain separate from prior local observations.
No TypeScript source changes in this HTTP-test component.

The kernel job reached 69 tests: 68 passed, while the toolbox reproducibility case reported only a missing file.
That case discarded the builder's failure status and stderr before attempting to hash its output path.
It now checks successful exit and reports stderr before retaining the existing exact-digest comparison; the underlying build failure still needs fresh CI evidence.

The three reports for `80fdd1b99` identify merge commit `ac251993369f39c0e2247d3d13d10b665a42349a`.
Its parents and tree match main `27deff642` and the published feature head.
Their partial union observes 191 uncovered changed Rust lines; the absent kernel report prevents a complete grade.

The existing Rust patch floor remains 99.51%, with no new exclusions, suppressions or threshold changes.
The original daemon-library resource proposal is in `/tmp/m236-unit-worker-resources.diff`; its later approval and applied settings are recorded below.

## Complete four-report grade and supplied build stamps

Published `0d11c16d1` passes every Rust unit job, including the daemon-library job that previously exhausted hosted disk.
All four coverage jobs pass; the aggregate fails its changed-line and touched-file coverage floors.
At that head, the unit-worker proposal was unapplied and did not block the run.

Run `41925170490`, aggregate job `126916074123`, reports 83,085 of 83,459 workspace lines covered: 99.5519%, above the existing 99.5% floor.
Changed lines reach 5,451 of 5,626: 96.8894%, below the required 99.51%; 175 changed lines remain uncovered.
Forty-seven of 194 measured touched files also miss the existing 99% whole-file floor.

The downloaded four-report set is in `/tmp/m236-coverage-0d11c16d1/`, including `unhit.json`.
Its revision is `c1efb431e08803dcb8f29fc91b9879f5ab67efca`; the parents and tree match the pushed feature head and main.
This supersedes the earlier partial-report diagnosis without changing any coverage threshold or exclusion.

Greptile reviewed that exact feature head successfully; both earlier inline findings remain answered and resolved.
Test success and workspace coverage do not satisfy the missing changed-line floor or finish M236.

Astra identified an unused build dependency in `afd_api/build.rs`: its Git-path watch precedes the supplied `GIT_COMMIT` return.
In a linked checkout, `.git` is a file, so that watched child is absent and Cargo invalidates the build script repeatedly.
The correction registers the environment watch first and returns a supplied revision before registering the unused Git path.

The regression compiles the actual script and uses a real linked Git worktree and a directory without Git metadata.
It checks the complete Cargo output with Git unavailable and both plain and whitespace-padded supplied revisions.
Child Git environments are cleared; temporary repository operations cannot inherit the real checkout's index or worktree settings.

The scoped parent-code Make run fails at the intended metadata assertion in `/tmp/m236-build-stamp-parent-red.log`.
With the correction, `make lint-rustd` passes and `make test-unit-rustd-daemon-libs` passes 3,413 tests.
Logs: `/tmp/m236-build-stamp-lint-final.log` and `/tmp/m236-build-stamp-unit.log`.

Astra's source review finds no material issue in this supplied-revision correction.
The scenario proves emitted instructions; no measured rebuild-time saving or broader raw-Cargo fallback freshness is claimed.

## Native authority and accounting boundary follow-through

Preparation now retains the signed generation returned by its existing task lock alongside the checked wire snapshot.
The ordinary and nonblocking accounting locks decode that same generation; no second lookup or public snapshot change is added.
Native insertion reuses the locked value, removing the unreachable reverse-conversion error without an unchecked cast.

The new scenario bundle covers refused ownership/generation/source associations, late billing after pause, unusable answers and uncertain usage.
It also covers exact fleet/task money limits, unsent refunds without call-counter resets, and ten source calls including permitted retries.
HTTP input scenarios reject unsupported scalar evidence and oversized receipts before valid reservation and settlement.

The latest lint, full Rust unit and clean storage gates pass, as recorded below.
Further reviewed work groups cover served model administration, booted task readback, provider response normalization, atomic owner edits and late schedule confirmations.

The first native storage run stops with three new scenario failures; the previously extended HTTP decision lifecycle passes.
Two failures are fixture mistakes: a missing goal identity correctly answers InvalidTarget, and outstanding reservations use `accounted_at IS NULL`.
The third reveals a real persisted-score defect: Serde's nested tagged enums cannot decode the numeric distribution keys with the original map adapter.

`serde_with` 3.24.0, the latest stable version inspected for this batch, supplies the standard string-key adapter with only its `std` feature enabled.
The public numeric map type and canonical JSON remain unchanged; no manual parser is added.
A complete receipt scenario covers all three answer types, usage, text/JSON round trips and invalid integral keys.

The native unusable-result scenario now compares the entire frozen request and saved provider result through credential-free replay.
Astra's source review finds no material issue in the correction; its function-length finding is fixed by reusing the existing snapshot helper.
Later verification below covers the corrected source.

Additional provider scenarios cover direct text input, encoded-question expansion, conflicting cache representations and invalid score totals.
Owner-edit scenarios retain the accepted definition and digest after same-sized role substitution or combined-question overflow.
Schedule scenarios cover repeated retirement and late confirmations after parent deletion while another owner's full state and history remain unchanged.

Targeted `cargo clean` removed 1,688 old package build files, reclaiming 6.4 GiB in this worktree.
The output is recorded in `/tmp/m236-native-boundaries-clean.log`; dependency caches and other worktrees remain untouched.

The served model-administration lifecycle now covers unsafe creation, complete decision-only readback, immutable-revision refusal and a reviewed replacement.
It then installs a distinct coding schedule, refuses its removal while omitted inference remains configured, disables decisions and deletes the owned model.
An independent native model keeps its complete public view throughout; normal-path cleanup removes both identities.

The existing administration fixture moves into its own test module, leaving the parent below the repository's file limit.
Astra's source review finds no material issue in these HTTP/storage checks.

The native update drops a redundant zero-row fallback.
Its prior `FOR UPDATE` locks the same existing model; the update has no additional predicate or suppressing trigger.

The full `make test-unit-rustd` gate passes 5,161 tests: 1,604 runner, 143 daemon and 3,414 daemon-library tests.
This includes the provider scenarios and the complete nested receipt regression; the earlier 3,413 count does not cover this later source.
Logs: `/tmp/m236-score-recovery-unit.log` and `/tmp/m236-score-recovery-lint.log`.

The clean `make test-integration-rustd` run passes 1,538 regular scenarios and 3 exclusive scenarios.
The new fleet boundaries, complete HTTP decision lifecycle, served model administration and deleted-owner schedule cases all pass.
Log: `/tmp/m236-score-recovery-integration-final.log`.

Astra also found colliding numeric score keys: `"0"` and `"00"` could replace one another after conversion.
The same standard crate now rejects duplicate converted keys, with no manual parser or additional dependency.
Wire and HTTP scenarios cover both `"00"` and `"+0"` collisions before stored or financial changes.

The scoped Make mutation proof removes only duplicate protection and fails at the intended receipt assertion.
Its output shows the overwritten distribution, `{0: 0.6, 1: 0.4}`; the exact guarded source is restored and hash-checked afterward.
Log: `/tmp/m236-score-collision-mutation-red.log`; the full Rust unit run supplies the green result.

Astra's final source pass finds no remaining issue in this correction batch.
Coherent test helpers also fix the reviewed function-length findings, including the older owner-policy round-trip function.

Indy approved the daemon-library workflow resource proposal: "Approve these resource settings".
Only that job gains `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` and `CARGO_BUILD_JOBS=2`.
Its tests, cache rules and coverage thresholds remain unchanged.

`make check-gh-actions-valid` passes actionlint, referenced Make targets and 22 release-candidate checks.
Log: `/tmp/m236-unit-resources-actions-check.log`.

The preceding storage build filled the local filesystem and failed during linking, before testcase results.
A full `cargo clean` removed 19,422 build files and reclaimed 19.4 GiB in this worktree.
Only the failed storage gate was rerun; the already-passing source and unit evidence remains valid.

`orly gate verify` passes all declared non-test checks, including `make lint-all` and `make check-version`.
Log: `/tmp/m236-score-recovery-verify.log`; the pinned product versions match 0.73.0.

## Full-page financial recovery and lease completion

The full-page scenario creates 102 legally unissued calls through native reservation, with distinct admitted sources and real funding refusals.
Its first 100-row scan recovers 99 calls while deferring a locked older owner.
An empty seek then wraps to that owner and two later arrivals; every call becomes accounted `NotSent` without a charge or wallet change.

The settlement scenario records a known receipt while the existing fault adapter's real ledger transport is unavailable.
Canonical replay preserves the receipt and unpaid line; the normal billing sweep repairs settlement after the actual grace boundary.
A second unknown call retains the original hold through a normal terminal report and eligible sweep, then late known usage settles once.

The next sweep releases that hold once; later sweeps and receipt replay leave financial state unchanged.
Assertions cover original payer/source/model attribution, distinct decision sequences, cumulative task usage, unchanged hold ownership/ceiling and safe diagnostic fields.
The existing report fixture is shared through test-only visibility; no new production seam, mutex or billing implementation is added.

Astra's requested extra-high source review finds no material issue in either composed scenario.
`make lint-rustd` passes; log `/tmp/m236-recovery-bundles-lint.log`.
`make test-integration-rustd` passes 1,540 regular scenarios and 3 exclusive scenarios; log `/tmp/m236-recovery-bundles-integration.log`.

Both new scenarios pass within the existing fixture and maintenance bounds.
The preceding full 5,161-test unit result remains recorded separately; this later batch changes ignored storage scenarios and documentation only.
The continuation point above preserves unfinished scope for the fresh session Indy requested.

Final `orly gate verify` passes the declared non-test checks, including lint and version validation.
Log: `/tmp/m236-recovery-bundles-verify.log`; all pinned product versions match 0.73.0.


## Resumed control API slice

The restored environment contained only `/workspace/agentsfleet` at `27deff642`.
The requested `/workspace/agentsfleet-m236-design` worktree was recreated from the published feature branch at `ebfb3b722`.
`orly doctor` passed with matching installed rules and declared verification commands.
Fresh main is `65372f505`; its six commits contain the M239 documentation merge.
The local merge retains both Durable Task scenario entries and the new fleet-to-fleet entry; its commit remains pending component verification.

The opening GitHub check uses the exact published head, not a previous run:

| Evidence | Opening observation |
|---|---|
| `get_pr_info`, PR #803 | Draft; head `ebfb3b722a911600bb00e9677e4f2b003e2169c7`. |
| Workflow `41948469205`, job `126986426824` | Daemon-library unit job failed in `test_migrations_precede_serving_and_refusal_preserves_running_daemons`: daemon port occupied. Rust tests did not run in that job. |
| Workflow `41948469274`, job `126986426534` | Kernel measurement failed fetching a Debian Snapshot package with HTTP 504. The full log is `/tmp/m236-head-kernel.log.gz`; no complete kernel report or combined grade was produced. |
| Workflow `41948469274` | Runner, daemon and substrate coverage jobs passed. Those partial reports do not establish a complete current-head Rust grade. |
| Other exact-head workflows | Governance, secret scanning, lint, test, dry and dry-smoke passed. Runner, daemon and secret-wiping unit jobs also passed. |
| Greptile | Both inline threads are resolved. The latest summary names `c8937f369`; it does not establish review of `ebfb3b722`. A fresh pushed-head review remains required. |

The predecessor environment's `/tmp/m236-coverage-c8937f369/` directory is absent from this restored environment.
Its recorded passing results and provenance above remain historical evidence; retrieve matching artifacts through workflow `41945042540` before using its gap list.
No previous passing gate was discarded or represented as a current-source result.

The local control slice adds `PATCH F/tasks/{task_id}` through the existing `TaskLifecycle` owner.
It requires FleetWrite, a strong current resource tag and the expected task generation, checked while holding the native task row lock.
Native controls preserve accepted questions, objective, cumulative usage and deadline; resume uses the existing admission queue.
The new HTTP body is bounded to 128 KiB, and malformed private field names are not logged.
Reviewed goal-revision adoption, browser/CLI controls and the remaining runtime owners are still unfinished.

New composed scenarios assert one winner among 100 overlapping original-tag controls, unchanged work after stale or invalid controls, one continuation after resume retries, foreign-child refusal and retained exhausted allowances.
These assertions remain unverified until their integration lane completes.
The existing native control scenarios now use the same resource-tag and generation interface as the public route.

`make lint-rustd` passed source rules, formatting, workspace Clippy and dependency checks before the last negative scenario and test-helper refinement.
The first attempt stopped on missing pinned cargo-shear; installing 1.14.0 under `/tmp/m236-tools` repaired local tooling without repository changes.
The first compile found three new test-code lint errors; explicit Arc cloning and an assertion corrected them without suppressions.
Final test-source lint, unit, integration, generated OpenAPI and publication remain pending.
The initial daemon-library lane completed at `/tmp/m236-controls-unit.log`; its deciding count was 3,415 Rust tests passed.
Its launcher/bootstrap prechecks passed 25, 14, 5 and 7 tests; that local pass does not resolve the hosted port-collision failure.

GNU Make's exported computed ports caused repeated shell expansion before any recipe started.
Commands use the exact ports returned by `scripts/test-infra-ports.sh` and the existing documented offsets as environment values; no Make recipe or gate was edited.
One Rust lane runs at a time with the declared cloud Cargo settings, umask 022 and the supplied child-reaping wrapper.
TypeScript source remains unchanged; the previous 100% results retain their recorded scope.

The remaining delivery sequence is controls and reviewed adoption; production bindings and input receipts; automatic intent/goal execution and task-aware report settlement; GitHub effects and waits; incremental R2 recovery; browser/CLI integration; complete coverage, review and live acceptance.
PR #803 remains draft. No milestone completion, final PR gate or merge readiness is claimed.


Indy's Nov 05 clarification requires a Codex-style activity timeline in both chat and CLI, beyond a compact Current work summary.
The active specification and activity owner now name concise progress explanations, expandable commands/output/diffs, check results, exact-head publication/waits, capture/restore stages and truthful recovery.
These UI/CLI behaviors remain acceptance requirements, not implemented or observed outcomes.

The initial `make test-unit-rustd-daemon-libs` run passed 3,415 Rust tests; log `/tmp/m236-controls-unit.log`.
Its source predates the newly identified precondition-status correction, so it is preserved as component evidence rather than final-source verification.
Source inspection found that the shared conditional refusal renderer uses each code's registered status and returns the fresh tag in the problem body.
Task preconditions need registered 412/428 statuses; exhausted allowances retain their existing 429 behavior.

The first precondition correction attempted `UZ-DT-013` (412) and `UZ-DT-014` (428), including native problem entries and stale-outcome metrics.
The shared conditional renderer carries the fresh tag in the problem body; tests follow that existing representation.
The correction is under focused unit verification for `afd_core`, `afd_fleet`, `afd_api`, `afd_http`, `afd_api_tenant` and `afd_api_wire`, through the existing Make lane.
That focused selection does not establish a new full-workspace unit result.
Worktree-scoped `cargo clean` reclaimed 21.7 GiB before the next lane; third-party build caches and passing logs remain available.


The focused unit run exposed the existing registry invariant that only `UZ-AUTHOR-004` owns HTTP 428.
It finished red with 76 passed and one failed core test; `/tmp/m236-controls-focused-unit.log` is retained.
The duplicate 428 code was removed and task controls reuse the shared precondition code; `UZ-DT-013` retains 412.
The registry assertion, allowed status set and all test/coverage floors remain unchanged.
Following the repository's wider-retry rule, the full `make test-unit-rustd` lane is running at `/tmp/m236-controls-full-rust-unit.log`.

The component standards review found three concrete issues: sequential-only resume replay, missing declared validation sentences and oversized changed test functions.
The corrections add 100-caller resume and cancel races through the served route, a constant validation sentence table, and smaller replay/terminal helpers using the shared resource tag.
A source-only follow-up review and clean native integration results remain pending.

All four historical coverage archives were recovered through GitHub's exported artifact references after the direct CLI redirect was refused.
Their SHA-256 digests match the artifact metadata, and all four revision sidecars name `d12a443729916f22f8c667afca8062974a8c8f0b`.
That measured revision and `c8937f369` share tree `31fe7522c1a821e191859067c3dc689a6d44a436`.
The unchanged repository parser reproduces 5,506/5,623 changed lines, all 117 unhit locations and 32/194 touched files below 99%.
Local copies, the original grade log, `reproduced-grade.json` and the complete gap list are now restored under `/tmp/m236-coverage-c8937f369/`.
This is recovered historical evidence, not a new test run or a current-source grade.

## User-authored setup correction

Indy clarified: "user will author" and "my vision was to have the user chat and provide the input. The initial prompt templates as i told you will be available in `Review Pull requests` when user selects that a template text will popup on the chat box." He then authorized the correction with "Yes go".
Delete the five provider `prompt/*.md` files and their loaders; do not relocate their full text into another static prompt. Preserve the editable starter catalog and native authorization, tool validation and owner-approved setup revisions. Add the exact requested newest-message decision check, no-edit rejection, bounded fallback recording and setup-chat billing. Preserve the separate soak branch's setup-chat admission SQL. Required final commands are `make lint-all`, `make test-unit-all` and `make test-integration-rustd`, with exits and counts.

Indy directed code completion before repeated broad checks. The in-progress `make lint-rustd` was terminated on that instruction; `/tmp/m236-prompts-lint-rustd.log` is preserved and is not a passing result. Final required checks remain due after implementation.

The code-first continuation removes the five compiled Markdown prompts, adds a native setup edit-intent journal/check and source/soak regressions, and shares the existing runner decision resolver. Known decision charges retain `SetupChat` attribution; unknown decisions keep their holds and accounting remains explicitly unresolved while a known chat answer can publish. A no-edit refusal refreshes the native continuation digest before replay. Source-review corrections are applied; none of this continuation's new source has passed verification yet.
The next native controls work adds explicit goal-revision adoption on the existing PATCH route and its retry receipt, with unchanged execution policy/target/limits and invalidated assessment generation. A complete pending-input read route is wired under `L/tasks/inputs`, including conversation scope, live lease/fence, ascending human FIFO, candidate summaries and keyset paging. Binding/input-receipt mutations, automatic runtime loop, GitHub effects/waits, R2 and CLI/browser activity remain unfinished. No broad check is running. No commit or push has been made in this continuation.

Browser task detail now wires conditional Pause/Resume/Cancel through the existing authenticated same-origin proxy; controls await confirmed daemon results and refresh on stale/uncertain results. The task detail response carries its native ETag in the body as well as its header for the existing body-only CLI transport. CLI source adds `agentsfleet fleet tasks list/show/pause/resume/cancel/adopt`, with typed response validation, readable saved questions and no mutation retry. TypeScript source has now changed, so historical 100% coverage remains historical and cannot grade this continuation. New UI/CLI tests and the final full gate pass remain due after code completion.

The browser also now has an explicit saved-criteria review/apply flow: it reads the shared fleet's immutable setup identity, displays the proposed completion questions, and sends the selected revision only after the person chooses “Use these criteria”. Shared fleet output now includes `setup_revision_id`; the existing stored setup resolver supplies it rather than introducing another lookup. Native adoption refuses changed execution policy and leaves task target, cumulative limits and usage intact. This is source implementation, not passing browser or integration evidence.

Indy added: "Also try and see how the postgres queries can be made optimal when you design them." and "and be adversarial on the file written". Source design now pairs pending-input reads with the existing unsettled fleet/sequence index and adds a forward partial unfinished-task ordering index. The new edit-decision DDL omits an attempt-only index because the unique attempt/usage-sequence index already serves that prefix. Adversarial read-only source review is active. Do not claim query performance before final EXPLAIN/measurement evidence, and do not restart broad checks during code completion.


## Native PostgreSQL, publication and checkpoint source continuation

Indy directed PostgreSQL completion before gates and an adversarial simplification/concurrency review after each item. No build, test, lint, formatting lane, commit or push has run during this batch. Published head remains `ebfb3b722`; these changes are uncompiled and unverified.

Native GitHub source now includes explicit existing-PR mode, reviewed workflow digests, narrowly scoped minting, guarded fixed-origin HTTP, tested-tree reconstruction and one expected-head mutation. Immutable effects reserve their ordinal before dispatch; uncertain outcomes never repeat the mutation. Bounded encrypted read-only reconciliation custody can outlive task/workspace deletion and is supervised separately from coding execution. Remaining publication corrections include default-branch workflow review, clipping the physical mutation to remaining native/token deadlines, and a coded 409 uncertainty body. Signed completion matching and wait ownership are still absent.

Mixed-input source includes native whole-message/item proof checks, one atomic binding path, task-local input sequencing and immutable apply/queued/answered receipts. GET inputs now also recovers already accepted meaning beyond the incorporated cursor, so accepted-but-unapplied work is not hidden by interpreted admission filtering. The receipt replay query checks both item/mode and operation identity. Native generation/current context in binding/input responses, task-aware lease/report transitions, Slack linked-person authorization and the automatic supervisor/agent loop remain unfinished.

Checkpoint source now lives under `afd_fleet/src/task/{checkpoint,storage}` with supervisor-only `afd_wire::task::checkpoint` types and nine runner method/path adapters from the approved API. R2 uses `object_store` 0.14.2 with the guarded reqwest transport, one vault/SDK session per batch, disabled SDK retries, create-only activation proof and absolute whole-operation deadlines. Credentials remain in the platform vault. Optional `R2_CHECKPOINT_STORAGE` contains only endpoint, bucket and `credential_name`, and requires `PLATFORM_ADMIN_WORKSPACE_ID`; no live bucket activation has been performed.

Slots 2410–2530 register identity keys, quota, retained bases, private capture plans/pages, opaque purge receipts, encrypted objects, scoped refs/pins, the deferred task checkpoint pointer, retained plan-operation receipts and an indexed owner-cleanup deadline. Slot 2520 retry identities survive private-plan retention. Task checkpoint allocation increments its native sequence rather than scanning/depending on retained private generations. Role and immutability inventories were extended with the tables. Final statement preparation, grants, migration execution and realistic plans remain due.

Registration serializes workspace quota before ordered object locks and uses bounded set reads/inserts. It creates one native random object identity, wrapped key and fixed nonce per missing keyed-content identity. Ciphertext descriptors freeze before signed URL exposure. Transfer verification checks ciphertext SHA-256, authenticated decryption, plaintext size and SHA-256 before one batched quota transition. Four independent storage operations run concurrently outside SQL transactions; no new shared mutex was introduced.

Immutable manifest pages validate paths and chunk sizes. Manifest commit streams retained base plus overlay pages, verifies expanded counts, parent collisions and chained symlink resolution with cycle bounds, and binds ordered conversation objects to the file generation. Durable refs protect objects until owner retention removes them under the same task lock used by pin acquisition/commit. Object collection rechecks refs after acquiring object locks; quota stays charged until remote DELETE is acknowledged. Opaque purge custody retains charged bytes/accounting disposition across metadata deletion and lost acknowledgements. Bucket references bind account endpoint plus bucket, so a configuration change cannot silently redirect old purges.

Read-only review found and source corrections addressed: missing purge accounting, cross-workspace refs, an unfinished base stranded after task deletion, delta totals checked without base expansion, and chained symlink escape. These are reviewed source corrections, not passing adversarial execution scenarios. Pin expiry/retention and batch cancellation still need executable race coverage. Owner-retention error isolation, combined manifest bounds, source-size/mechanical conformance and current TypeScript fixtures/coverage also need final review.

Supervised private collection has its own cancellation-bound pass so slow R2 deletion cannot delay GitHub evidence reads. The daemon boot inventory names both task maintenance and storage collection. Its implementation and the new native API source have not been compiled. Public schema/topic/API docs, OpenAPI generation, compatibility fixtures, meaningful scenario coverage and all acceptance remain due. Preserve every historical result above; none grades this source.

## Task lease, report and wait source continuation

Source work continued without running a build, lint, formatter, unit or integration gate, following Indy's PostgreSQL-first/code-first direction.
This section is an implementation inventory, **not passing evidence**. The published head remains `ebfb3b722`; no new commit/push is claimed.

- `task/lease.rs`, ordinary issue/installed/delivery and schema 2570 now freeze task generation/stop generation, accepted/current setup revision and the actual bounded execution deadline. The initial binding clips both lease and slot to the same deadline. Task resources require the existing `durable_tasks_v1` capability; the runner still advertises false until its automatic coordinator is implemented.
- Discovery and claim both retain unsettled interpreted work subject to native task eligibility. Supplemental sources settle only after all-item response receipts; unanswered sources recover their accepted meanings and recorded responses rather than classifying again. The reviewer withdrew the suspected duplicate-settlement finding after inspecting that path.
- The ledger returns newly inserted coding charges. Original-lease request accounting now survives denied renewal, stopped task authority and fenced reports; it does not extend execution. Current task runtime uses a lease-owned cursor. Final verification must cover late receipts, repeated/out-of-order request sequences and final expired runtime.
- `task/report/` joins task state to the ordinary report transaction before slot release. Native preparations require recorded accounted goal answers, reviewed thresholds, current task/input/effect stamps and independent GitHub status. Continue/wait requires a complete compatible current checkpoint. Current profile activation, live repository authority and grants are checked again at commit. Expected continuation admission refusals use a savepoint and become Blocked without discarding known accounting. A native notice replaces unverified completion text in event history, session state, completion frames and outbound delivery.
- `task/waits/` and slots 2540–2560 now have native source for arming, claimed reads, signal matching, one ordinary continuation, cooldown, expiration and short-lived inbox collection. Four native dependency readers run without an agent, restore or decision call. A dependency observation reads the PR and latest workflow runs through the existing read-only credential broker; the fallback allowance counts these logical observations, each bounded to two status requests plus the existing scoped mint. Backoff is 60/120/240/300 seconds, with the task's cumulative limit retained across waits. Actual transport/latency proof remains due.
- App signature verification and installation/workspace mapping precede completion recording. Fleet hooks stay restricted to their verified fleet. Completion matching precedes ordinary investigation filtering and does not broaden that fanout. Missing manual-hook installation identity is not fabricated; independent reads remain the backup.
- Completion matching accepts a pending placeholder, a newer workflow run number, or the same run with a non-older attempt. Independent latest-status reads decide actionability. Unchanged/pending progress keeps the wait; closed PRs or external head changes block without overwriting human commits.
- Adversarial review caught conflated due/claim deadlines and millisecond signal-cursor loss. Source now separates `claim_expires_at` and uses monotonic `signal_sequence` / `observed_signal_sequence`; a racing signal cannot steal an active read or disappear at the same timestamp. Source scenarios and real contention proof remain due.
- Slot 2580 adds bounded transition reasons plus partial native-target containment and spent-attempt indexes. It preserves the target JSON as one source. Proposed generated projections were removed before verification; no schema vocabulary/role gate was changed. The inbox only retains completions relevant to unfinished exact native targets. Query plans, write amplification, fanout bounds and realistic cardinality measurements remain unverified.
- Report preparations and wait/read/completion operations now have bounded tracing/metric labels; the closed task-operation census is 25 × 11 outcomes (275 combinations). Runtime instrumentation/coverage proof remains due.

Remaining source work includes automatic intent/goal execution in the existing Root/lease loop, task-aware supervisor capture/verified restore and file safety, production input/Slack authority gaps, browser/CLI activity and all public documentation/acceptance coverage. Constructor/golden/DTO propagation, mechanical source conformance and meaningful adversarial scenarios are unfinished. Do not run the broad gates until PostgreSQL and the implementation batch are complete. Preserve every historical result above; none grades these new files.

## Generation ownership and bounded completion delivery

Source corrections only; no build, formatter, lint, unit, integration or acceptance run accompanies this section.
Exact-head GitHub reads still report `ebfb3b722`: governance, gitleaks, lint, dry-smoke, dry and test succeed; Rust unit and integration are red. Both existing inline Greptile threads remain resolved. These results do not cover the local implementation.

- Slot 2590 freezes each bound admission's task generation. Discovery, claim and issue require that generation; a retained marker prevents task deletion from turning bound work into ordinary work. No historical generation is guessed. Historical-row migration/recovery scenarios remain due.
- Native transitions retire never-issued obsolete task continuations. Fleet and deployment backlog queries also exclude bound obsolete native continuations, including previously issued continuations whose original accounting rows remain needed. Human sources continue to consume capacity until their separately owed replies settle. Usage and report recovery still use original lease/event ownership; they do not inherit a newer task's execution authority.
- Setup blocking now re-reads setup compatibility under the existing parent/slot locks. A stale refusal cannot by itself block a restored compatible setup. The read-only reviewer found this correction consistent with actual setup writers' parent locks.
- Effect allocation retains the complete canonical private request whose exact serialized bytes form its immutable digest; lease fences and retry identities do not become a different effect meaning.
- Slot 2600 is a durable projection cursor over immutable signed completion receipts. HTTP ingress only inserts the receipt/cursor. Maintenance examines at most 64 scoped waits per pass, one wait lock per transaction, advancing over irrelevant workflows as well as matches. Matching remains fleet-scoped when the signed hook is fleet-scoped. Its active-wait index includes the full equality prefix and `id` keyset order. The first page uses UUID's lower bound; later pages use the saved native wait identity.
- Completion delivery keeps the progress lock until the one wait update and cursor commit. A busy wait cannot be skipped; a failed cursor is deferred so later evidence can advance. Unfinished completion projections are retained beyond ordinary inbox collection. Wait consumption remains the only path that appends execution to existing admissions.

The 78-row role inventory and explicit slot registration include completion fanout. SQL execution, realistic `EXPLAIN (ANALYZE, BUFFERS)`, statistics, query latency, grants and concurrency scenarios remain unverified. No performance result is claimed.

The source-only reviewer reports no remaining actionable findings in these generation/backlog/setup/fanout corrections. Retaining issued admissions is intentional: ordinary final metering requires their unsettled marker, whereas queue capacity no longer counts an obsolete native continuation. This does not constitute executable financial or concurrency proof.

## Typed supervisor task integration source

The existing `ControlPlane` / `RunnerApi` transport now has typed task methods for the 17 approved runner method/path pairs: inputs, bindings, input receipts, decision reservation/receipt, effect publication/read/list and the nine checkpoint operations. The closed `Verb` list carries 41 entries with explicit methods and diagnostic names. Calls do not add a dispatch retry loop. Native paths use checked UUIDv7 identities and encoded cursor queries; dispatch replies retain the existing wiping `Body` owner.

`LeaseVerbs` now exposes native input/binding/receipt/decision/effect behavior; `FencedVerbs` supplies its own lease fence to implemented calls. `DecisionEvaluation` exposes only accounted evidence, never physical-call credentials. Physical decision execution/receipt recovery remains to be connected, so its default refuses rather than claiming a result. The runner capability remains false. Native input/binding responses can move their owned recovery context out of the private transport buffer without retaining it.

Checkpoint request bodies now support the same serialization their API input types already deserialize. Supervisor capture/restore is still absent: adding typed calls does not implement quiescent file capture, hostile restore checks or a functioning automatic root coordinator. The read-only standards reviewer found no actionable findings in this integration: methods/paths match the served resources, the supervisor supplies fences, ownership helpers preserve fields and the existing transport/error mapping is reused. This source has not been compiled or tested; runtime, coverage and final verification remain pending. No gate was run.
