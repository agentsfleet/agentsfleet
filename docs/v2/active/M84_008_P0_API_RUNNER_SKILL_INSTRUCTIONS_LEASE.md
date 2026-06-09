# M84_008: Trigger-time SKILL.md instructions reach NullClaw

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 008
**Date:** Jun 08, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — launch-blocking runtime correctness. Installed agents currently store their behavior prose, but the split runner path does not prove that prose reaches the sandboxed NullClaw turn at trigger time.
**Categories:** API
**Batch:** B1 — standalone runner/control-plane wire repair; may run after the active M84 sandbox-hardening worktree closes or on a branch rebased over it.
**Branch:** feat/m84-skill-instructions-lease
**Depends on:** M80_002 runner cutover — the bug exists only in the split `zombied` / `zombie-runner` path. M84_006 is adjacent sandbox-depth work and is not a dependency.
**Provenance:** agent-generated from Indy's Jun 08, 2026 architecture review question about whether the installed platform-ops `SKILL.md` actually runs after a GitHub deploy-failure trigger.

> **Provenance is load-bearing.** The issue was code-grounded against `main`: install stores `source_markdown`; `ZombieSession.instructions` extracts the body; `LeasePayload` omits it; `child_exec.buildCallArgs` composes only the event message; `engine.execute` is called with `context = null`.

**Canonical architecture:** `docs/architecture/runner_fleet.md` §Running one event and `docs/architecture/data_flow.md` §C.EXECUTE. This spec reconciles those docs with the actual lease payload.

---

## Implementing agent — read these first

1. `docs/architecture/scenarios/01_default_install.md` — the platform-ops happy path; it states that NullClaw runs the installed `SKILL.md` prose against GitHub webhook and steer events.
2. `docs/architecture/data_flow.md` — canonical event → lease → sandboxed child → report flow, including secret-delivery boundaries.
3. `src/zombied/fleet/zombie_session.zig` and `src/zombied/fleet/service.zig` — where `source_markdown` is loaded, instructions are extracted, and leases are issued.
4. `src/lib/contract/protocol.zig` and `src/runner/child_exec.zig` — the shared runner protocol and the current message assembly gap.
5. `samples/platform-ops/SKILL.md` and `~/Projects/skills/usezombie-install-platform-ops/SKILL.md` — the regression fixture and installer behavior the runtime must honor.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `fix(m84_008): deliver installed skill instructions on runner leases`
- **Intent (one sentence):** When any installed agent wakes by webhook, cron, or steer, the runner must give NullClaw the stored `SKILL.md` body plus the event payload, while keeping raw secrets only in policy/tool-bridge state.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intended flow in your own words and list `ASSUMPTIONS I'M MAKING:`. Confirm whether backward compatibility with older runners is required before changing the wire shape; if it is required, document the additive/default behavior before editing.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`**:
  - **RULE NDC** — no compatibility shim, fallback prompt, or install-time duplicate storage that is unused by production.
  - **RULE NLR** — files touched for the lease gap stay clean; do not opportunistically refactor unrelated fleet or runner code.
  - **RULE UFS** — any JSON field names, prompt-section labels, and sentinel strings are single-sourced constants when reused.
  - **RULE TST-NAM** — tests use descriptive behavior names, not milestone IDs.
  - **RULE NLG** — no new legacy framing before v2.0.0.
- **`dispatch/write_zig.md`** — required for every `*.zig` edit: memory/lifecycle discipline, file/function length, cross-compile both Linux targets.
- **`docs/architecture/runner_fleet.md` / `docs/architecture/data_flow.md`** — architecture consult is mandatory because this changes the runner lease flow.
- **`dispatch/write_ts_adhere_bun.md`** — only if the Command-Line Interface (CLI) or User Interface (UI) install surfaces need text updates; no TypeScript runtime change is expected.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `*.zig` protocol/control-plane/runner edits | Read `dispatch/write_zig.md`; run unit tests plus cross-compile both Linux targets. |
| PUB / Struct-Shape | yes — `LeasePayload` gains a field | Additive field only; no unrelated exported type churn; update protocol tests. |
| File & Function Length | yes | Keep prompt assembly small; split helper only if it prevents `child_exec.zig` or a function from crossing the cap. |
| UFS | yes | Single-source the prompt labels such as `Installed instructions` and `Trigger event`. |
| LOGGING / ERROR REGISTRY | maybe | No new logs expected; if a malformed lease becomes an explicit failure, use an existing error code or add a registered one with a negative test. |
| SCHEMA / UI / DESIGN TOKEN | no | No database schema or design-system change. |

---

## Overview

**Goal (testable):** A lease issued for an installed platform-ops agent carries the extracted `SKILL.md` body to the runner, and the sandboxed NullClaw prompt includes both that body and the trigger event message without including raw tool or provider secret bytes.

**Problem:** The documented architecture says installed behavior lives in `SKILL.md` and that NullClaw runs that prose on each trigger. The current split runner path appears to stop short: `zombied` stores and extracts `source_markdown`, but `LeasePayload` carries only event + `ExecutionPolicy`; the runner child calls the engine with the event message and `context = null`. A GitHub deploy-failure webhook can therefore wake a platform-ops agent that has credentials and tools, but not the instructions that tell it to fetch GitHub logs, correlate Fly/Upstash evidence, and post Slack diagnostics.

**Solution summary:** Add the installed instructions to the runner lease as a first-class, additive field populated from `ZombieSession.instructions`. The runner composes the NullClaw turn from two explicit parts: installed instructions and trigger event. Secrets remain outside the prompt: provider key stays in `ExecutionPolicy.provider/api_key`, tool credentials stay in `secrets_map`, and placeholder substitution remains inside the policy-aware tool bridge.

**User outcome lens:** This preserves the one-time install behavior: a user installs an agent from `SKILL.md` + `TRIGGER.md` once, then later steer/webhook/cron events inherit that behavior without the user re-pasting instructions into every trigger. The setup step that gets easier is trigger wiring: after credentials and webhook/cron registration, the event only carries facts about what happened, not the full operating playbook. The first successful user moment is a deploy-failure or smoke-test event waking the installed platform-ops agent and receiving an answer that follows the stored `SKILL.md` body: fetch the right logs, correlate the right systems, and post or return a concrete diagnosis.

**Concurrency posture:** Current `zombie-runner` is serial: one daemon loop holds one live lease, forks one sandboxed child, waits/renews/reports, then polls again. The Postgres/Redis control-plane primitives are already concurrent enough for racing lease calls (`fleet.runner_affinity` conditional claim + fencing, Redis pooled per-command connections), and the runner memory plane is **already lease/zombie-scoped** (hydrate keyed by `zombie_id`, capture fenced by `lease_id` + `fencing_token` server-side, idempotent writes) — it does **not** assume one live lease, resting instead on the per-zombie **affinity-slot single-holder** claim (`uq_runner_affinity_zombie` + the `leased_until` time-gate) plus capture-time fencing (not a unique constraint on the leases table). This spec fixes the per-lease instruction payload and must not introduce a multi-lease runner scheduler; the additive `instructions` field makes each lease **more** self-contained, which is a prerequisite the multi-lease worker pool (M88_002) builds on. The remaining gate to running N children in one runner process is the daemon loop split (M88_002), not the memory plane.

---

## Prior-Art / Reference Implementations

- **`docs/architecture/scenarios/01_default_install.md`** — defines the user-visible platform-ops behavior. The fix makes implementation match this scenario instead of weakening the scenario.
- **`src/zombied/fleet/zombie_session.zig`** — already extracts `instructions` from `source_markdown`; use it as the source of truth instead of reparsing markdown in the runner.
- **`src/runner/engine/runner_helpers.zig` / `runner.composeMessage` tests** — existing context composition and secret-non-injection tests show where prompt assembly belongs and how to test it.
- **`samples/platform-ops/SKILL.md`** — regression fixture for the exact GitHub deploy-failure / morning-health-check prose Indy asked about.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/lib/contract/protocol.zig` | EDIT | Add `instructions: []const u8 = ""` (additive + **defaulted**, mixed-version-safe) to `LeasePayload`; update comments/child input serialization. |
| `src/lib/contract/protocol_test.zig` | EDIT | Add protocol roundtrip coverage proving instructions survive JSON serialization. |
| `src/zombied/fleet/service.zig` | EDIT | Populate the lease field from `ZombieSession.instructions` during fresh and reclaimed leases. |
| `src/zombied/fleet/control_plane_integration_test.zig` or nearest lease integration test | EDIT | Assert a created agent's `source_markdown` body appears in the issued lease. |
| `src/runner/child_exec.zig` | EDIT | Compose the engine input from installed instructions plus event message/payload; keep secrets out. |
| `src/runner/engine/runner_security_test.zig` or nearest runner prompt test | EDIT | Prove prompt composition includes instructions + event and excludes raw secrets. |
| `samples/platform-ops/SKILL.md` | READ / fixture only | Use as regression input; edit only if the runtime requires an instruction wording fix. |
| `docs/architecture/data_flow.md` and `docs/architecture/runner_fleet.md` | EDIT | Land the architecture correction in the same commit as the flow change. |
| `docs/architecture/scenarios/01_default_install.md` | EDIT | Make the trigger-time delivery of stored `SKILL.md` explicit in the scenario. |

No CLI or UI install-surface change is expected: both already submit `{trigger_markdown, source_markdown}`.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** focused wire repair. Carry instructions beside the event on the lease, then compose the child prompt in the runner. This is the smallest change that makes documented runtime behavior true.
- **Alternatives considered:** (a) store instructions in `ExecutionPolicy` — rejected because policy is the hard-enforcement/tool boundary; `SKILL.md` is soft reasoning input. Mixing them makes secret and prompt boundaries harder to audit. (b) re-fetch `source_markdown` from the runner — rejected because the runner must hold zero datastore credentials and should not gain a control-plane read for agent config. (c) inject instructions at install smoke-test only — rejected because production webhook/cron/steer must share one reasoning loop.
- **Patch-vs-refactor verdict:** this is a **patch** because the architecture and storage model are already correct; the missing piece is the lease-to-prompt handoff.
- **Concurrency verdict:** this is **not** the one-runner-many-leases refactor. The implementation must keep current serial runner semantics intact while making every individual lease self-contained. Do not patch memory routing, capacity scheduling, or batch lease APIs here.

---

## Sections (implementation slices)

### §1 — Lease carries installed instructions

The lease reply becomes self-contained for reasoning input: event, hard policy, and installed instructions all cross together. The field is populated from the already-extracted session value.

- **Dimension 1.1** — `LeasePayload` gains an additive, **defaulted** `instructions: []const u8 = ""` field (matching the protocol's existing mixed-version-safe additive fields, e.g. `ReportRequest.failure_reason`/`input_tokens`) that JSON roundtrips unchanged, **and a payload omitting `instructions` parses to `""`** (not an error) → Test `lease payload preserves installed instructions` + `lease payload defaults instructions when absent`
- **Dimension 1.2** — `issueLease` sets the field from `ZombieSession.instructions` for fresh leases → Test `lease issuance includes extracted skill body`
- **Dimension 1.3** — reclaimed leases use the same session extraction path and do not lose instructions → Test `reclaimed lease keeps installed instructions`

### §2 — Runner prompt assembly uses instructions plus event

The sandboxed child gives NullClaw explicit, ordered input: installed instructions first, trigger/event second. Manual steer, webhook, cron, and continuation all use the same assembly. **Implementation route (preserve the secret boundary by construction):** extend the existing `composeMessage` **allowlist** (`runner_helpers.zig` — the fixed `fields` array, today 5 keys) with an `instructions` key rather than building a new assembly path, and pass a populated `context` object into `engine.execute` instead of `null`. This keeps the existing secret-non-injection allowlist test (`runner_security_test.zig` "composeMessage allowlist is tight") covering the new field — a fresh assembly path would bypass exactly the guard §3 relies on.

- **Dimension 2.1** — child prompt includes the installed instructions before the event message/payload → Test `runner prompt includes installed instructions before trigger event`
- **Dimension 2.2** — non-JSON and JSON event bodies still reach the prompt after instructions → Test `runner prompt preserves raw event payload when message field is absent`
- **Dimension 2.3** — when `instructions` is empty (frontmatter-only `SKILL.md`), the runner emits an **explicit sentinel section** (`## Installed instructions` followed by `(no installed instructions)`) **and logs a warning** — it never silently omits the section or degrades to a generic chat bot. (Create/install-time rejection of an empty body is a complementary upstream guard — Out of Scope here.) → Test `runner handles empty installed instructions explicitly`

### §3 — Secret boundary remains unchanged

Instructions are prompt input; secrets are not. The implementation must not stringify `ExecutionPolicy` into the prompt and must not substitute placeholders before the tool bridge.

- **Dimension 3.1** — raw `secrets_map` values never appear in the composed prompt → Test `composed prompt excludes tool secret bytes`
- **Dimension 3.2** — provider API key never appears in the composed prompt → Test `composed prompt excludes provider key`
- **Dimension 3.3** — placeholder strings in `SKILL.md` remain placeholders until a permitted tool call → Test `skill placeholders are not pre-substituted in prompt`

### §4 — Architecture and scenario docs match the fixed flow

The docs must say exactly when stored `SKILL.md` is read: install stores it; trigger-time lease delivers it; runner prompt uses it.

- **Dimension 4.1** — `data_flow.md` and `runner_fleet.md` show `instructions` in the lease input without weakening hard policy language → Test `architecture docs mention trigger-time instructions delivery`
- **Dimension 4.2** — scenario 01 explicitly states that GitHub deploy-failure and install smoke-test steer both use stored `SKILL.md` delivered on lease → Test `scenario documents one reasoning loop with stored skill body`

---

## Interfaces

Runner lease payload, additive wire field:

```text
LeasePayload
  lease_id
  fencing_token
  lease_expires_at
  secret_delivery
  event
  policy
  instructions    installed SKILL.md body after frontmatter extraction
                  (additive + DEFAULTED: `instructions: []const u8 = ""`, like the
                   protocol's existing mixed-version-safe additive fields)
```

Prompt assembly shape, observable in tests but not exposed as a public API:

```text
Installed instructions
<stored SKILL.md body>

Trigger event
<event message if present, otherwise request_json>
```

The engine continues to receive provider config and hard tool policy through `agent_config`, `tools_spec`, and `ExecutionPolicy`; this spec only adds missing reasoning prose.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Generic-agent run | lease omits instructions or runner ignores them | Tests fail; at runtime the runner must not silently strip the installed behavior. |
| Secret prompt leak | implementation serializes `ExecutionPolicy` or `secrets_map` into the prompt | Security tests fail; raw secret bytes remain only in policy/tool-bridge memory. |
| Placeholder pre-substitution | runner replaces `${secrets.NAME.FIELD}` before the tool call | Negative test fails; placeholders stay harmless text until a permitted tool bridge call. |
| Old runner receives new lease (mixed deploy) | new `zombied` emits `instructions`; an **old** runner parses `LeaseResponse` with `std.json` **default** options (`control_plane_client.zig` — *not* `ignore_unknown_fields`) | the old runner gets `error.UnknownField` → `MalformedResponse` → it runs **no work** (fails loud, NOT a silent generic chatbot). Mitigation: the field is additive+defaulted (so new-runner/old-`zombied` parses `""` cleanly), and **rollout is runners-first** — deploy all runners before `zombied` starts emitting `instructions`. |
| Empty `SKILL.md` body | install/dashboard accepts frontmatter-only source markdown | Runner emits the explicit `(no installed instructions)` sentinel section + a warning log (never a silent generic-chat fallback). Create/install-time rejection of an empty body is a complementary upstream guard (Out of Scope here). |
| Oversized instruction body | operator installs a huge `SKILL.md` | Existing request/lease size limits apply; if a new limit is added, it is named and tested with a clear error. |

---

## Invariants

1. **Runtime behavior comes from installed `SKILL.md`** — every lease for a runnable agent carries the extracted instructions. Enforced by lease integration tests.
2. **One reasoning loop for all actors** — steer, webhook, cron, and continuation pass through the same prompt assembly. Enforced by tests covering at least steer-shaped and webhook-shaped request bodies.
3. **Secrets are not prompt material** — no raw tool secret or provider key is present in composed input. Enforced by negative security tests using planted secret values.
4. **The runner stays datastore-blind** — instructions arrive on the lease; the runner does not query Postgres, Redis, or Vault. Enforced by unchanged build graph/import boundaries and no new runner control-plane fetch.
5. **Docs and code name the same flow** — architecture docs include the field/flow in the same implementation commit. Enforced by grep-based acceptance criteria.
6. **No accidental multi-lease behavior** — one `zombie-runner` daemon remains one live lease in this spec; concurrent children in one daemon are out of scope here (that is M88_002, which the already-lease-scoped memory plane and this spec's self-contained lease both enable).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `lease payload preserves installed instructions` + `lease payload defaults instructions when absent` | JSON encode/decode of `LeasePayload` keeps `instructions = "Do platform ops"`; a payload **omitting** `instructions` decodes to `""` (defaulted, not an error). |
| 1.2 | integration | `lease issuance includes extracted skill body` | Create agent with frontmatter + body; lease response contains body text without frontmatter. |
| 1.3 | integration | `reclaimed lease keeps installed instructions` | Expire/reclaim a lease; replacement lease still carries the same instructions. |
| 2.1 | unit | `runner prompt includes installed instructions before trigger event` | Composed prompt contains instruction text before `workflow_run failure`. |
| 2.2 | unit | `runner prompt preserves raw event payload when message field is absent` | Webhook payload without `message` appears after instructions as JSON text. |
| 2.3 | unit | `runner handles empty installed instructions explicitly` | empty instructions → explicit `(no installed instructions)` sentinel section + warning log; never silent generic chat. |
| 3.1 | unit | `composed prompt excludes tool secret bytes` | Planted `secrets_map.github.api_token = ghs_secret` is absent from prompt. |
| 3.2 | unit | `composed prompt excludes provider key` | Planted provider key is absent from prompt. |
| 3.3 | unit | `skill placeholders are not pre-substituted in prompt` | `${secrets.github.api_token}` remains literal in instructions. |
| 4.1 | docs | `architecture docs mention trigger-time instructions delivery` | grep finds `instructions` / `SKILL.md` in lease sections of data-flow and runner-fleet docs. |
| 4.2 | docs | `scenario documents one reasoning loop with stored skill body` | scenario 01 states stored `SKILL.md` is delivered on each lease. |

- **Regression:** existing CLI/UI install tests still POST only `trigger_markdown` + `source_markdown`; no install API expansion.
- **Idempotency/replay:** reclaimed lease coverage in Dimension 1.3 proves retry/replay keeps instructions stable.

---

## Acceptance Criteria

- [ ] Lease wire includes installed instructions — verify: `zig build test -- src/lib/contract/protocol_test.zig` or nearest repository test target.
- [ ] Control-plane lease issuance includes extracted `SKILL.md` body — verify: targeted fleet integration test for lease issuance.
- [ ] Runner prompt contains instructions + event and excludes secrets — verify: targeted runner unit/security tests.
- [ ] Platform-ops regression passes with `samples/platform-ops/SKILL.md` as fixture — verify: targeted test names sample fixture or copies its body into a fixture.
- [ ] Architecture docs updated in the implementation commit — verify: `git grep -n "instructions\|SKILL.md" docs/architecture/data_flow.md docs/architecture/runner_fleet.md docs/architecture/scenarios/01_default_install.md`.
- [ ] Standard gates clean: `make lint-zig`, `make test-unit-zigrunner`, relevant fleet integration test, both Linux cross-compiles, and `gitleaks detect`.

---

## Eval Commands (post-implementation)

```bash
# E1: protocol/lease tests name the installed-instructions behavior
git grep -nE 'installed instructions|SKILL.md body|skill body' src/lib/contract src/zombied/fleet src/runner

# E2: runner unit/security lane
make test-unit-zigrunner

# E3: fleet integration lane covering lease issuance (DB-backed; Redis tests self-skip)
make test-integration-db

# E4: Zig lint and cross-compile
make lint-zig
zig build -Dtarget=x86_64-linux
zig build -Dtarget=aarch64-linux

# E5: architecture update present
git grep -n "instructions" docs/architecture/data_flow.md docs/architecture/runner_fleet.md docs/architecture/scenarios/01_default_install.md

# E6: secret scan
gitleaks detect
```

---

## Dead Code Sweep

**1. Orphaned files — none expected.**

| File to delete | Verify |
|----------------|--------|
| N/A — additive protocol and prompt repair | — |

**2. Orphaned references — none expected.** No symbol removal planned.

---

## Discovery (consult log)

- **Origin (Jun 08, 2026):** Indy asked whether `zombiectl install` / dashboard install actually makes NullClaw run the installed behavior markdown after a trigger, especially GitHub deploy failure → platform-ops diagnosis. Architecture docs say yes; code path did not prove it.
- **Architecture consult:** `docs/architecture/scenarios/01_default_install.md` says `zombiectl install --from` stores `source_markdown`, later webhook/steer use the same reasoning loop, and NullClaw runs the `SKILL.md` prose. `docs/architecture/runner_fleet.md` and `data_flow.md` describe the lease as event + `ExecutionPolicy`, but are silent on the exact field carrying instructions. This spec extends those docs and requires same-commit doc repair.
- **Code-grounded facts:** `load-skill-from-path.ts` reads `SKILL.md` and `TRIGGER.md`; CLI and UI send `{trigger_markdown, source_markdown}`; `create.zig` stores both; `ZombieSession.claimZombie` extracts `instructions`; `protocol.LeasePayload` currently lacks instructions; `child_exec.buildCallArgs` currently derives only event message/request JSON and passes `context = null`.
- **Concurrency finding:** current `zombie-runner` loop is serial (`heartbeat → lease → executeAndReport → report`); multi-agent concurrency today comes from multiple runner daemons/hosts. Postgres assignment is concurrent via `fleet.runner_affinity` claim/fencing and `fleet.runner_leases` reclaim/report fences. Redis request-path operations are pooled and per-command, and event reads use `XREADGROUP` only after the Postgres slot is claimed. The blocker for one runner process holding many leases is runner-side state ownership, especially memory hydrate/capture, which `runner_fleet.md` documents as safe because the loop is strictly serial.
- **Memory-plane reconcile (Jun 08 2026):** the earlier "blocker is memory hydrate/capture" framing was over-cautious. Code-grounded re-check for the M88_002 re-scope confirms the memory plane is already multi-lease-safe: hydrate keyed by `zombie_id` (runner `loop.zig`), capture fenced by `lease_id` + `fencing_token` server-side (`src/zombied/http/handlers/runner/memory.zig`), idempotent `ON CONFLICT (key, zombie_id)`, durable store in `zombied` Postgres. M84_005's Jun 06 decision already removed the one-live-lease dependency. The true multi-lease gate is the daemon loop split (M88_002), and memory isolation rests on the per-zombie affinity-slot single-holder claim + capture-time fencing — NOT `zombie_id` isolation alone, and NOT a unique constraint on `fleet.runner_leases`. This spec's `instructions` field is a multi-lease **enabler**, not a serial-only change.
- **Adversarial review fixes (Jun 09 2026):** a pre-implementation skeptic pass against `main` corrected five things now baked into the spec: (1) **BLOCKER** — the mixed-version Failure Mode was inverted; `control_plane_client.zig` parses `LeaseResponse` with `std.json` *default* options (no `ignore_unknown_fields`), so an **old** runner getting a new `instructions` field hard-fails `error.UnknownField` → `MalformedResponse` → **no work** (loud, not silent generic); the field is additive+`= ""` defaulted (new-runner/old-`zombied` parses cleanly) and **rollout is runners-first**. (2) `instructions: []const u8 = ""` is explicitly defaulted (§1.1 tests the absent→`""` path). (3) prompt assembly extends `composeMessage`'s existing allowlist (6th key) rather than a new path, so §3's secret-non-injection allowlist test keeps covering it. (4) empty-`SKILL.md` behavior is pinned concretely (explicit `(no installed instructions)` sentinel + warning), not deferred to PLAN. (5) `make test-integration-runner` does not exist → corrected to `make test-integration-db`.
- **Deferrals:** none. Any future claim that mixed-version rollout, empty-body validation, or doc repair is deferred needs an Indy-acked verbatim quote here.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits lease/prompt/secret-boundary coverage against this Test Specification. | Clean; iteration count recorded in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs runner architecture, `dispatch/write_zig.md`, secret boundary, and Failure Modes. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open Pull Request (PR) against the immutable diff. | Comments addressed before human review/merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Runner unit/security | `make test-unit-zigrunner` | to be filled during VERIFY | |
| Fleet integration | `make test-integration-db` | to be filled during VERIFY | |
| Lint | `make lint-zig` | to be filled during VERIFY | |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | to be filled during VERIFY | |
| Gitleaks | `gitleaks detect` | to be filled during VERIFY | |
| Architecture grep | `git grep -n "instructions" docs/architecture/data_flow.md docs/architecture/runner_fleet.md docs/architecture/scenarios/01_default_install.md` | to be filled during VERIFY | |

---

## Out of Scope

- Changing install surfaces: CLI and UI already submit the required markdown bodies.
- Treating `INSTALL.md` as runtime input. Install instructions are for the local coding agent; `SKILL.md` is the runtime behavior source.
- Expanding sandbox depth, egress proxying, or cap-drop work from M84_004/M84_006.
- Making one `zombie-runner` process lease/supervise multiple children concurrently. That is M88_002 (the runner worker-thread pool); this spec only repairs what each serial lease receives. The runner memory plane is already multi-lease-safe (see Concurrency posture), so M88_002 carries no memory-routing prerequisite.
- Adding new credential storage or resolving secrets in prompt assembly.
- Rewriting NullClaw's core agent API beyond the minimal context/message assembly needed for this bug.
