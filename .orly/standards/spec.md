# Specs

This standard covers how a milestone spec is named, filled, grounded and proven, and how its rubric is graded, deferred and transferred. An authoring agent reads it while filling [the spec template](spec-template.md); an implementing agent reads it before marking work done. Here `review` means the EGM (Elephant-Goldfish Model) review at authoring and the standards reviewer at ship, because orly's gate checks no spec structure.

## Files and fields

### SPEC-TODO — Every non-trivial intent becomes a spec, and no `TODO.md` or loose task list stands in for one.
- **Applies to:** work that touches more than one file, a schema, a public interface, a security boundary or a new dependency
- **Checked by:** review
- **Bad:** a `TODO.md` with ten bullets on the branch
- **Good:** `docs/v2/pending/M52_001_P2_API_BUN_VENDOR_UTILITIES.md`, filled from the template
- **Why:** a task list carries no tests, rubric or deferral record.

### SPEC-NAME — A spec file is `docs/v<P>/<stage>/M<N>_<WS>_P<priority>_<CATEGORIES>_<NAME>.md`, and its NAME states the outcome in at most six words.
- **Applies to:** spec files under `docs/v*/`
- **Checked by:** review
- **Bad:** `docs/v2/pending/bump-bun-deps.md`; `M52_001_P2_API_BUMP_BUN_DEPS.md`
- **Good:** `docs/v2/pending/M52_001_P2_API_BUN_VENDOR_UTILITIES.md`
- **Why:** sortable, outcome-named files let a reader find the spec behind a behaviour.

`<P>` is the Prototype's major version and `<N>` the Milestone number; they are independent.
Categories are alphabetised codes: API, CLI, DOCS, GOV, INFRA, OBS, SKILL and UI.
Legacy-form files already under `docs/v1/` or `docs/v2/done/` keep their names.

### SPEC-NUM — Milestone and schema slot numbers come from a freshly fetched `origin/main`.
- **Applies to:** choosing `M<N>_<WS>` for a new spec, and the next migration or schema slot
- **Checked by:** review
- **Bad:** `ls docs/v*` in a worktree last fetched a week ago
- **Good:** `git fetch origin`, then `git ls-tree -r --name-only origin/main docs/`
- **Why:** a local listing misses numbers that other branches already merged, so two specs or migrations collide.

A number already used under `done/` is never reused.

### SPEC-STATUS — `**Status:**` is PENDING, IN_PROGRESS, DONE or DEFERRED, and the spec's directory agrees with it.
- **Applies to:** the Status header and the spec's location
- **Checked by:** review
- **Bad:** `**Status:** DONE` on a spec in `active/`; `**Status:** WIP`
- **Good:** PENDING in `pending/`, IN_PROGRESS in `active/` from implementation start, DONE in `done/`
- **Why:** readers and tools find work by directory, so a mismatch hides it.

A DEFERRED spec moves to `done/` as a record, and Discovery holds the reason and the conditions for reopening it.

### SPEC-BASELINE — Branch and the baseline headers stay pending at authoring, and their stages fill in the full comparison commit and its measured counts.
- **Applies to:** the Branch, Baseline revision, Test Baseline and Baseline evidence headers
- **Checked by:** review
- **Bad:** `**Baseline revision:** 289d226`; lane counts typed at authoring before any run
- **Good:** the full 40- or 64-character commit at implementation start, and `**Test Baseline:** unit=812 integration=144` measured before the Pull Request
- **Why:** a test delta means nothing without the exact revision and counts it compares against.

`**Baseline evidence:**` is an existing repository path or an `https://` URL. A repository without a declared lane writes `n/a — <reason>`.

### SPEC-SIZE — A spec holds 3 to 9 Sections in at most 500 lines, and a milestone holds at most four Workstreams.
- **Applies to:** spec scope and milestone decomposition
- **Checked by:** review
- **Bad:** a 700-line spec; a milestone with six Workstreams
- **Good:** two specs split at a Section boundary, each with its own rubric
- **Why:** a spec that passes 500 lines before its rubric holds two specs.

A fifth Workstream is allowed only for a cross-cutting concern.

### SPEC-TERMS — A spec names its units Prototype, Milestone, Workstream, Section, Dimension and Batch.
- **Applies to:** every file a spec or its Pull Request writes
- **Checked by:** review
- **Bad:** `Sprint 3`, `Task 2.1`, `Story`, `AC-4`, `Wave 2`
- **Good:** `M40_001 §3`, `Dimension 3.4`, `Batch B2`
- **Why:** one vocabulary maps every reference to exactly one unit.

Batches run in order: B2 starts after every B1 Workstream passes its gates. Conversation replies are exempt.

### SPEC-ESTIMATE — A spec carries no time or effort estimates, complexity ratings, percent complete, assigned owners or implementation dates.
- **Applies to:** every spec section and table
- **Checked by:** review
- **Bad:** `| Task | Effort | Owner |` with `2 days` and a name; `60% done`
- **Good:** `**Priority:** P1, operators see stale runs`; `**Depends on:** M40_001, the schema lands first`
- **Why:** estimates and owners go stale on the first day and crowd out the signals that stay true.

Priority is the only sizing signal and Depends on the only sequencing signal.
P0 blocks, P1 faces customers or operators, P2 is tooling and P3 can wait.

### SPEC-FILL — A filled spec keeps no `{{fill:` slot and no `tpl:` comment.
- **Applies to:** a spec leaving `pending/`
- **Checked by:** review
- **Bad:** `**Goal (testable):** {{fill:one sentence}}` left in place
- **Good:** every slot replaced with instance content, and the authoring-rules banner as the only comment left
- **Why:** leftover guidance reads as instruction to the implementing agent.

Runtime parameters the spec defines, such as `/projects/{project_id}`, are not slots.

## Intent

### SPEC-GOAL — The goal is one testable sentence that could serve as a test name.
- **Applies to:** the Overview's Goal line
- **Checked by:** review
- **Bad:** "Implement streaming."
- **Good:** "The Server-Sent Events handler streams project events as `text/event-stream`, p95 under 200 ms."
- **Why:** a goal that cannot be a test name means the intent is not yet understood.

### SPEC-HANDSHAKE — The authoring handshake restates the request and lists its assumptions, and implementation waits until the implementer's restatement matches the Intent.
- **Applies to:** the Intent and handshake section, and the Session Notes written at PLAN
- **Checked by:** review
- **Bad:** an Intent line copied from the request, with no assumptions listed
- **Good:** "Authoring handshake: … ASSUMPTIONS I'M MAKING: 1. …", then the implementer's own restatement in the Session Notes before the first edit
- **Why:** a divergence caught at PLAN costs one message, while one caught at review costs the branch.

### SPEC-CLARITY — Product Clarity's ten questions are answered in order before any implementation Section is written.
- **Applies to:** the Product Clarity section
- **Checked by:** review
- **Bad:** Product Clarity filled last, restating the Sections
- **Good:** internal work answers items 1 to 7, and items 8 to 10 read "N/A: no user surface" with the reason
- **Why:** the product behaviour decides the file list, not the reverse.

### SPEC-GOLDEN — The golden path names every lookup, data source and secret store, and an open `[?]` keeps the spec in `pending/`.
- **Applies to:** the design flow and Sections of a spec leaving `pending/`
- **Checked by:** review
- **Bad:** "the runner fetches the token [?] and calls the provider"
- **Good:** "the runner loads the provider key from the vault by key name, then calls the provider"
- **Why:** an unnamed source becomes a guess during implementation.

### SPEC-NOUN — A spec's key nouns mean what the repository already means by them.
- **Applies to:** terms such as "e2e", "acceptance" and "live", and any new target or recipe
- **Checked by:** review
- **Bad:** a new `e2e` umbrella target that comes out byte-identical to the integration target
- **Good:** search for the existing meaning first, then reuse the target or name the real difference
- **Why:** a duplicate target means the abstraction is wrong.

A lane the spec adds is proven in the CI environment that runs it; a local compose run proves nothing there.

### SPEC-FRAME — A pre-launch spec describes the target design as the design, without "production today" or pre-milestone framing.
- **Applies to:** spec prose and architecture notes while the product is pre-launch
- **Checked by:** review
- **Bad:** "pre-M80 operational truth" beside "the post-M80 design"
- **Good:** "This file describes the single-process model; the M80 split supersedes it."
- **Why:** with no production baseline, a current-versus-future split has nothing to anchor to.

### SPEC-BOUNDARY — Security-boundary and backend-heavy follow-ups get their own spec and Pull Request.
- **Applies to:** role-based access control, secret reveal, account or tenant deletion, auth webhooks, new endpoints, scheduled jobs and billing policy
- **Checked by:** review
- **Bad:** folding a tenant-deletion endpoint into a UI-polish Pull Request
- **Good:** a separate spec, so the change gets its own auth review and a focused diff
- **Why:** these changes need a review profile that a mixed diff dilutes.

Small pure-UI items may share one Pull Request.

### SPEC-AMEND — A spec that contradicts a rule is amended, and the rule is never weakened to fit it.
- **Applies to:** any conflict between a spec and a standard
- **Checked by:** review
- **Bad:** a Section that keeps an old flag as an alias, which NLG forbids
- **Good:** the Section amended to drop the alias
- **Why:** a spec is one instance, while rules are the constants every spec shares.

## Grounding and mechanism

### SPEC-RULES — Applicable Rules names the exact rule identifiers the diff will trip, from standards this repository has.
- **Applies to:** the Applicable Rules section
- **Checked by:** review
- **Bad:** "Follow the code rules."
- **Good:** "`tests.md`: TCF, TEST-PARTIAL; `languages/rust.md`: ERR-RS"
- **Why:** named identifiers give the implementer exact rules to check, while a generic pointer earns review findings.

### SPEC-READ — Each read-first item starts with an existing repository path or an `https://` URL, and files the spec creates belong in Files Changed.
- **Applies to:** the read-first list
- **Checked by:** review
- **Bad:** "1. `src/new_module.rs` — mirror this" for a file that does not exist yet
- **Good:** "1. `crates/api/tests/project_grants.rs` — the route test to mirror"
- **Why:** a pointer that does not resolve sends the implementer to guess.

A correctness-critical requirement is stated in the spec as well as cited, because a reader given only the spec cannot open the link.

### SPEC-PRIOR — Prior Art names a reference implementation to mirror, or says greenfield and points at the architecture page that defines the shape.
- **Applies to:** the Prior-Art section
- **Checked by:** review
- **Bad:** a new command designed with no reference and no reason
- **Good:** "Reference: the nearest existing handler and the HTTP standard; divergence: none"
- **Why:** mirroring a known-good shape avoids reinventing a solved pattern.

### SPEC-FILES — Files Changed lists every file the work creates, edits or deletes, by role, never by line number or function name.
- **Applies to:** the Files Changed table
- **Checked by:** review
- **Bad:** "`api.rs` lines 40–80"; a fixture the change edits but the table omits
- **Good:** "`tests/support/project.rs` | EDIT | seeds the new column"
- **Why:** the table scopes review and orphan sweeps, and line numbers drift.

The table includes the spec itself, its move to `done/`, the changelog entry and any architecture page. Edits outside it need the owner's approval.

### SPEC-BLAST — A teardown, rename or flip spec lists every word-boundary hit of its token across the repository, with production and test files apart.
- **Applies to:** specs that delete, rename or flip a name, flag or path
- **Checked by:** review
- **Bad:** `grep -rn '"old_name"' src/`, a quoted literal behind a path filter
- **Good:** `git grep -n -w old_name` from the root, with every hit in Files Changed
- **Why:** YAML and multiline strings carry no quotes, and a path filter misses same-directory references.

The spec's invariant and rubric searches use the same pattern as this discovery search.
Hits that share the spelling but not the meaning are listed apart.

### SPEC-GREP — A zero-match search gate names product phrases, never a common English word.
- **Applies to:** rubric rows and invariants that assert a pattern matches nothing
- **Checked by:** review
- **Bad:** `\b(run)\b` must match nothing
- **Good:** `run interrupt|spec init|gate loop` must match nothing
- **Why:** a gate that fires on ordinary English forces contorted prose.

### SPEC-MECH — A pinned mechanism names its state authority, guarded transition, commit-before-release boundary and failover assumption.
- **Applies to:** Sections and Invariants that promise safety, uniqueness or single use
- **Checked by:** review
- **Bad:** "Preserve single use."
- **Good:** "The `used_at` column is the authority; `UPDATE … WHERE used_at IS NULL` claims the token; the claim commits before the response."
- **Why:** an implementer can obey a vague sentence and still break the invariant.

A bounded prototype may leave a performance parameter open; an unchosen security mitigation is not ready to build.

### SPEC-PIN — A pinned detail records its source revision and the proofs that rerun when it changes, and a client-server claim is verified on both sides.
- **Applies to:** versions, identity scope, uniqueness, privilege boundaries, release order, failure behaviour and resource budgets
- **Checked by:** review
- **Bad:** "the client follows cluster redirects" with no version named
- **Good:** the client version from `Cargo.lock`, the datastore server version, and the failover test to rerun when either changes
- **Why:** an unpinned compatibility claim breaks silently on the next upgrade.

### SPEC-PROSE — A spec states mechanisms and proofs in prose and points at code to mirror, with no code listings, variable lists, line-by-line schema statements or test code.
- **Applies to:** Section bodies
- **Checked by:** review
- **Bad:** a 60-line function inside §2; a `CREATE TABLE` listing every column
- **Good:** "Pin identity, null and deletion semantics and grants; mirror the nearest migration; Dimension 2.1 proves it."
- **Why:** listings go stale and invite copying, while a mechanism and its proof stay true.

Interface examples, evidence commands and referenced diagnostics are allowed.

### SPEC-ORDER — Section prerequisites form an acyclic order, and release or migration order is pinned when correctness depends on it.
- **Applies to:** Section dependencies and the Depends on header
- **Checked by:** review
- **Bad:** §2 needs §4's table while §4 needs §2's API
- **Good:** "§1 migration, then §2 API, then §3 CLI; the migration deploys before the API"
- **Why:** a cycle has no first slice to build.

### SPEC-DECIDE — Agent defaults, measured outputs and human decisions are recorded apart, and human acceptance is a `manual` proof with its procedure and evidence.
- **Applies to:** Sections, the Test Specification and Discovery
- **Checked by:** review
- **Bad:** "Indy accepted the risk" with no quote; an agent marking a manual check passed
- **Good:** "Implementation default: a per-project lock, because …"; a `manual` row naming the person, the steps and where the evidence lands
- **Why:** an invented sign-off is scope the owner never accepted.

### SPEC-IFACE — The Interfaces section locks every surface other code depends on, and changing one amends the spec first.
- **Applies to:** HTTP endpoints, request and response shapes, and shared signatures
- **Checked by:** review
- **Bad:** renaming a response field during implementation with the spec unchanged
- **Good:** amend Interfaces, then change the code and its tests
- **Why:** callers and tests are written against the spec's shapes.

### SPEC-INVARIANT — Every invariant is enforced by code: the compiler, a linter, a compile-time assertion or a runtime check.
- **Applies to:** the Invariants section
- **Checked by:** review
- **Bad:** "Reviewers make sure no handler logs a token."
- **Good:** "The `Secret` type implements no `Display`, so logging it does not compile."
- **Why:** review discipline fails on the first busy day.

An operator procedure is a prerequisite, not a runtime guard.

## Proof

### SPEC-DIM — Every Dimension names one test, and that name equals the Test cell of its Test Specification row.
- **Applies to:** Dimension lines and the Test Specification table
- **Checked by:** review
- **Bad:** `- **Dimension 2.1** — handles errors` with no test named
- **Good:** `` - **Dimension 2.1** — expired tokens are refused → Test `should_reject_expired_token` `` with a matching row of concrete input and output
- **Why:** a Dimension without a test is not implemented.

### SPEC-TESTS — A Dimension is `DONE` only when the test it names exists in a tracked test source and passes.
- **Applies to:** marking a Dimension `DONE`
- **Checked by:** review
- **Bad:** marking 2.1 `DONE` after lint passes; a test name that appears only in the spec
- **Good:** `should_reject_expired_token` passes in the Section's lane, and `DONE —` lands in the same commit as the code
- **Why:** a name in the spec proves only that someone typed it.

Formatting and lint never mark a Dimension `DONE`. A row of tier `manual` carries durable evidence in place of a test source.

### SPEC-WIRED — A Dimension is `DONE` only when a production entry point calls the code it delivers.
- **Applies to:** marking a Dimension `DONE`
- **Checked by:** review
- **Bad:** a tested `refresh_quota()` that no handler calls
- **Good:** the route handler calls `refresh_quota()`, and a test reaches it through the route
- **Why:** tested code that nothing calls ships nothing.

### SPEC-FAIL — Every Failure Mode and error row has a negative test, and at least half the Test Specification rows test failure paths.
- **Applies to:** Failure Modes, error tables and the Test Specification
- **Checked by:** review
- **Bad:** a "dependency unavailable" Failure Mode with an empty Test cell
- **Good:** `should_503_when_cache_down` in both the Failure Mode row and the Test Specification
- **Why:** a failure path without a test is a promise nobody checks.

Failure Modes cover timeout, malformed input, auth failure, network blip, race, replay, exceeded quota and dependency unavailable where they apply.

### SPEC-E2E — A spec with a user-facing Category carries at least one end-to-end scenario through the real path.
- **Applies to:** specs whose Categories include CLI, UI or API
- **Checked by:** review
- **Bad:** unit tests alone for a new command
- **Good:** a subprocess test that runs the built command-line binary against a live daemon
- **Why:** each unit can pass while the wired path fails.

### SPEC-METRICS — Metrics & Observability declares each product or operator event with its test, or states that no signal changed.
- **Applies to:** the Metrics & Observability section
- **Checked by:** review
- **Bad:** an empty table; an event first found in review
- **Good:** `project_created` with its allowed properties, privacy guard and test; or "Not applicable: no product or operator signal changes."
- **Why:** a missing event is an implementation gap that ships unseen.

Event properties never carry raw email, passwords, tokens, one-time passwords or key material.

## Rubric, deferral and transfer

### SPEC-RUBRIC — The Acceptance Rubric holds 5 to 12 outcome rows, each with a copy-paste Verify command and a mechanically checkable Expected value.
- **Applies to:** the Acceptance Rubric
- **Checked by:** review
- **Bad:** one row per Dimension; Expected "works correctly"; a Verify command containing a pipe
- **Good:** `R2 | Expired tokens are refused (§2) | make test-unit-all | exit 0 | P0`
- **Why:** one scoring surface with checkable rows makes grading mechanical.

S rows carry every declared `conform` and `verify.*` command verbatim, plus `gitleaks detect --redact --no-banner`.
A pipeline is wrapped in a script, because a Markdown table escapes the pipe character.

### SPEC-GRADE — Each Graded cell holds ✅ or ❌ and one decisive output line from running its Verify command verbatim.
- **Applies to:** the Graded column at VERIFY and at close
- **Checked by:** review
- **Bad:** ✅ with no output; a pasted wall of log; an empty cell at close
- **Good:** `✅ test result: ok. 1412 passed; 0 failed`
- **Why:** missing evidence counts as ❌.

S rows point at the final `orly gate pr` run recorded in the Pull Request Session Notes.

### SPEC-SHIP — A ❌ row ships only as its priority allows: P0 never, P1 with an owner quote naming it, P2 and P3 with a Discovery disposition.
- **Applies to:** closing a spec and opening its Pull Request
- **Checked by:** review
- **Bad:** a P0 row at ❌ with "follow-up" written in Discovery
- **Good:** every P0 row ✅ or `MOVED`; a P2 ❌ with its cause and next step in Discovery
- **Why:** a failing P0 row is the spec's core promise broken.

A disposition that defers work is a deferral and needs the owner quote too.

### SPEC-MOVE — A `MOVED to M<N>_<NNN> R<n>` row names a successor that carries the criterion as its own P0 row, with the mapping in both specs and the owner's quote in Discovery.
- **Applies to:** transferring a rubric criterion to another spec
- **Checked by:** review
- **Bad:** `MOVED to M41_002` where M41_002 has no matching row
- **Good:** both specs map each moved Dimension, and Discovery quotes the owner authorising the transfer
- **Why:** a one-sided transfer is a deferral under a new name.

A `MOVED` row is never ✅: the criterion is still unmet, and the rubric names its new owner.

### SPEC-DEFER — A spec item is deferred only when Discovery quotes the owner naming it, and until then it stays in scope and blocks close.
- **Applies to:** Sections, Dimensions and rubric rows described as deferred
- **Checked by:** review
- **Bad:** "Dimension 2.3 deferred to a follow-up", written by the agent
- **Good:** `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<verbatim quote>" — context: <item, why>` under Discovery's deferrals bullet
- **Why:** an agent-written deferral is incomplete scope, not a decision.

PR-DEFER in the pull request standard holds the same rule for claims made in a Pull Request.

### SPEC-MODEL — A model review is advice, and Discovery records its model, version, finding counts and dispositions, never its raw output.
- **Applies to:** EGM reviews and any other model review of a spec or diff
- **Checked by:** review
- **Bad:** a pasted review transcript; "the model approved it" cited as proof
- **Good:** "EGM review, <model> <version>: 7 findings, 6 fixed, 1 declined with its reason"
- **Why:** a model score proves neither completion nor delivery.

Repository content goes to an outside model only with the owner's approval, with secrets and private data removed.
