<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M251_001: A tenant runner connects from the internet, rotates its token without a restart, and runs platform-model fleets through a daemon proxy

**Prototype:** v2.0.0

**Milestone:** M251

**Workstream:** 001

**Date:** Nov 17, 2026

**Status:** PENDING

**Priority:** P1, M246 lets a tenant enrol a runner, but that runner cannot reach the private stream listener from outside Fly, cannot rotate without a restart, and cannot run a fleet on the platform default model

**Categories:** API, CLI, DOCS, INFRA, OBS, UI

**Batch:** B1, follows M246_001; ships as its own Pull Request because it moves a security boundary (SPEC-BOUNDARY)

**Depends on:** M246_001, which adds the runner's `tenant_id`, the `runner:tenant` routes, the ownership filter and the platform-model refusal this spec relaxes; M243_001 (done), whose stream re-checks a connection on an interval; for §2, the Linux runner kernel lane (`make test-runner-kernel`)

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 17, 2026), moved out of the Nov 06 M246_001 draft at Indy's request; §2 is the former pending M240_001 audit, written at Indy's explicit audit request (Nov 04, 2026, baseline `1920ff345`)

**Canonical architecture:** `docs/architecture/runner_protocol.md` §"Certificates and configuration" and §"Connection direction and deployment" own the stream route §1 changes; `docs/AUTH.md` §"Runner token (`agt_r`) — the machine principal" owns the trust model

---

## Overview

**Goal (testable):** A tenant runner outside Fly reaches the stream with only a URL, a token and the published trust, keeps working through a token rotation with no restart, and runs a platform-model fleet without ever holding the platform key.

**Problem:** M246_001 ships the ownership boundary and leaves four gaps:
- The stream listener is private to Fly. Prod has "No public Fly service" (`deploy/fly/agentsfleetd-prod/fly.toml`), and the listener's certificate names only `agentsfleetd-prod.internal` (`docs/architecture/runner_protocol.md`). A customer's host has no route in, and a WireGuard tunnel would open the whole private Fly network to it.
- Rotation overwrites `token_hash`, and the old token dies on the next read (`rotate_runner_token.sql`). The runner reads its token once at boot (`afr_supervisor/src/config/mod.rs`), so every rotation is a restart.
- A tenant runner refuses platform-model fleets (M246_001 §4), so a tenant on the platform default model cannot use its own runner. A tenant also cannot keep its work off platform runners.
- Nobody has measured whether a tenant shell can stop or signal its own trusted executor. On a tenant-owned host, that executor is the boundary between the tenant and the platform.

**Solution summary:** §1 opens a public route to the stream listener on a public name, with trust the runner already supports. §2 measures the executor's signal boundary on a real Linux kernel; any runtime change waits for Indy's mechanism review. §3 keeps the old token digest for an overlap window, and the runner reloads its token from a file on SIGHUP or a 401. §4 adds a tenant placement setting with three choices. §5 gives a tenant runner on a platform-model fleet a proxied model-call capability; the daemon calls the provider and meters the tokens. §6 adds Settings → Runners and the admin badge, and §7 the docs.

**Design flow:**

```
host: AGENTSFLEET_RUNNER_STREAM_URL=https://<public name>:4443 (+ CA file if not a public CA)
   ─► TLS 1.3 to the public listener ─► POST /v1/runners/me/stream, Bearer agt_r ─► M243 session
admission for fleet F of tenant T, offered to runner R:
   claim ─ R.tenant = T, or R.tenant NULL and T's placement admits platform runners
   gate ─ R tenant-owned and posture platform → model_access proxied {token, path}; else direct
proxied turn: runner ─► POST /v1/runners/me/leases/{lease_id}/model-calls
   ─► daemon opens the platform key, runs af_inference::stream ─► chunk frames back
   ─► usage line logged by the daemon under the lease row lock, then the end frame
rotate: current ← new digest, previous ← old digest until now + RUNNER_TOKEN_OVERLAP
   host: new token into the file, then SIGHUP or the next 401 ─► reload ─► new stream supersedes old
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): public stream route, overlap rotation and proxied models for tenant runners
- **Intent (one sentence):** let a customer's runner join from anywhere, rotate without downtime and run every fleet, while the platform key stays in the daemon.
- **Authoring handshake:** Indy moved these Sections out of M246_001 to ship "tenant runs a runner" first. He left the certificate authority choice open: "it depends on the CA authority the agentsfleet have." ASSUMPTIONS I'M MAKING:
  1. M246_001 has merged: runners carry `tenant_id`, and a tenant runner refuses platform-model fleets.
  2. A tenant with no placement row reads `tenant_or_platform`, M246_001's fixed rule.
  3. Runner and daemon ship together, so `ExecutionPolicy` changes shape with no compatibility field.
  4. The open blockers in Discovery keep this spec in `pending/` until Indy answers.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE). CHORE(open) re-verifies Files Changed against `origin/main`; schema and error-code numbers are taken fresh (SPEC-NUM).

## Implementing agent — read these first

1. `docs/architecture/runner_protocol.md` and `playbooks/operations/runner_stream_tls/001_playbook.md`: the listener, its certificate delivery and the private route §1 widens.
2. `rustd/crates/runner/afr_supervisor/src/config/stream.rs` and `rustd/crates/protocol/af_stream/src/tls/boring.rs`: the runner's trust, OS roots plus an optional CA file.
3. `rustd/crates/runner/afr_sandbox/src/bubblewrap/mod.rs`, `afr_sandbox/src/harden/` and `rustd/crates/runner/afr_executor/src/server/launch/`: for §2, the PID namespace, sandbox identities, seccomp and Landlock rules, process groups and cgroup placement.
4. `make test-runner-kernel` in `make/test-unit.mk` and `rustd/crates/runner/afr_sandbox/examples/kernel_lane/`: for §2, the lane's privileges and cleanup ownership.
5. `rustd/crates/daemon/afd_runner/src/rotate.rs` and `afd_state/src/credentials/`: rotation and the token lookup §3 changes.
6. `rustd/crates/daemon/afd_authoring/src/model/capability.rs` and `af_wire/src/authoring/lease.rs` (`ModelAccess`): the proxied capability §5 mirrors.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M251_001_P1_API_CLI_DOCS_INFRA_OBS_UI_TENANT_RUNNERS_CONNECT_ROTATE_PROXY.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `deploy/fly/agentsfleetd-dev/fly.toml`, `deploy/fly/agentsfleetd-prod/fly.toml` | EDIT, with Indy's approval | §1: a public service for the stream port |
| `playbooks/operations/runner_stream_tls/001_playbook.md` | EDIT | §1: the certificate covers the public name |
| `schema/<UTC>_runner_runners_token_overlap.sql`, `<UTC>_runner_tenant_placements.sql`, `<UTC>_runner_leases_model_token.sql` | CREATE | §3, §4, §5; or table-file edits before the first production deploy |
| `rustd/crates/daemon/afd_db/src/migration.rs` and its position assertions | EDIT | register the slots (MIG) |
| `rustd/crates/runner/afr_sandbox/examples/kernel_lane/` and its fixtures | CREATE, EDIT | §2: the identity record, the stop probe and the signal-route inventory |
| `rustd/crates/runner/afr_sandbox/src/bubblewrap/mod.rs`, `afr_sandbox/src/harden/`, `afr_executor/src/server/launch/` | EDIT, only after Indy's mechanism review | §2: whichever isolation the probe shows is needed |
| `rustd/crates/daemon/afd_state/src/sql/select_runner_token.sql`, `credentials/mod.rs`, `credentials/rows.rs`, `credentials/tests.rs` | EDIT | §3: the previous digest authenticates inside its window |
| `rustd/crates/daemon/afd_runner/src/rotate.rs`, `sql/runner/rotate_runner_token.sql` | EDIT | §3: overlap rotation |
| `rustd/crates/daemon/afd_gateway/src/session/context.rs` and the connection re-check it calls | EDIT | §3: re-check by the digest that opened the connection |
| `rustd/crates/runner/afr_supervisor/src/config/mod.rs`, `config/tests.rs`, `client/`, `halt/mod.rs`, `rustd/crates/runner/main/src/main.rs`, `af_core/src/signal.rs`, `agentsfleet_runner/tests/fake_daemon.rs`, `tests/fixtures/runner/help.txt` | EDIT | §3: token file, SIGHUP and 401 reload |
| `rustd/crates/foundation/af_core/src/timing/mod.rs` | EDIT | §3, §4: `RUNNER_TOKEN_OVERLAP`, `PLATFORM_FALLBACK_AFTER` |
| `rustd/crates/daemon/afd_runner/src/placement/` (`mod.rs`, `tests.rs`, `sql/*.sql`) | CREATE | §4: setting names, rule table, settings read and write |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql`, `claim_admission_head.sql`, `lease/pull/mod.rs`; `afd_authoring/src/lease/claim/sql/candidates.sql` | EDIT | §4, §5: the setting joins the predicate; the posture refusal becomes the proxy |
| `rustd/crates/daemon/afd_http/src/route/tenant.rs`, `route/runner.rs`, `services/leasing/mod.rs`; `afd_api_tenant/src/handler/tenant/runner/placement.rs`; `afd_api_wire/src/tenant/runners.rs` | CREATE, EDIT | §4, §5: the placement route and the model-call route |
| `rustd/crates/protocol/af_wire/src/paths.rs`, `af_wire/src/policy/mod.rs`, `af_wire/src/redact.rs`, `af_wire/src/runner/model_call.rs` | EDIT, CREATE | §5: `ModelAccess` and model-call frames |
| `rustd/crates/daemon/afd_gate/src/policy/build/mod.rs` | EDIT | §5: direct or proxied access by tenant and posture |
| `rustd/crates/daemon/afd_fleet/src/lease/model_call/` (`mod.rs`, `meter.rs`, `tests.rs`), `lease/usage/mod.rs`, `lease/renew/`, `lease/report/` | CREATE, EDIT | §5: daemon-side turn and metering; runner lines skipped |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/model_call.rs`, `lib.rs`, `openapi.rs` | CREATE, EDIT | §5: the runner-plane route |
| `rustd/crates/runner/afr_supervisor/src/model_proxy/` (`mod.rs`, `tests.rs`), `lib.rs`; `af_providers/src/connect/mod.rs`, `turn.rs` | CREATE, EDIT | §5: the proxied provider |
| `rustd/crates/daemon/afd_api/tests/route_inventory.rs`, `route_meta_total.rs` | EDIT | §4, §5: two templates |
| `rustd/crates/daemon/main/tests/integration_rust_runner_tenant.rs`, `daemon_suite.rs`, `tests/support/fake_model.rs` | CREATE, EDIT | §5: end-to-end run |
| `rustd/crates/protocol/af_observability/src/metrics/declared/fleet.rs`, `producers/fleet/mod.rs`, `docs/metrics.census.tsv` | EDIT | §5: one operator family |
| `cli/src/commands/platform/runners.ts`, `cli/src/program/tree/runners.command.ts` | EDIT | §4: `runners placement` |
| `ui/packages/app/app/(dashboard)/settings/runners/` (`page.tsx`, `actions.ts`, `loading.tsx`, `components/`), `settings/page.tsx` | CREATE, EDIT | §6: Settings → Runners |
| `ui/packages/app/lib/api/tenant_runners.ts`, `lib/auth/scopes.ts`, `lib/analytics/events.ts` | CREATE, EDIT | §6 |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerTile.tsx`, `AddRunnerDialog.tsx` | EDIT | §6: tenant badge; the dialog takes its server action |
| `ui/packages/app/tests/runners-settings-*.test.tsx`, `tests/e2e/acceptance/settings-runners.spec.ts` | CREATE | §6 |
| `docs/AUTH.md`, `docs/architecture/runner_protocol.md`, `runner_fleet.md`, `billing_and_provider_keys.md`, `runner_execution.md`, `postgres_schemas.md`, `observability.md` | EDIT | §7; `runner_execution.md` also records §2's observed results |
| `public/openapi.json` | EDIT (regenerated with `--no-banner`) | §7 |
| `~/Projects/docs`, branch `chore/m251-tenant-runners-changelog` | EDIT | §7 |

## Applicable Rules

- **`domains/auth.md`** — AUTH-DOC, AUTH-GUARD, AWO, AUTH-SECRET, AUTH-NOLOG, AUTH-RANDOM, CTM, CTC
- **`domains/sql.md`** — SCH, MIG, SQL-SLOT, STS, SQL-READER, SQL-ID, SQL-DEADLINE, SQL-LOCKCLOCK, SQL-FILE, SQL-PREPARE, SQL-TIMEOUT, SGR
- **`domains/http.md`** — HTTP-NOUN, HTTP-VERB, HTTP-GARDE, HTTP-VALID, HTTP-ROUTE; **`domains/http-responses.md`** — HTTP-STATEEXT, HTTP-LEAK; **`domains/http-surface.md`** — HTTP-OPENAPI, HTTP-SURFACE, HTTP-ENUM
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SENTINEL, RS-NEWTYPE; **`languages/typescript.md`** — TS-SCHEMA, TS-ERRSTYLE, TS-BUNTEST
- **`code.md`** — CODE-SEAM, FLL, NLG, CODE-ERRREG; **`domains/logging.md`** — LOG-SECRET, LOG-EVENT, OBS
- **§2** — the repository error shell and typed native failures; the existing namespace and cgroup helpers and kernel-lane Make targets. An unrun probe establishes nothing, and an unsupported kernel is recorded as unsupported, never as a pass.
- **`tests.md`** — TEST-NEG, TEST-REAL, TEST-ABSENT, TEST-RACE, TEST-LANE; **`docs.md`** — DOC-02, DOC-03, DOC-11; **`domains/architecture.md`** — ARCH-LAND, ARCH-PUBLISHED

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| Deploy configuration edits | yes, §1 | Indy approves the `fly.toml` change in session before it is committed |
| `gitleaks` at commit and `orly gate pr` | yes, the key-sentinel test | the sentinel is drawn from `af_crypto::entropy` at test time |
| `make lint-all` | yes, the crates, the CLI and the app | `lint-rustd` and the app lint per Section |
| Route inventory and `route_meta_total` | yes, two templates | `INVENTORY` and counts move with §4 and §5 |
| Metrics census and producer tests | yes, one family | census row, declaration and producer in one commit |
| `docs.language` | yes, `AUTH.md` and six architecture pages | sentences under 25 words, paragraphs of at most three |
| Vitest coverage lock | yes, new app files | tests land with each component |
| `make test-runner-kernel` | yes, §2 | the probe runs on a real Linux kernel with root, bubblewrap, Landlock and cgroup v2 |
| File & Function Length | yes, `afd_fleet` lease and the tenant handlers | `model_call/` split into call and meter |

## Prior-Art / Reference Implementations

- **Reference:** the M243 listener and its private CA (`runner_protocol.md`, `playbooks/operations/runner_stream_tls/001_playbook.md`): §1 widens its route and its certificate name, not its protocol.
- **Reference:** the trusted host's sandbox process handle and cgroup cleanup: §2 reuses both, and a mechanism is chosen only after the probe shows which boundary failed.
- **Reference:** `afd_authoring/src/model/capability.rs` and the authoring model call over the stream: a lease-bound capability checked in constant time. Divergence: the runtime turn streams chunk frames and meters runtime usage.
- **Reference:** `schema/20261108121600_model_tenant_selections.sql`: a per-tenant row where a missing row means the default.

## Sections (implementation slices)

### §1 — The stream listener has a public route

The listener gains a public name and a public Fly service on its port. Its certificate covers that name. A runner outside Fly sets `AGENTSFLEET_RUNNER_STREAM_URL` to the public origin and needs no tunnel. The runner's trust code does not change: it already trusts OS roots and adds an optional CA file (`afr_supervisor/src/config/stream.rs`). The certificate authority is open blocker 1. TCP carries HTTP/2 first. HTTP/3 over UDP follows once Fly's public UDP requirements are checked, and the runner falls back to HTTP/2 meanwhile, as M243 specifies.

- **Dimension 1.1** — a runner outside Fly connects → Test `public_stream_route_carries_a_tenant_session` (manual, dev environment): a runner on a host with no WireGuard tunnel, configured with the public URL, a token and the published trust, opens a session; the dev runner gate's heartbeat check passes.
- **Dimension 1.2** — a wrong name is refused → Test `public_route_refuses_an_unmatched_certificate` (native transport integration): a certificate that does not cover the dialled name ends the opening with a certificate refusal and no fallback.

### §2 — The executor's signal boundary is measured

This Section authorizes measurement and inventory only. Every runtime change it leads to waits for Indy's mechanism review. The runner ending deadline mitigates an unresponsive connection; it neither explains why an executor stopped nor proves signal isolation.

The kernel lane first records the executor's and the tenant shell's facts: namespace and host process IDs, real and effective user and group IDs, capabilities, cgroup placement, and where each receives its seccomp and Landlock rules. It proves that every target belongs to this test's sandbox before signalling it. Namespace PID 1 signal rules are measured, never assumed.

The controlled stop probe runs in five steps:
1. The trusted host starts the sandbox and keeps its stable process handle.
2. A tenant shell finds its own executor through the verified namespace mapping.
3. That shell runs `kill -STOP <executor pid>`.
4. The host checks on its own whether the executor stopped and whether its native channel stopped answering.
5. An external watchdog always resumes the executor and destroys the test sandbox, on failure or cancellation too.

The host stays outside the target's stoppable group, and a host PID is never substituted into the tenant namespace without a proven mapping. The syscall result, the process state and the connection behaviour are recorded separately. Finally, the lane inventories `kill`, process-group signals, `tkill`, `tgkill` and `pidfd_send_signal`, including capabilities and inherited descriptors. The chosen mechanism keeps ordinary shell jobs, cancellation, process groups, terminals and executor-driven termination.

- **Dimension 2.1** — kernel evidence binds both actors to the intended sandbox → Test `executor_signal_probe_records_trusted_process_identity` (integration, `make test-runner-kernel`): the record names each process's namespace and host PID, user and group IDs, capabilities and cgroup, and a target outside this test's sandbox is refused before any signal.
- **Dimension 2.2** — a tenant stop attempt has an observed kernel outcome → Test `tenant_stop_of_executor_is_observed_and_always_cleaned_up` (integration, `make test-runner-kernel`): the syscall result, the executor's state and the channel's answer are three separate facts, and the sandbox ends resumed and destroyed.
- **Dimension 2.3** — an interrupted probe leaves nothing stopped → Test `executor_signal_probe_cleanup_survives_interruption` (integration, `make test-runner-kernel`): a probe cancelled mid-run leaves no stopped executor and no sandbox behind.
- **Dimension 2.4** — tenant process control still works → Test `tenant_job_control_survives_executor_signal_isolation` (integration, `make test-runner-kernel`): shell jobs, process groups and cancellation still stop and resume the tenant's own processes.
- **Dimension 2.5** — equivalent routes stay blocked → Test `tenant_signal_variants_cannot_control_the_executor` (integration, `make test-runner-kernel`): `kill`, a process-group signal, `tkill`, `tgkill` and `pidfd_send_signal` from the tenant shell each leave the executor running and its channel answering.

### §3 — Rotation without a restart

`runner.runners` gains `previous_token_hash` and `previous_token_expires_at`, both or neither. `Runners::rotate_token` locks the runner row, moves the current digest to `previous` with `previous_token_expires_at = clock_timestamp() + RUNNER_TOKEN_OVERLAP`, stores the new digest and commits before the response reveals the token. Inside an open overlap, a second rotation replaces only the current digest and keeps the previous one. A retried rotation after a lost response therefore never cuts off the token the host still holds. Revoke ends both at once through the status gate. `select_runner_token.sql` matches the current digest, or the previous one while `previous_token_expires_at > statement_timestamp()`.

`AGENTSFLEET_RUNNER_TOKEN_FILE` names a file holding the token, and exactly one of it and `AGENTSFLEET_RUNNER_TOKEN` may be set. The supervisor reads the file at boot, on SIGHUP and once on a `401`. A changed token replaces the client's bearer and opens a new stream, which supersedes the old. An unchanged token on a `401` halts as `halt/mod.rs` does today. An unreadable file on SIGHUP keeps the current token and logs `runner_token_reload_failed`. The gateway's re-check re-runs the lookup with the digest that opened the connection, so the overlap's end closes a stream on the old token within one re-check interval. **Implementation default:** `RUNNER_TOKEN_OVERLAP` is one hour; a suspected leak is a revoke, which skips the overlap.

- **Dimension 3.1** — both tokens work, then only the new → Test `rotation_keeps_the_old_token_for_the_overlap` (integration): after a rotation, old and new each answer `200` on `GET /v1/runners/me`; with the expiry moved 1 s into the past, old → `401 UZ-RUN-001` and new → `200`.
- **Dimension 3.2** — a repeated rotation keeps the host's token → Test `rotation_inside_the_overlap_keeps_the_previous_token` (integration): two rotations → the original still `200` until its expiry, the first new token `401`, the second `200`.
- **Dimension 3.3** — revoke skips the overlap → Test `revoke_ends_both_tokens` (integration): a revoke inside the overlap → both tokens `401 UZ-RUN-009`.
- **Dimension 3.4** — a stream on the old token closes → Test `overlap_end_closes_a_stream_on_the_old_token` (integration): it closes within one re-check interval of the expiry, and its reconnect answers `401`.
- **Dimension 3.5** — a 401 reloads before it halts → Test `runner_reloads_its_token_file_on_401` (unit, fake daemon): the fake refuses token A while the file holds B → the next call carries B with no halt; with the file still A, the runner halts with `runner_token_refused`.
- **Dimension 3.6** — SIGHUP swaps the token → Test `sighup_swaps_the_token_without_a_restart` (unit, fake daemon): SIGHUP after writing B → the next connection carries B; SIGHUP with the file removed → still A, `runner_token_reload_failed` logged, no halt.
- **Dimension 3.7** — boot refuses an ambiguous or bad source → Test `config_refuses_token_and_token_file` (unit): both variables set → a boot error naming both; an empty file, a missing path or a non-`agt_r` value → a boot error naming `AGENTSFLEET_RUNNER_TOKEN_FILE`.

### §4 — The tenant chooses where its work may run

`runner.tenant_placements` holds one row per tenant with `placement_type`: `tenant_only`, `tenant_then_platform` or `tenant_or_platform`, validated in the application. A missing row reads `tenant_or_platform`, M246_001's fixed rule. `afd_runner::placement` names the three settings and one rule table, and the SQL binds the names from its constants (STS). A platform runner takes a tenant's work unless the tenant chose `tenant_only`, and under `tenant_then_platform` only once the work has waited `PLATFORM_FALLBACK_AFTER` (open blocker 2). The claim decides the setting under the slot lock; the issue re-check stays ownership only. `GET` and `PUT /v1/tenants/me/runner-placement` read and write it, and `agentsfleet runners placement` prints and sets it.

- **Dimension 4.1** — the rule table is complete → Test `placement_rule_covers_every_cell` (unit): three settings by own, platform and foreign, plus `tenant_then_platform` before and after the threshold → 12 verdicts; foreign is false in every setting, platform is false under `tenant_only`.
- **Dimension 4.2** — the setting reads its default and refuses nonsense → Test `placement_setting_refuses_unknown_value` (integration): GET with no row → `tenant_or_platform`; PUT `tenant_only` → `200`; PUT `anywhere` → `400 UZ-REQ-001`, row unchanged.
- **Dimension 4.3** — fallback waits for its threshold → Test `tenant_then_platform_waits_for_the_threshold` (integration): with no own runner online, a platform runner at the threshold less 1 s gets nothing, and at 1 s past it leases.
- **Dimension 4.4** — a new setting refuses a platform holder → Test `placement_change_refuses_the_platform_holder` (integration): T's fleet is held on a platform runner; T sets `tenant_only` → the holder's next claim gets nothing, and T's runner claims once the hold lapses.

### §5 — Platform-model fleets run on tenant runners through a daemon proxy

`ExecutionPolicy` replaces `api_key`, `inference_host` and `base_url` with `model_access`: `direct` with those three fields, or `proxied` with a token, its expiry and an invocation path. `proxied` has no key field. The gate picks `proxied` when the host-locked tenant is set and the posture is `platform`, which replaces M246_001's refusal. Issue draws the capability from `af_crypto::entropy` and stores its SHA-256 as `runner.leases.model_token_hash`.

`POST /v1/runners/me/leases/{lease_id}/model-calls` (`Guard::RunnerBearer`, `runner:self`) checks that the lease is this runner's, active, at its current fence and proxied, and that the capability matches in constant time. `afd_fleet::lease::model_call` re-resolves the platform key, pins the lease's model and opens one `af_inference::stream::StreamTurn`. Its chunks return as newline-delimited JSON (NDJSON) frames. The daemon logs the call's usage line under the lease row lock and commits it before the `end` frame. Renew and report on a proxied lease skip runner request lines. On the runner, `afr_supervisor::model_proxy` implements `af_providers::Connect` over the daemon client.

- **Dimension 5.1** — the platform key never reaches a tenant runner → Test `proxied_lease_carries_no_platform_key` (integration): with a per-run sentinel key, T's runner leases T's platform-model fleet, makes one proxied call and reports. The sentinel appears in no lease body, frame, answer, `runner.leases` row or daemon log line, and the fake provider received exactly the sentinel.
- **Dimension 5.2** — a wrong or stale capability spends nothing → Test `model_call_refuses_a_wrong_capability` (integration): a wrong token, a reclaimed lease's token and a stale fence each answer `409 UZ-RUN-005`, and the fake provider counts zero calls.
- **Dimension 5.3** — direct and foreign leases have no proxy → Test `model_call_refuses_a_direct_or_foreign_lease` (integration): a `direct` lease, or another runner's proxied lease → `404 UZ-RUN-006`, zero provider calls.
- **Dimension 5.4** — the daemon writes the token bill → Test `proxied_usage_is_metered_by_the_daemon` (integration): the fake provider reports 1,000 input and 200 output tokens while the runner claims 1 and 1 → one request line of 1,000 and 200, none from the runner, and `agentsfleet_proxied_model_calls_total{outcome="ok"}` rose by one.
- **Dimension 5.5** — a provider failure ends the turn cleanly → Test `proxied_call_reports_a_provider_failure` (integration): the fake answers `503` after some text → an `error` frame, known usage logged once, and the turn fails as a direct provider failure does.
- **Dimension 5.6** — the runner rebuilds the stream faithfully → Test `proxied_provider_streams_daemon_chunks` (unit): scripted frames → the same `Chunk` sequence; a stream cut before `end` → an error; an unknown frame type → an error naming it.
- **Dimension 5.7** — a real tenant host completes a chat → Test `tenant_runner_completes_a_platform_model_chat` (e2e, `integration_rust_runner::`): a tenant enrols, the runner boots from a token file, leases a platform-model chat, completes it through the proxy against the fake model, and its report lands.

### §6 — Settings → Runners and the admin badge

Settings → Runners (`app/(dashboard)/settings/runners/`) shows only with `runner:tenant`: the tenant's runners, Add runner (`AddRunnerDialog` and `PolicyFields` with a tenant server action), the reveal-once dialog, row actions and the placement choice. Its three choices read "Only my runners; work waits", "My runners, then AgentsFleet hosts" and "Any matching runner, mine or AgentsFleet's". Under `tenant_only` with no runner online, the page says "Work waits until one of your runners is online". The admin `RunnerTile` shows a Tenant badge on a tenant runner. `runner_token_minted` gains `plane` (`platform` or `tenant`), and `runner_placement_changed` carries `placement`.

- **Dimension 6.1** — the page hides without the scope → Test `runners settings page redirects without runner:tenant` (unit, Vitest): no scope → redirect to `/settings`, and the list is never fetched.
- **Dimension 6.2** — the wait warning is earned → Test `tenant-only placement warns only with no runner online` (unit, Vitest): `tenant_only` with two offline runners → the warning shows; with one online → it does not.
- **Dimension 6.3** — the signed-in path works → Test `settings-runners.spec.ts` (e2e, Playwright acceptance, run locally before the Pull Request): an owner adds a runner, sees the token once and never after closing, finds the row, and sets `tenant_only`, which survives a reload.
- **Dimension 6.4** — product events carry fixed words only → Test `runner settings actions capture plane and placement` (unit, Vitest): a tenant enrolment captures `runner_token_minted` with `plane: "tenant"`; a placement save captures `runner_placement_changed` with `placement` alone; a refused save captures nothing.

### §7 — The docs follow

`runner_protocol.md` documents the public route, its trust and the external runner's prerequisites. `docs/AUTH.md` adds overlap rotation, the token file and proxied model access. `runner_fleet.md` adds the placement setting. `billing_and_provider_keys.md` states that proxied usage is metered by the daemon. `runner_execution.md` records §2's observed results and notes that a proxied lease's supervisor holds no key. `public/openapi.json` is regenerated. The docs repository's "Run your own runner" page gains the public URL, the token file and SIGHUP.

- **Dimension 7.1** — the pages read clean → Test `orly gate verify` (integration): zero `docs.language` findings on the changed pages.

## Interfaces

| Method and path | Scope | Answers |
|---|---|---|
| `GET`, `PUT /v1/tenants/me/runner-placement` (placeholder, open blocker 3) | `runner:tenant` | `{"placement": "tenant_only"}`, one of `tenant_only`, `tenant_then_platform` and `tenant_or_platform` |
| `POST /v1/runners/me/leases/{lease_id}/model-calls` (placeholder, open blocker 4) | `runner:self` | `200` `application/x-ndjson` frames `text`, `call`, `usage`, `end` or `error`; `404 UZ-RUN-006`; `409 UZ-RUN-005` |

A proxied lease's policy carries `"model_access": {"type": "proxied", "token", "expires_at", "invocation_path"}`; a direct lease carries `{"type": "direct", "api_key", "inference_host", "base_url"}`. The runner reads `AGENTSFLEET_RUNNER_TOKEN_FILE` and reloads on SIGHUP. `af_core::timing` adds `RUNNER_TOKEN_OVERLAP` and `PLATFORM_FALLBACK_AFTER`. The stream URL gains a public origin; the stream protocol does not change.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Untrusted listener certificate | security | a name or issuer the runner does not trust | the opening fails closed, no fallback | `public_route_refuses_an_unmatched_certificate` |
| Tenant shell stops its executor | security | shared namespace or credentials | the probe records the outcome; a demonstrated route goes to Indy's mechanism review | `tenant_stop_of_executor_is_observed_and_always_cleaned_up` |
| Probe interrupted | timeout | a timeout or failed assertion mid-run | the watchdog resumes the executor and destroys the sandbox | `executor_signal_probe_cleanup_survives_interruption` |
| Another signal route | security | `tkill`, `tgkill`, `pidfd_send_signal` or a group signal | each route is inventoried and blocked | `tenant_signal_variants_cannot_control_the_executor` |
| A remedy breaks tenant job control | failure | isolation also blocks the tenant's own signals | the mechanism is refused at review | `tenant_job_control_survives_executor_signal_isolation` |
| Host never got the new token | auth failure | the overlap passed | `401 UZ-RUN-001`; one reload, then a halt | `rotation_keeps_the_old_token_for_the_overlap`, `runner_reloads_its_token_file_on_401` |
| Rotation retried | replay | a lost rotate answer | the host's token stays previous; the undelivered one dies | `rotation_inside_the_overlap_keeps_the_previous_token` |
| Live stream outlives the overlap | auth failure | a connection opened on the old token | closed within one re-check | `overlap_end_closes_a_stream_on_the_old_token` |
| Token file unreadable on SIGHUP | failure | a removed file or bad permissions | current token kept, failure logged | `sighup_swaps_the_token_without_a_restart` |
| Unknown placement | malformed input | a typo or a stale client | `400 UZ-REQ-001`, row unchanged | `placement_setting_refuses_unknown_value` |
| Placement changed under a hold | race | `tenant_only` while a platform runner holds | the holder is refused; an own runner claims after the hold | `placement_change_refuses_the_platform_holder` |
| Tenant host under-reports tokens | security | a modified runner sends small lines | the daemon meters from the provider; runner lines skipped | `proxied_usage_is_metered_by_the_daemon` |
| Wrong or stale capability | replay | a guessed token, a reclaimed lease | `409 UZ-RUN-005`, no provider call | `model_call_refuses_a_wrong_capability` |
| Provider down mid-turn | dependency unavailable | `503` or a timeout | `error` frame; known usage logged once | `proxied_call_reports_a_provider_failure` |

## Invariants

1. A proxied lease carries no platform key. `ModelAccess::Proxied` has no key field, and the gate chooses it from the host-locked tenant; `proxied_lease_carries_no_platform_key`.
2. At most two digests authenticate a runner, the older only until its expiry. The both-or-neither check and the lookup's `statement_timestamp()` predicate enforce it; `rotation_keeps_the_old_token_for_the_overlap`.
3. Token charges on a proxied lease come only from attempts the daemon ran. Renew and report skip runner lines when `model_token_hash` is set; `proxied_usage_is_metered_by_the_daemon`.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_proxied_model_calls_total` | operator | a proxied turn ends | `outcome` (`ok`, `provider_error`, `refused`) | no identifier, no model text | `proxied_usage_is_metered_by_the_daemon` |
| `runner_token_reloaded`, `runner_token_reload_failed` (log events) | operator | the supervisor reads its token file | `source` (`sighup`, `unauthorized`) | never the token or its digest | `sighup_swaps_the_token_without_a_restart` |
| `runner_token_minted` (gains `plane`) | product | an enrolment succeeds | `runner_id`, `sandbox_tier`, `plane` | opaque identifier only | `runner settings actions capture plane and placement` |
| `runner_placement_changed` | product | a save changes the setting | `placement` | a fixed word only | `runner settings actions capture plane and placement` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A proxied lease never carries the platform key (§5) | `grep -c "proxied_lease_carries_no_platform_key ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R2 | The daemon, not the host, writes the proxied token bill (§5) | `grep -c "proxied_usage_is_metered_by_the_daemon ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R3 | A real tenant host completes a platform-model chat (§5) | `grep -c "tenant_runner_completes_a_platform_model_chat ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R4 | Rotation overlaps, then the old token gets 401 (§3) | `grep -c "rotation_keeps_the_old_token_for_the_overlap ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R5 | The runner reloads its token on a 401 without a restart (§3) | `grep -c "runner_reloads_its_token_file_on_401 ... ok" .tmp/rustd-unit-runner.log` | `1` | P0 | |
| R6 | A placement change refuses a platform holder (§4) | `grep -c "placement_change_refuses_the_platform_holder ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R7 | The executor's signal boundary has an observed kernel outcome, with job control kept (§2) | `make test-runner-kernel` | exit 0 | P0 | |
| R8 | The new page renders and stays first-party (§6) | `make dry-app` | exit 0 | P0 | |
| R9 | The public stream route refuses a certificate that does not cover its name (§1) | `grep -c "public_route_refuses_an_unmatched_certificate ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `ExecutionPolicy`'s top-level `api_key`, `inference_host` and `base_url` | `git grep -n -e "policy\.api_key" -e "policy\.inference_host" -e "policy\.base_url" -- rustd` | no output |
| M246_001's posture refusal in the pull step | `git grep -n "PLATFORM_MODEL_REF" -- rustd/crates/daemon/afd_fleet` | no output |

## Out of Scope

- One-time enrolment codes and self-registration: excluded by decision.
- Counting holds, auto-cordon and checkpoint respawn: M247.
- For §2: claiming that the ending deadline fixes signal isolation, probing production, broad host process control, and kernel-policy changes nobody has reviewed.
- Cloudflare in front of the stream: M243 rules it out as a prerequisite.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An owner enrols a runner from Settings → Runners, starts it on a laptop at home with the public stream URL, and a fleet on the platform default model answers through it.
2. **Preserved user behaviour** — Platform runners and M246_001's tenant runners on self-managed models run as before; a tenant with no placement row sees no change.
3. **Optimal-way check** — The public route, the token file and the proxy deliver the moment. The gap: a platform-model turn on a tenant runner takes one extra hop through the daemon.
4. **Rebuild-vs-iterate** — Iterate: the M243 listener, the authoring capability and the M246 routes are extended.
5. **What we build** — A public stream route, the signal probe, overlap rotation with file reload, the placement setting, the model proxy and the Settings page.
6. **What we do NOT build** — Enrolment codes, tenant lease history, cross-tenant sharing, per-runner graphs.
7. **Fit with existing features** — Compounds with M243's stream and M246's ownership filter. The claim, the hottest statement, gains one join on the placement row.
8. **Surface order** — API and CLI with the page in the same Pull Request, because the placement choice is a settings decision.
9. **Dashboard restraint** — No per-runner graphs and no tenant events tab. The one computed signal is the wait warning under `tenant_only` with no runner online.
10. **Confused-user next step** — "My fleet doesn't start": `agentsfleet runners placement` prints the setting and "0 of 2 runners online; work waits until one connects".

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the route first, because nothing outside Fly connects without it; then the signal audit, then rotation, the setting, the proxy, the page and the docs.
- **Alternatives considered:** a WireGuard tunnel per tenant, rejected because it opens the whole private Fly network; Cloudflare in front of the stream, ruled out by M243; a byte-forwarding proxy, rejected because it needs a usage parser per format and forwards any body on the platform key; rotating twice to kill a leaked token, rejected because a retried rotation would cut off a healthy host.
- **Patch-vs-refactor verdict:** a **patch**; the only reshaped type is `ExecutionPolicy`'s model access.

## Discovery (consult log)

- **Consults** — Source review on `main` `97224ed74`, Nov 17, 2026:
  - Prod declares no public Fly service, and Cloudflare Tunnel reaches only the API on port 3000 (`deploy/fly/agentsfleetd-prod/fly.toml`). The stream binds `[::]:4443`.
  - The listener certificate names `agentsfleetd-dev.internal` and `agentsfleetd-prod.internal`, issued by a ten-year private CA held in the `agentsfleet-runner-stream` vault item (`docs/architecture/runner_protocol.md`).
  - The runner verifies the daemon with OS roots plus an optional CA file and presents no client certificate; its bearer token proves it (`af_stream/src/tls/boring.rs`, `afr_supervisor/src/downlink/mod.rs`).
  - unverified: Fly's public UDP needs a dedicated IPv4 address and a special bind address; Fly's UDP documentation settles it before HTTP/3 goes public.
- **Owner decisions** —
  > Indy (Nov 06, 2026: 10:43 AM): "2. Respawn across ownership - this is a tenant setting" — context: §4's three-option setting.
  > Indy (Nov 17, 2026): "I dont want to decide on this. as its not important and it depends on the CA authority the agentsfleet have." — context: §1's certificate authority, left as open blocker 1.
  > Indy (Nov 17, 2026): "To me in this spec, we need these, can you trim the spec and create this one in the current spec, the other review audit and move to a new spec after M250 (the last current sequence)" — context: the transfer of the audit and the remaining Sections from M246_001 into this spec.
  > Indy (Nov 17, 2026): "i prefer to move that M251 since i dont understand that this runner-settings is ?" — context: the placement URL and the model-call transport stay open in this spec as blockers 3 and 4.
- **Folded audit (§2)** — the former pending `M240_001` executor signal-isolation audit, written Nov 04, 2026 and carried in PR #803, folded into M246_001 on Nov 10, and moved here on Nov 17. Its runtime mechanism remains unapproved, and its causal explanation remains a hypothesis.
  > Indy: "The root cause is still open."
  > Indy: "Running kill -STOP <executor pid> from a shell call in the Linux kernel lane would settle it"
- **Finding on label placement (Nov 17, 2026)** — a runner carries `labels`, set at registration and by `PATCH`. A fleet carries `required_tags`, and the scan places it only on a runner whose labels contain every tag (`select_ledger_candidates.sql`, `required_tags <@ labels`). Fleet creation binds no tags (`afd_authoring/src/creation/reserve.rs`, `NO_TAGS`), and no route, command or page sets them, so every fleet reaches every runner by labels. Labels on a tenant runner therefore cannot narrow what it takes. §4's `tenant_only` setting keeps a tenant's work on its own runners as a whole. Pinning one fleet to one runner would need a way to set `required_tags`, and it is not scoped here.
- **Model reviews** — none yet.
- **Open blockers** —
  1. **The stream's certificate authority.** Options: re-issue the leaf from the existing private CA with the public name and publish the CA file (no renewal job; the CA signing key, readable by the CI service account, then guards customer hosts); or a public CA such as Let's Encrypt (no CA file for customers; a renewal job every ~60 days). ☠️ my call: the existing private CA, because the runner already loads a CA file and the ten-year lifetime is approved.
  2. **The platform-fallback threshold for `tenant_then_platform`.** ☠️ my call: one `LEASE_TTL`, in `af_core::timing`; the alternatives are falling back only when none of the tenant's runners is online, or a per-tenant column.
  3. **The placement setting's URL.** Its values are settled (`tenant_only`, `tenant_then_platform`, `tenant_or_platform`); the path is not, and the Interfaces row stays a placeholder until Indy picks. Candidates: `GET` and `PATCH /v1/tenants/me/runner-settings`, one settings record per tenant holding `placement`, beside `/v1/tenants/me/provider`; a `runner_placement` field on a new `/v1/tenants/me` resource; or the placeholder `GET` and `PUT /v1/tenants/me/runner-placement`. Indy's constraint (Nov 17, 2026): "The tenant urls must be v1/tenants/me/runners." The setting's path therefore sits under `/v1/tenants/me/runners`, and the `runner-settings` sibling is out. ☠️ my call within it: `GET` and `PATCH /v1/tenants/me/runners/settings`, a static segment the router prefers over `{runner_id}`. No admin counterpart is planned: the setting is a tenant's preference about its own work, and the platform side has no equivalent value. An operator read, if support needs one, would be `GET /v1/admin/tenants/{tenant_id}/runner-settings`, the first route of an admin tenants family that does not exist today.
  4. **How a proxied model call travels.** Candidates: a message on the runner stream's `work` lane, as authoring model calls travel since M243 (`af_wire/src/protocol/up/mod.rs`), with no URL; or the placeholder `POST /v1/runners/me/leases/{lease_id}/model-calls`. ☠️ my call: the stream message.
- **Deferrals and transfers** — received from M246_001 under Indy's Nov 17 quote above; none deferred onward. Each criterion the Nov 06 M246_001 draft carried is this spec's own P0 row: its executor signal row is R7, its overlap rotation row R4, its token reload row R5, its proxied key and metering rows R1 and R2, its proxied chat row R3, and its Settings page row R8. R6 and R9 are new.
