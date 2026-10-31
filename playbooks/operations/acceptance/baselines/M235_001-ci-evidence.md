# M235 development source-read evidence

Implementation branch: `feat/m235-ci-evidence`.
Comparison revision: `7b18cd4166b87b9289c9d94cd31000f96d500e15`.
Full repository verification and implementation grades remain incomplete.

## Intent and scope

Fix generic guarded GET downloads first, then verify GitHub job-log text in a separate read-only development fleet.
Keep per-hop admission, credential boundaries, private-target handling and shared no-follow clients intact.
Later instruction, repair and evaluation Sections remain incomplete; this source probe cannot substitute for M224's demonstrations.

## Observed source failure

Authenticated native metadata identifies job `123968144417`, run `41027232532`, attempt `1` and commit `d029957192e53ce4a3a83a928c1ed78a23e75683`.
The completed job failed; its recorded interval is `2026-10-30T08:33:29Z` through `2026-10-30T08:35:32Z`.
Its log endpoint again returned HTTP `302`, with safe destination origin `https://productionresultssa6.blob.core.windows.net`.

No signed URL or credential was displayed or saved.
Native development doctor passes for workspace `01a154ce-2ad5-7ce5-9493-f3a9d95a0b0e`.
The separate fleet's own successful log read remains unproved.

## Separate development fleet: before the fix

Native CLI identity is `nkishore@megam.io`, authenticated by the environment's tenant API key against `https://api-dev.agentsfleet.net`.
`git fetch origin main` and `git rev-parse origin/main` returned `7148b1d4ff9a5ad7da3c4ba3c562ec539abef7a1`; `cat VERSION` returned `0.71.0`.

| Native setup record | Identifier |
|---|---|
| Displayed fleet | `github-log-verification` |
| Fleet | `01a18da1-28e7-7b46-a9bc-03b26a61c76a` |
| Draft | `01a18d9d-0abd-734f-8d85-9b6c533d7d5e` |
| Candidate | `01a18d9d-0abd-7336-8012-2c8c39240d2e` |
| Save operation | `01a18da1-27cb-7706-925e-6fc0ed3d2642` |
| GitHub read grant | `01a18da1-28e7-72c9-9b42-5564f888ef1b` |
| Approval | `01a18da1-28e7-7d4b-a5fc-1769998b21c2` |
| Review digest | `authoring:v1:sha256:7f9eb35335b0fd003fe83f7c203b4d6f5244f3e52cd5db9672ebe9b190ed1b5a` |

Indy replied, “Approved in development”. The native approval record was still pending when reread, so the agent posted that exact approval through `agentsfleet approvals approve`; it resolved at `2026-10-30T16:36:50.026Z`.

Resuming the original Save operation returned setup `READY`, all nine checks ready, and fleet `ACTIVE`.
The fleet has only `http_request`, GitHub read access to `agentsfleet/linkwarden`, read-only networking, no triggers, and exact reviewed API/download hosts.

The before-fix message produced event `01a18db0-c90e-7324-b7a4-b3fb31116542`, created `2026-10-30T16:42:15.706Z`.
Native event detail and addressed full-call reads report three calls, zero omitted calls, and no argument/output truncation.

| Fenced call | Actual source | Result |
|---|---|---|
| `1:1` | run `41027232532`, explicit attempt `1` | HTTP 200; matching attempt and commit |
| `1:2` | job `123968144417` | HTTP 200; matching run/attempt/commit; failed step “Check link title decoding”, `08:35:24Z`–`08:35:26Z` |
| `1:3` | that job's `/logs` | `upstream_status`; HTTP 302; safe storage origin; empty body |

The actual answer begins “Result: log content NOT retrieved — stopping at the gap”.
It also states “Log body access was not verified.”
This is a fleet-owned reproduction of the missing source, not a passing log-read demonstration.
The event's `processed` status records settlement only.

## Local implementation checks

On the unchanged comparison revision, `make test-unit-rustd-runner` failed with 54 executor-suite tests passed and two failed: `processes::test_executor_kill_reaps_process_group` and `orphans::a_backgrounded_job_in_the_group_ends_with_its_command`.
Earlier suites reported 142 agent, 76 egress and 107 executor-library tests passed.
This cloud container runs `tail` as process 1 and retains orphan zombies; repository process tests were not changed or suppressed.

After implementation and the first review fixes, the inner-loop command was:

```sh
cd rustd
CARGO_TARGET_DIR=/workspace/agentsfleet-m224/rustd/target \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 \
cargo test --all-features -p afr_egress -p afr_tools -p afd_fleet_runtime
```

Result: **477 passed, zero failed**: egress 85, tools 256, runtime library 68 and runtime integration-target unit tests 68.
These are changed-package results, not a full repository or manual acceptance pass.
An earlier negative privacy test caught a once-encoded URL hex-case leak; the corrected masker passes that regression.

The initial 477-pass selection used an injected retry classifier and observed one tool request versus two control requests after a connection failure.
Checking the complete Cargo feature graph then showed HTTP/2 enabled transitively by `gix-transport`, while the earlier standalone egress selection lacked it.

The final regression uses a real HTTP/2 server returning `REFUSED_STREAM` after observing headers.
It expects one tool GET/POST request versus the shared guard’s original request and two default retries.

Dev-only dependencies make that protocol test explicit in standalone selections, using already-locked `h2` 0.4.20 and `reqwest` 0.13.5.
That protocol test changed no production protocol or inference retry policy. Later dependency updates are recorded separately.

The final HTTP/2 regression passed in the repository runner unit selection, observing the required one-versus-three request counts for GET and POST.

The composing privacy tests return protected target echoes in HTTP 200, 403 and 500 bodies and content types.
They assert sanitized model output and paired structured events.

The scoped hosted integration passed after the local proxy correction below.
It checked real TLS downloads and native full-record reads for HTTP 200 and 403 responses.
This scripted-model result proves wiring, not actual-model development acceptance.

Full integration setup required the session proxy's trusted certificate inside the disposable QStash container, supplied through an external temporary Compose override.
Initial integration attempts found a new Rust submodule path error and a missing test-client timeout argument; both were corrected before rerunning the lane.
No tracked workflow, test harness or deployment configuration was altered for either recovery.

Later repository lanes run under task-local `tini -s`, version `0.19.0`, to reap orphan processes normally; its official release checksum was verified before use.
The named make commands, assertions and source are unchanged.

The first full integration attempt stopped with 1,343 passes in completed suites.
The daemon suite reported 53 failed test lines; the attempt has no passing final verdict.

Its first failure, `integration_cluster_outcomes::test_cluster_preserves_application_outcomes`, was isolated with fresh schemas and make's exported environment.
The expected heartbeat HTTP 200 was HTTP 403 because the test addresses the daemon's `[::]` listener through the inherited proxy.

An owned local IPv6 listener reproduced HTTP 403 with inherited settings and HTTP 200 with its local address added to `NO_PROXY`.
Command-local `NO_PROXY` and `no_proxy` additions `[::],::,0.0.0.0` preserve the inherited external proxy and credential settings.
After a clean reset/migration, the isolated cluster test passed one test and the hosted redirect test passed one test, with zero failures in either selection.

The first `make test-unit-all` run under the reaper passed the executor suite's 56 tests and the new HTTP/2 retry regression.
It then failed at the unchanged toolbox image mode assertion: actual `0400`, expected `0444`, under the cloud process's `umask 0077`.

The subsequent full unit command uses process-local `umask 022`.
Evidence logs retain explicit mode `0600`; no test assertion or repository source changed for this adjustment.
Full repository unit, integration and lint verdicts remain pending.

## Review and documentation

The first standards review found five material issues:

- Mixed-case GET selection.
- A URI-component signature spelling.
- Absent wire retry coverage.
- Vacuous composing privacy checks.
- Missing safe stop context.

Confirmation review found all five addressed with zero new material findings.
The final protocol-test review also found zero material findings after the expected lockfile dependency edge was refreshed.

This review does not grade later instruction, repair or eight-case evaluation Sections.

Companion documentation is committed at `81241c5ef0d303b7c2636dba19090266f3c68224` on `/workspace/docs`, branch `chore/m235-ci-downloads-changelog`, with tool limits and an unpublished changelog entry marked coming soon.
`bun run orly gate verify` passed there, including `make lint`, build validation and broken-link checks.

## Remaining live proof

A fresh development check returned HTTP 200 from `/healthz` and `/readyz`, reporting commit `7148b1d`, version `0.71.0`, and database/queue ready.
The native runner list showed runner `01a154d8-ae2c-7ee2-89a1-9c19f0f60ba2` active with `last_seen_at: 2026-10-30T17:24:57.892Z`.
GitHub still lists successful development deployment `41006375734` at the matching full main revision.

Optional native download-size preparation reached the job-log redirect but the local session proxy refused the reviewed storage-origin tunnel with HTTP 403.
No download size or local source-read pass is asserted from that check; the development fleet uses its own runner network.

Review and approve the exact implementation revision for the existing development deployment workflow.
Then correlate the deployed API/runner revision and health before triggering the same separate fleet again.
Require its own final HTTP 200 log result, reported redirect count, usable actual failure excerpt and native full tool records.

Do not substitute native log retrieval, scripted-model wiring, a queued event or this before-fix stopped answer for that proof.
M224's three development demonstrations and production gates remain incomplete.


### Recovery checkpoint

The cloud terminal service disconnected during verification and subsequently reported the environment unavailable.
Worktree files survived.

On recovery the provider reported `running`, `connected`, no failure reason and no retained diagnostic records.
Kernel logs are inaccessible, so the cause remains unknown.

Indy explicitly requested a commit and feature-branch push to preserve the work. This is an incomplete checkpoint, not acceptance or merge approval.

The redirect fix, separate-fleet before-fix evidence, nine stable dependency updates and HTTP egress API settings are present. The Linux acceptance adapter remains planned.

The focused post-update run passed the runtime, gate, wire and HTTP admission packages. It exposed 20 supervisor checkout failures after `gix` 0.90 changed bare-clone branch placement.

An explicit remote refspec restored 19 tests; the detached-default-branch refusal test still fails because upstream now guesses a default branch. Full unit verification stopped at that remaining failure.

Lint, full integration, coverage, live positive fleet proof and PR checks remain incomplete.


### Linux adapter and dependency follow-up

The three Rust make unit shards passed: runner 1,462; daemon 139; daemon libraries 3,338. Total: 4,939 passed, zero failures.
The checkout refusal now passes after preserving the remote’s advertised detached HEAD instead of accepting the library’s inferred branch.

The full unit command then encountered 29 frontend analytics failures. The cloud environment explicitly disables analytics, while these existing tests set a fixture key and expect the unset switch’s default.

A focused rerun with `NEXT_PUBLIC_POSTHOG_ENABLED=` passed all 41 analytics tests. The full command was stopped after the failure; it has no passing final verdict.

The Docker runner image built successfully. Five focused Linux adapter tests passed, covering signature/version refusal, certificate ownership/cleanup and Compose overlay argument handling.

The local development launcher suite passed 26 tests. The browser and CLI acceptance suites have not yet passed through the new adapter.

The existing development toolbox was recovered through the GitHub connector from workflow `41006375734`, record `12955698432`. Its signature verified against the runner’s compiled release key and its manifest admits version `0.71.0`.
No release key was loaded or used to sign new material.

OpenAPI was regenerated with the built native command `agentsfleetd --no-banner openapi`. It publishes `network.http_egress` and `package_manager_domains` in definitions and leases.


The next frontend coverage attempt reported 451 passing files, four failing files and one failed assertion. All four failures referenced missing machine environment files.

The configured UI, daemon and runner file paths are absent even after home-directory expansion. Existing credential bindings remain available in the process.

Private task-local file paths now preserve those bindings without copying their values. A fresh local session-code pepper is stored only in the private daemon settings file.

The four affected frontend test files then passed all 25 tests using those task-local paths. Full coverage verification remains pending.
