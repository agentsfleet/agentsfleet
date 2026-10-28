# Tests

This standard covers test names, what a test must prove, how live-dependency tests stay real and isolated, and what a verification claim carries. A coding agent reads it before writing or changing a test; a reviewer reads it to judge a diff's tests. Proof rules for spec Dimensions live in [the spec standard](spec.md).

## Naming

### TST-NAM — Test filenames and test names carry no milestone, workstream, Section or Dimension identifier.
- **Applies to:** test files and test functions in every language
- **Checked by:** review
- **Bad:** `m24_001_cross_workspace_idor.rs`; `it("M28_001 3.3: Jira valid signature")`
- **Good:** `cross_workspace_idor.rs`; `it("should accept a valid Jira signature")`
- **Why:** a milestone closes within a cycle, while the test lives as long as the code it covers.

### TNM — A test name states the behaviour it verifies, and a test file is named for the subject it covers.
- **Applies to:** test functions and test files in every language
- **Checked by:** review
- **Bad:** `it("does not retry — wait, it retries once")`; `tests/metering_m18.rs`
- **Good:** `it("should retry once on a 502")`; `tests/metering_telemetry.rs`, named for the flow it proves
- **Why:** a failing test's name is the first thing an engineer reads, and a narrated or dated name sends them hunting.

Unit tests sit beside the code they cover; an integration test file names the flow, endpoint or invariant it proves.
Comments in tests follow NRC and CODE-MSID in the code standard.

## What a test proves

### TCF — A test fails when the clause, guard or branch it covers is deleted.
- **Applies to:** every new or changed test
- **Checked by:** review
- **Bad:** a tenant-scope test that asserts only absence and still passes with the scope clause deleted; a check asserting only an exit code
- **Good:** delete the clause, see the test fail, restore it; assert what the check found, such as the count, symbol or line
- **Why:** a vacuous pass and a real pass look identical from outside, so the suite stays green after the guarantee is gone.

A test that survives removal of its own subject is repaired or deleted, never kept for coverage.

### TEST-MUTATE — A red run mutates a value inside the function under test and fails at an assertion.
- **Applies to:** proving that a new or changed test can fail
- **Checked by:** review
- **Bad:** deleting the call to `scope_filter()` to see red, which leaves the helper unused and fails the build instead
- **Good:** flip `>=` to `>` inside `within_quota()`, or run `cargo mutants --in-diff <patch>`, and watch an assertion fail
- **Why:** a build failure shows that the code changed, not that the test checks the behaviour.

### TEST-REDGREEN — A fix's regression test fails on the parent commit and passes on the fix.
- **Applies to:** every change that fixes a bug, incident or regression
- **Checked by:** review
- **Bad:** a regression test written beside the fix and only ever run green
- **Good:** run it in `git worktree add ../pre HEAD~1` and see it fail, then pass on HEAD; record both results in the Session Notes
- **Why:** a test that passes on the buggy code does not pin the bug.

### TEST-OUTCOME — A test asserts the outcome: the return value and every side effect the code promises.
- **Applies to:** unit and integration tests
- **Checked by:** review
- **Bad:** `expect(sendMail).toHaveBeenCalled()` alone; an integration test asserting only the response body
- **Good:** assert the stored row, the emitted event's fields and the response together
- **Why:** a mock-call or body-only assertion passes while the write is lost.

Log assertions read structured fields from a capturing logger, never a string match.

### TEST-ERROR — A failure test asserts the error's type, code and structured fields.
- **Applies to:** tests of error paths in Rust and TypeScript
- **Checked by:** review
- **Bad:** `assert!(result.is_err())`; `expect(run).toThrow()`; `assert!(res.status().is_client_error())`
- **Good:** `assert_eq!(res.status(), 422)` plus the problem body's `code`; `expect(run).toThrow(LimitError)` plus its fields
- **Why:** a bare failure assertion also passes on the wrong failure.

### TEST-NEG — At least half of a changed surface's tests exercise failure paths, including empty, null, maximum, malformed and non-ASCII input.
- **Applies to:** the tests a diff adds for a changed function, handler or command
- **Checked by:** review
- **Bad:** three happy-path tests for a parser
- **Good:** one success case plus empty, oversized, truncated, `\r\n` and emoji input, each asserting its error
- **Why:** bugs live on the paths a happy case never walks.

### TEST-PUBLIC — A test drives the public surface and states expected values literally.
- **Applies to:** unit tests
- **Checked by:** review
- **Bad:** `assert_eq!(total(&items), items.iter().map(|i| i.price).sum())`; a helper made `pub(crate)` only for a test
- **Good:** `assert_eq!(total(&items), 4_200)` over a fixture whose prices are known
- **Why:** a test that re-implements the function agrees with its bugs, and one that reaches past the surface breaks on every refactor.

A private function that needs its own tests is public in practice, so it is promoted.

### TEST-REACH — An uncovered branch is first shown reachable from a fixture, then tested through the verb that composes it.
- **Applies to:** tests added to cover a changed branch
- **Checked by:** review
- **Bad:** a unit test calling the leaf `parse_suffix()` with input no caller can produce
- **Good:** build the fixture that reaches the branch through `project create`, then assert through that command
- **Why:** a branch no input reaches is dead code to delete, and a leaf test misses how the verb composes it.

### TVR — Test values are values the schema and types allow, unless the test asserts their rejection.
- **Applies to:** fixtures and inputs for code behind a schema or typed boundary
- **Checked by:** review
- **Bad:** inserting `0` into a column declared `CHECK (size_bytes >= 512)` and asserting success
- **Good:** 512 accepted and 511 rejected by the real constraint
- **Why:** an unreachable value passes in isolation and fails at integration.

A drift test compares against an independent statement of the schema; comparing code with itself catches nothing.

### TEST-TRUTH — When code and spec disagree, the test asserts the spec and the code is reported as the defect.
- **Applies to:** writing or changing an expected value
- **Checked by:** review
- **Bad:** editing the expected value until it matches what the code returns
- **Good:** keep the spec's value, let the test fail, then fix the code or raise the conflict
- **Why:** a test bent to the code proves only that the code does what it does.

Observed runtime behaviour, such as logs and incidents, outranks the spec; a conflict between them is raised before testing.

### TEST-DIFF — Every changed public function, new branch and new error path in a diff has a test or a recorded reason it has none.
- **Applies to:** every code-changing diff
- **Checked by:** review
- **Bad:** a new `match` arm with no test and no note
- **Good:** the arm's test, or `needs-infra: Linux sandbox only` recorded in the Session Notes
- **Why:** a bare gap is a defect, not a deferral.

### TEST-BRANCH — Coverage means branch coverage: each clause of a condition, each early return and each error path gets its own input.
- **Applies to:** tests for conditions, normalisers and guards
- **Checked by:** review
- **Bad:** one `"y"` input covering `s === "" || s === "y" || s === "yes"`
- **Good:** inputs for empty, `"y"`, `"yes"`, `"no"`, upper case and padded text
- **Why:** line coverage marks the whole condition covered after one input.

Coverage gates read the aggregate. Lines the compiler erases, such as `import type`, are never chased per file or hidden with ignore entries.

### TEST-GOLDEN — A snapshot or golden file changes only with a written reason, and never stands as a test's only assertion.
- **Applies to:** snapshot tests and golden output files
- **Checked by:** review
- **Bad:** rerunning with `-u` to accept a changed snapshot; a test whose only check is `toMatchSnapshot()`
- **Good:** the commit names the behaviour that changed the output, and a behavioural assertion sits beside the snapshot
- **Why:** a silently accepted snapshot records the bug as the expected output.

### TEST-DRY — Tests favour explicit setup, and a shared helper exists only when three or more tests share setup that hides no logic.
- **Applies to:** test helpers and shared setup
- **Checked by:** review
- **Bad:** a `build_world(flags)` helper whose branches decide what each test gets
- **Good:** each test builds the two rows it needs, and one helper seeds the project every test shares
- **Why:** logic inside a helper hides what an assertion depends on.

## Fixtures and data

### TFX — Test fixtures use the production constants, never copies of their values.
- **Applies to:** fixtures, seeds and expected values in tests
- **Checked by:** review
- **Bad:** `const DOMAIN = "agentmail.dev"` in a test while production reads `MAIL_DOMAIN`
- **Good:** `import { MAIL_DOMAIN } from "../config"` in the test
- **Why:** three copies of one value drift apart, and only one matches production.

### STL — A test stops writing a wire field in the same change that removes the field from the parser.
- **Applies to:** test request bodies and builders after a wire-protocol field is removed
- **Checked by:** review
- **Bad:** keeping `"stage_id": 1` in a test body because the handler now ignores it
- **Good:** remove the field from every test body in the same commit, and delete a test that existed only for that field
- **Why:** a test that sends an ignored field passes while exercising less than its name claims.

### ITF — An integration test seeds the real schema through shared fixture helpers and asserts against the real table.
- **Applies to:** integration tests that touch a production table
- **Checked by:** review
- **Bad:** `CREATE TEMP TABLE workspaces (...)` shadowing the real table
- **Good:** a fixture module named for its scope, such as `tests/support/workspace_credit.rs`, with `seed` and an idempotent `cleanup` around each test
- **Why:** a temporary-table mock keeps the old columns and hides the new `NOT NULL` column the real query hits.

Fixture constants and cleanup helpers live in test code, never in production modules. A probe table with no production counterpart is fine.

### TEST-GITENV — A test that runs Git in a scratch repository strips inherited `GIT_*` variables.
- **Applies to:** tests and test helpers that spawn `git`
- **Checked by:** review
- **Bad:** a bare `Command::new("git")` in a test that runs under a commit hook
- **Good:** `Command::new("git").env_remove("GIT_DIR").env_remove("GIT_INDEX_FILE").env_remove("GIT_WORK_TREE")`
- **Why:** Git exports variables such as `GIT_INDEX_FILE` to hooks, and a test that inherits them can act on the real repository.

## Live dependencies

### TEST-REAL — An integration test runs real Postgres, the real key-value store, and the real router with its middleware.
- **Applies to:** tests in the integration lane
- **Checked by:** review
- **Bad:** calling the handler function directly with a stub pool
- **Good:** send the request through the router to a migrated Postgres and the local key-value store
- **Why:** integration bugs live in the seams, and a stub removes the seam.

A test that mocks any of these, or an internal module, is a unit test and is labelled one.

### TEST-MOCK — Tests mock only system boundaries: network, disk, external services, clock and randomness.
- **Applies to:** unit and integration tests
- **Checked by:** review
- **Bad:** `mock.module("../services/billing", ...)`; a fake repository behind an integration test
- **Good:** a local HTTP server standing in for the payment provider; a fake clock
- **Why:** mocking an internal module tests the fake, and the integration bug ships.

External services are faked at their HTTP boundary, never at the SDK call site.

### TEST-LOCAL — Integration tests run against local containers, never a deployed environment.
- **Applies to:** the integration lane
- **Checked by:** review
- **Bad:** a test that calls `https://api.staging.example.com`
- **Good:** the Postgres and key-value store containers the integration lane starts and resets
- **Why:** a deployed target ties results to someone else's data and releases, and the test can damage them.

### TEST-SWITCH — A live-dependency test runs only in a documented lane that CI calls, and never passes silently without its dependency.
- **Applies to:** tests that need Postgres, a key-value store or another live service
- **Checked by:** review
- **Bad:** a test that returns early and passes when `DATABASE_URL` is unset
- **Good:** `#[ignore = "needs live Postgres: the integration lane"]`, run by that lane in CI
- **Why:** a silent skip reads as a pass, and nobody learns the lane exists.

### STR — A streaming test proves bytes arrive incrementally at the transport, not only that the parser handles a full buffer.
- **Applies to:** Server-Sent Events (SSE), chunked, WebSocket and other streaming paths
- **Checked by:** review
- **Bad:** feeding a pre-buffered SSE body to the parser
- **Good:** read the first event from the live response while the producer holds the rest, then release the next
- **Why:** a correct parser cannot show whether the HTTP layer buffers the whole response.

Reconnection with `Last-Event-ID`, a slow consumer and a client disconnect each get a test too.

## Failure injection

### TEST-INJECT — Every failure mode named in code or spec has a test that injects it deterministically.
- **Applies to:** error branches, retries and named failure modes
- **Checked by:** review
- **Bad:** "handles connection reset" with no injection mechanism
- **Good:** `pg_terminate_backend(pid)` from a sibling connection mid-transaction; a fake clock past the lease deadline
- **Why:** a failure the test cannot reproduce on demand is fiction.

A timeout and a connection reset are shown to take different paths.

### TEST-PARTIAL — An operation that touches several systems, or acquires one resource twice, has a failure test at each ordered interaction.
- **Applies to:** handlers and services with more than one dependency call
- **Checked by:** review
- **Bad:** draining the connection pool and calling it coverage of every acquisition
- **Good:** a failpoint that fails acquisition 2 of 2, selected by workflow and boundary-call ordinals and scoped to one operation
- **Why:** a drained pool proves only the first acquisition.

The failpoint resets per test, counts safely under concurrency and asserts it fired exactly once. Release, then an external call, then reacquire is the riskiest window.

### TEST-RESIDUAL — A failure-injection test asserts the residual state: durable rows, external effects, the user's next read and the retry outcome.
- **Applies to:** every injected failure
- **Checked by:** review
- **Bad:** asserting only the 503
- **Good:** assert the 503, no orphan row, no remote charge, and a retry that completes exactly once
- **Why:** the response alone hides a half-finished operation.

### TEST-ACKLOSS — Each external mutation has a test where the remote effect commits, its reply is lost, and a retry yields exactly one effect.
- **Applies to:** calls that change state in an external service
- **Checked by:** review
- **Bad:** treating a durable intent row as proof that a retry cannot reissue the remote call
- **Good:** a mock server that commits, then drops the response; the test reads remote state and asserts one effect after retry
- **Why:** a lost acknowledgement turns a naive retry into a second charge.

A mutation that can complete before any durable intent or repair record exists is a design defect to report. Read-only prerequisites are exempt.

### TEST-KILL — A process that holds a lease is tested by `SIGKILL` mid-lease: the lease is reclaimed, the work runs once, and the late report is refused.
- **Applies to:** client daemons such as a job runner, and any process that holds a lease
- **Checked by:** review
- **Bad:** only a graceful `SIGTERM` drain test
- **Good:** `kill -9` the runner mid-lease; assert reclaim with a higher fencing token, one processed event and a stale-token rejection
- **Why:** sudden death is the case a happy-path loop never reaches.

A restart re-registers without duplicate work. A forked child is reaped, and its sandbox removed, on every exit path.

## Isolation and determinism

### TEST-ORDER — Each test resets the state it touches and passes in any order.
- **Applies to:** every test suite
- **Checked by:** review
- **Bad:** test B reads the row test A created
- **Good:** unique identifiers or per-test rollback, proven with `bun test --randomize` or another shuffled run
- **Why:** an order-dependent pass hides coupling that fails once the order changes.

A suite-level reset makes the first run clean; it does not isolate tests from each other within one run.

### TEST-SCOPE — Concurrent live tests isolate on every scope the code reads or cancels: owner, tenant and project.
- **Applies to:** integration tests that run in parallel against shared datastores
- **Checked by:** review
- **Bad:** two tests sharing one tenant, where one test's cancel-all stops the other's runs
- **Good:** each test mints its own owner, tenant and project
- **Why:** a shared scope turns one test's cleanup into another test's failure.

### TEST-RACE — A flaky test is fixed at its race and measured alone and beside its siblings, never by a wider timeout or a clean rerun.
- **Applies to:** any test that fails intermittently
- **Checked by:** review
- **Bad:** raising a wait from 5 to 30 seconds; accepting a green rerun
- **Good:** mint the rows the test scans, wait for the full user-visible end state, then repeat it alone and with its siblings
- **Why:** a wider timeout or a lucky rerun hides the race until production meets it.

### TEST-PORT — A test harness binds its listener once and hands the bound socket to the server.
- **Applies to:** test servers and harnesses that listen on a port
- **Checked by:** review
- **Bad:** ask the OS for a free port, close it, then let the server bind that number
- **Good:** `TcpListener::bind("127.0.0.1:0")` passed straight to `axum::serve(listener, app)`
- **Why:** another test takes the port in the gap, and the server fails with `EADDRINUSE`.

A harness that must reserve a number retries on conflict and never panics on a bind race.

### TEST-TEARDOWN — Test reset derives what it drops from the live catalog or the migration set, never from a hand-kept list.
- **Applies to:** test database reset and teardown code
- **Checked by:** review
- **Bad:** a teardown that drops a fixed list of schemas
- **Good:** drop every schema `information_schema.schemata` lists outside the system set
- **Why:** a migration that adds a schema survives a fixed list, and the next run cannot start.

### TEST-COUNTER — Performance and complexity claims are proven with counters, and wall-clock time only as a median of runs against a pinned baseline.
- **Applies to:** performance budgets, N+1 query checks and complexity claims
- **Checked by:** review
- **Bad:** `assert!(elapsed < Duration::from_millis(200))` on one sample
- **Good:** count database round trips at n and 10n, and assert linear growth
- **Why:** a counter gives the same verdict on every machine, while one timing sample flakes.

A missed budget is a design finding, and the threshold never loosens silently.

### TEST-CONTEND — An exactly-once or idempotency claim is proven by at least 100 callers released together from a barrier.
- **Applies to:** idempotency keys, leases and concurrent writes to one row
- **Checked by:** review
- **Bad:** two sequential calls with the same key
- **Good:** 100 tasks on one `tokio::sync::Barrier` sharing a key, asserting one row and a peak in-flight count above one
- **Why:** sequential calls cannot reveal a lost update or a hidden global lock.

### TEST-LEAK — A suite that drives live dependencies ends with the open connections, file descriptors and memory it started with.
- **Applies to:** integration suites and long-running request loops
- **Checked by:** review
- **Bad:** no check, or a check after a single request
- **Good:** drive N requests and assert the pool's open count and the heap high-water mark stay flat
- **Why:** a slow leak shows only as growth across many requests.

## Verification evidence

### TEST-LANE — A package-scoped runner proves a package, and only the declared `verify.*` commands prove the repository.
- **Applies to:** any claim that a branch is verified or ready
- **Checked by:** gate: cmd.verify.unit, cmd.verify.integration
- **Bad:** `cargo test -p app_wire` passes, and the message says the tests pass
- **Good:** the commands `.orly/orly.json` declares, such as `verify.unit` and `verify.integration`
- **Why:** a package run skips every other crate and the TypeScript coverage gates.

`orly gate pr` runs every declared command itself and never infers success from a push.

### TEST-CLAIM — A claim that tests pass names the command it ran and the counts it printed.
- **Applies to:** done messages, Pull Request descriptions and Session Notes
- **Checked by:** review
- **Bad:** "Tests pass."
- **Good:** "`make test-unit`: 1,412 passed, 0 failed, 37 ignored."
- **Why:** a bare claim cannot be checked or compared with the baseline.

### TEST-SKIP — A lane that could not run is reported as not run with its reason, never folded into a pass.
- **Applies to:** verification reports
- **Checked by:** review
- **Bad:** "Tests pass" while Docker was down and the integration lane never started
- **Good:** "`verify.integration` not run: Docker unavailable; unit lane passed with the counts above."
- **Why:** a hidden skip reads as proof.

### TEST-EXIT — A verdict is the command's own logged exit status, never a pipeline's last stage, a trailing `echo`, a background notice or `| tail`.
- **Applies to:** agents and scripts that judge a command's result
- **Checked by:** review
- **Bad:** `make X 2>&1 | tail -2 && git commit`
- **Good:** `make X > log 2>&1; echo "EXIT=$?" >> log`
- **Why:** `tail` exits 0 whatever the command did, so the commit proceeds on a failure.

The zsh shell has no `PIPESTATUS` array, and `sh -c 'false | tail -1; echo $?'` prints 0.

### MKP — A Make recipe never pipes a command whose status matters into a filter unless the recipe shell sets `pipefail`.
- **Applies to:** `Makefile` and `make/*.mk` recipes
- **Checked by:** review
- **Bad:** `@cargo test 2>&1 | tail -3`
- **Good:** drop the pipe, or set `SHELL := bash` and `.SHELLFLAGS := -eu -o pipefail -c`
- **Why:** Make's `/bin/sh` reports the pipeline's last status, so the recipe passes while the test fails.

### TEST-ABSENT — An absence claim counts only when the same check finds a known positive and fails loudly on error.
- **Applies to:** empty searches, zero counts and "pre-existing" claims
- **Checked by:** review
- **Bad:** `.unwrap_or(0)` turning a failed count query into "zero rows"; an empty search over a mistyped path
- **Good:** `.expect("count tenant rows")`; the same search run first on a file known to match
- **Why:** a check that cannot find anything reports absence for free.

### TEST-DELTA — A diff that adds code adds tests: the unit count rises over the recorded baseline, or the Pull Request says why not.
- **Applies to:** code-adding branches before the Pull Request opens
- **Checked by:** review
- **Bad:** a new handler, an unchanged unit count and no note
- **Good:** baseline and branch counts from the same lanes in the Session Notes, with each removed test explained
- **Why:** a flat count on new code means the code shipped untested.
