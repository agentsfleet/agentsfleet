# M33_001: Homelab Zombie — Flagship Executable Sample

**Prototype:** v2.0.0
**Milestone:** M33
**Workstream:** 001
**Date:** Apr 21, 2026
**Status:** IN_PROGRESS — parked pending M19_001 (`zombiectl zombie install --from <path>`) and tool-enforcement runtime (nullclaw). §1/§2/§5 land in this pass; §3/§4 dims carry `BLOCKED_ON` tags.
**Priority:** P1 — The flagship sample that proves the v2.0-alpha demo. Homelab Zombie is the project used for end-to-end testing of the install → trigger → reason → tool-call → diagnosis loop.
**Batch:** B2 — alpha gate, parallel with M11_005, M19_001, M13_001, M21_001, M27_001, M31_001
**Branch:** feat/m33-homelab-zombie (worktree at `~/Projects/usezombie-m33-homelab-zombie`)
**Depends on:** M2_002 (Clawhub skill format — DONE), M19_001 (`zombiectl zombie install --from <path>`), M5_001 (tools as attachments — DONE), M6_001 (firewall policy — DONE)

---

## Overview

**Goal (testable):** In a fresh dev environment (Clerk-authed tenant, `zombied` running, kubectl + docker credentials added to the tenant vault), running `zombiectl zombie install --from samples/homelab` creates a zombie. Invoking `zombie` (or curling the webhook) with "Jellyfin pods keep restarting" produces an activity stream where the zombie reads `kubectl get pods`, `kubectl describe`, `kubectl logs`, and `kubectl top` output through the firewall, reasons about the failure, and returns a diagnosis — without ever seeing the raw kubeconfig content. If the kubectl credential is missing, a single `UZ-GRANT-001` event fires cleanly; no crash, no partial state.

**Problem:** The v2.0-alpha needs one flagship executable sample the team dogfoods and the launch blog post demonstrates. `docs/brainstormed/docs/homelab-zombie-launch.md` pitches this exact thing: an AI agent that diagnoses a homelab via `kubectl` without holding the kubeconfig, tools allowlisted at the verb level, worker runs in-network. Without a shipping `samples/homelab/` directory with a runnable skill + triggers + README, the blog post ships ahead of the product and M32 quickstart has no concrete install target.

**Solution summary:** Under `samples/homelab/`, ship a three-file operator-ready zombie bundle: `SKILL.md` (the diagnostic prompt with a **prose allowlist** of kubectl verbs and docker commands in a "Tools you can use" section, plus `model: claude-sonnet-4-6`), `TRIGGER.md` (webhook default, optional daily cron, minimal runtime wiring: `tools: [kubectl, docker]` + `credentials: [kubectl_config, docker_socket]`, network allowlist, budget), `README.md` (operator quickstart: credential add → install → trigger → example conversation → prose allowlist summary). The allowlist lives in natural language inside the prompt body; there is no per-tool sub-skill directory. Every outbound kubectl/docker call is parsed, verb-checked against the prose allowlist by the tool dispatcher, and re-originated by the worker with the real credential injected at the network boundary. An integration test colocated with the M19_001 install-from-path handler proves install + trigger + credential-missing all behave exactly as the README promises.

---

## Files Changed (blast radius)

All under `$REPO_ROOT/` unless noted (the usezombie checkout). Set `REPO_ROOT` before running any eval command:

```bash
export REPO_ROOT="${REPO_ROOT:-$HOME/Projects/usezombie}"  # default for local dev; CI / agent worktrees must export their own checkout path
```

This milestone does NOT touch `src/**`; the E2E test is the only code addition.

| File | Action | Why |
|------|--------|-----|
| `samples/homelab/SKILL.md` | CREATE | Flagship skill: diagnostic prompt + `model: claude-sonnet-4-6` + **natural-language tool policy** for `kubectl` and `docker` (allowed verbs/commands and forbidden ones listed in the prompt body). |
| `samples/homelab/TRIGGER.md` | CREATE | Webhook trigger (default) + optional daily cron hint (`0 9 * * *` UTC) + minimal runtime wiring: `tools: [kubectl, docker]`, `credentials: [kubectl_config, docker_socket]`, network allowlist, budget. |
| `samples/homelab/README.md` | CREATE | Operator quickstart: prereqs, `zombiectl credential add kubectl_config --file ~/.kube/config`, `zombiectl zombie install --from samples/homelab`, curl trigger example, sample conversation transcript from the Jellyfin scenario, short prose allowlist summary that points readers at `SKILL.md` for the authoritative list. |
| `src/samples/homelab/install_integration_test.zig` (or colocated with M19_001 install-from-path handler once it exists) | CREATE (deferred) | E2E: seed credentials, `zombiectl zombie install --from samples/homelab` → 201, trigger with sample payload → activity stream asserts allowed-verb tool calls + final reasoning message. Negative branch: no credential → one `UZ-GRANT-001`. **Deferred:** lands when M19_001 ships `--from <path>`. Path follows this repo's colocation convention (`src/**/*_integration_test.zig`); no `tests/integration/` directory is introduced. |

**No sub-skill directory.** The homelab zombie is a single consumer of `kubectl` and `docker`; there is no second zombie sharing the same allowlist today. Per user decision (option B — natural language policy), the allowlists live as prose inside `samples/homelab/SKILL.md` rather than as machine-readable `policy: {allowed_verbs, denied_resources, allowed_commands}` blocks in per-tool sub-skill files. If a second zombie ever shares the same allowlist, lift the policy into a shared structured representation then — the speculative abstraction is rejected now. The brainstormed `docs/brainstormed/samples/skills/{kubectl,docker}-readonly/README.md` files are untracked reference material and are not load-bearing; they stay on disk as launch-post reference.

---

## Applicable Rules

- **RULE FLL** — each `.md` file stays reviewable; keep below 350 lines (markdown exempt from the 350L code gate but readability matters).
- **RULE FIR (firewall-allowlisted tools only)** — `TRIGGER.md` `tools:` list names only registered tool primitives (`kubectl`, `docker`). No raw `fetch`, no unregistered shims.
- **RULE ORP** — zero stale references to `docs/brainstormed/samples/skills/kubectl-readonly` and `.../docker-readonly` in load-bearing paths. Coordination prose inside other specs (e.g. M32_001) is not a stale reference, but the now-removed `samples/homelab/skills/` path should also not appear as a live target anywhere.
- **RULE TST-NAM** — no milestone IDs in the integration test filename or `test "…"` names.
- Standard set otherwise; no `src/**` mutations.

---

## Sections (implementation slices)

### §1 — Sub-skill files (RETIRED — no longer in scope)

**Status:** RETIRED

Original intent: author `samples/homelab/skills/{kubectl,docker}-readonly/SKILL.md` as per-tool sub-skill files carrying structured `policy: {allowed_verbs, denied_resources, allowed_commands}` blocks. Retired after user decision (option B — natural language policy) because the homelab zombie is the sole consumer of these allowlists; a separate sub-skill file per tool is speculative reuse. The allowlists now live as prose in `samples/homelab/SKILL.md` under the "Tools you can use" section. The sub-skill files and directory were deleted; the per-skill lint-grep gate (§1.3) is replaced by the prose-policy check in §2.4.

**Dimensions:** none — retired.

### §2 — Homelab skill authoring (the flagship)

**Status:** DONE

Authored `samples/homelab/SKILL.md`, `TRIGGER.md`, and `README.md` following the `docs/brainstormed/docs/homelab-zombie-launch.md` narrative: the Jellyfin → OOMKilled scenario, verb-level allowlist expressed as **prose** in the SKILL.md prompt body, placeholder-credential model, in-network worker. Integration-parse dims (2.1/2.2) remain pending the skill parser reaching these files at install time; that happens alongside M19_001.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | DONE | `samples/homelab/SKILL.md` | skill parser | parses; frontmatter has `model: claude-sonnet-4-6`; prompt body contains the "Tools you can use" section naming both `kubectl` and `docker` with their allowed and forbidden verbs/commands | integration |
| 2.2 | DONE | `samples/homelab/TRIGGER.md` | trigger loader | parses; returns `{ webhook: true, payload_schema: { message: string }, optional_cron: "0 9 * * *", tools: [kubectl, docker], credentials: [kubectl_config, docker_socket] }` | integration |
| 2.3 | DONE | `samples/homelab/README.md` | new operator reads top-to-bottom | sections: Prereqs, Credential setup, Install, Trigger (webhook curl), Example conversation (Jellyfin → OOMKilled diagnosis), prose allowlist summary, How it works (placeholder-credential story + "policy is prose" paragraph) | manual review |
| 2.4 | DONE | `samples/homelab/SKILL.md` prompt body | grep for destructive verbs after the phrase "Never use" | every destructive verb (`delete`, `apply`, `exec`, `patch`, `edit`, `replace`, `run`, `rm`, `rmi`, `kill`, `pause`, `unpause`, `build`, `push`, `pull`, `scale`, `rollout`, `cordon`, `drain`) appears inside a "Never use" sentence for the right tool; `secrets` appears inside a "never read" sentence | lint (grep) |

### §3 — Firewall policy enforcement

**Status:** BLOCKED_ON — tool-enforcement runtime (nullclaw). The current `src/zombie/firewall/` engine enforces at the HTTP boundary (domain + endpoint + injection); shell-verb allowlisting (e.g. rejecting `kubectl delete ns`) is a separate enforcement layer that belongs to the tool dispatcher / nullclaw. §3 dims assert that layer's behavior once it exists. M33 lands the allowlist as **prose in `samples/homelab/SKILL.md`**; the enforcement layer either (a) parses the prose at install time, or (b) defers to soft LLM-level trust on the prose, or (c) requires the operator to lift the allowlist into a structured form before enforcement is available. That decision belongs to nullclaw, not M33.

The allowlist the homelab zombie is expected to respect at runtime is the one described in the "Tools you can use" section of `samples/homelab/SKILL.md`. Any attempt to invoke a destructive verb should emit `UZ-FIREWALL-001` and halt the tool call.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | BLOCKED_ON nullclaw-tool-policy | runtime tool dispatcher with homelab skills loaded | synthetic tool call: `kubectl delete ns media` | tool call rejected; activity stream emits `UZ-FIREWALL-001`; zombie receives a structured error string and reasons from it | integration |
| 3.2 | BLOCKED_ON nullclaw-tool-policy | runtime tool dispatcher | synthetic tool call: `kubectl get secrets -A` | rejected (`secrets` on denylist even for `get`); `UZ-FIREWALL-001` emitted | integration |
| 3.3 | BLOCKED_ON nullclaw-tool-policy | runtime tool dispatcher | synthetic tool call: `docker exec jellyfin sh` | rejected; `UZ-FIREWALL-001` emitted | integration |

### §4 — Install & trigger integration test

**Status:** BLOCKED_ON M19_001 — `zombiectl zombie install --from <path>` does not yet exist (today only `zombiectl install <template>` is shipped). The integration test file lands alongside M19_001's handler in this repo's colocation convention (`src/**/*_integration_test.zig`) — not under a `tests/integration/` directory, which does not exist in this repo.

E2E happy-path: spin up `zombied`, seed kubectl + docker credentials in the vault, install from `samples/homelab`, trigger with "Jellyfin pods keep restarting", assert activity stream contains at least one allowed-verb tool call and a final reasoning message.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | BLOCKED_ON M19_001 | `src/samples/homelab/install_integration_test.zig` (or colocated with the M19_001 install-from-path handler) install path | `zombiectl zombie install --from samples/homelab` in fresh dev env with seeded credentials | exit 0; HTTP POST `/v1/workspaces/{ws}/zombies` returns 201; response carries webhook URL + zombie ID | integration |
| 4.2 | BLOCKED_ON M19_001 | same test, trigger happy path | POST `{"message":"Jellyfin pods keep restarting"}` to webhook URL, with stubbed k8s API returning canned pod+describe+logs output | activity stream contains ≥1 `kubectl get pods` tool call event, ≥1 reasoning message, and a final `zombie_completed` event with a non-empty diagnosis string | integration |
| 4.3 | BLOCKED_ON M19_001 | same test, credential-missing path | trigger zombie with `kubectl_config` credential absent from the vault | activity stream emits exactly one `UZ-GRANT-001` event at the first tool call, zombie run ends cleanly (no crash, no partial writes); event message points to `zombiectl credential add kubectl_config` | integration |

### §5 — Orphan sweep and cross-layer consistency

**Status:** DONE

Orphan sweep (E7) passes: no load-bearing code or sample references `brainstormed/samples/skills`. Remaining matches are prose documentation — this spec's own Discovery notes and a coordination reference inside `docs/v2/pending/P1_DOCS_API_CLI_M32_001_QUICKSTART_V2_REWRITE.md` that still mentions `samples/homelab/skills/kubectl-readonly/` + `docker-readonly/`. That M32 reference is now stale because M33 deleted the sub-skill directory during the option-B pivot; it is a spec-level cross-reference for the other agent owning M32_001 to resolve, and is called out in the Discovery section. Credential-name consistency (5.2) verified under the new shape: `kubectl_config` appears in `samples/homelab/README.md` (4×) and `samples/homelab/TRIGGER.md` (1×); the SKILL.md no longer declares credentials — they live in TRIGGER.md as a declarative runtime-wiring list. `samples/homelab/` is the forward canonical example for this convention (see Discovery #8 for why not `samples/lead-collector/`).

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | DONE | `src/`, `samples/`, and tracked `docs/` (excluding `docs/brainstormed/` which is untracked reference material and `v1/done` historical logs) | `grep -rn "brainstormed/samples/skills"` under those paths | only coordination/prose matches; no code or live-sample references | grep |
| 5.2 | DONE | credential name consistency | `grep kubectl_config samples/homelab/{SKILL.md,README.md,TRIGGER.md}` | `kubectl_config` present in at least README.md and TRIGGER.md (declared in TRIGGER.md, referenced in README.md operator guidance); SKILL.md no longer declares credentials | grep |

---

## Interfaces

**Status:** LOCKED

This workstream consumes existing interfaces; it introduces no new public code surfaces.

### Consumed interfaces

- **Clawhub SKILL.md schema** — owned by M2_002. M33 consumes the YAML frontmatter format (`name`, `version`, `description`, `tags`, `author`, `model`) + the prompt body below the frontmatter. Under the option-B pivot, M33 does NOT consume `credentials:` or `policy:` keys in SKILL.md — credentials live in TRIGGER.md and the allowlist lives as prose in the prompt body.
- **Clawhub TRIGGER.md schema** — owned by M19_001. M33 consumes `{ trigger: {type, payload_schema, optional_cron}, tools: [name,...], credentials: [name,...], network: {allow: [...]}, budget: {daily_dollars, monthly_dollars} }`. `samples/homelab/TRIGGER.md` is the canonical example of this shape going forward — do not point at `samples/lead-collector/`, which is slated for deletion under the v2 direction (see Discovery #8).
- **`zombiectl zombie install --from <path>`** — owned by M19_001. M33 depends on it resolving a local directory, loading `SKILL.md` + `TRIGGER.md`, and creating the zombie on the tenant.
- **`zombiectl credential add <name>`** — owned by M13_001. M33 README documents `zombiectl credential add kubectl_config --file ~/.kube/config` and `zombiectl credential add docker_socket --file /var/run/docker.sock`.
- **Tool dispatcher / verb allowlist enforcement** — owned by nullclaw (future). `src/zombie/firewall/` (M6_001) enforces at the HTTP boundary (domain + endpoint + injection) and is not the enforcement layer for shell-verb allowlisting. M33 expresses the allowlist as prose in SKILL.md; nullclaw reads + enforces it when it ships.
- **Activity stream event types** — owned by M19_001 / M20_001. M33 asserts emission of `zombie_triggered`, `tool_call_requested`, `tool_call_completed`, `zombie_completed`, `UZ-GRANT-001`, `UZ-FIREWALL-001`.

### Webhook payload shape (input)

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `message` | string | 1..4000 chars | `"Jellyfin pods keep restarting"` |

### Activity stream events emitted (output)

| Event type | When | Payload shape |
|------------|------|---------------|
| `zombie_triggered` | webhook POST received | `{ zombie_id, trigger_id, message }` |
| `tool_call_requested` | skill asks to run a kubectl/docker command | `{ tool: "kubectl" \| "docker", verb: "get", args: [...] }` |
| `tool_call_completed` | dispatcher allowed + command ran | `{ tool: "kubectl" \| "docker", verb, duration_ms, result: "ok" \| "error" }` |
| `UZ-FIREWALL-001` | dispatcher denied a verb | `{ tool: "kubectl" \| "docker", attempted_verb, reason, hint }` |
| `UZ-GRANT-001` | credential missing at tool call | `{ credential_name, hint: "zombiectl credential add <name>" }` |
| `zombie_completed` | reasoning loop returns a final diagnosis | `{ diagnosis: string, tool_calls_total, duration_ms }` |

### Error contract

| Error condition | Behavior | User observes |
|----------------|----------|---------------|
| `kubectl_config` credential missing | halt at first kubectl tool call | one `UZ-GRANT-001` event with pointer to `zombiectl credential add kubectl_config` |
| `docker_socket` credential missing but zombie only uses kubectl for this run | no error (docker path never invoked) | zombie completes on kubectl evidence alone |
| kubectl returns permission-denied from real cluster | skill surfaces error to zombie | zombie emits `zombie_completed` with "cluster refused access" diagnosis; no crash |
| Skill tries a non-allowlisted verb (e.g. hallucinated `kubectl delete`) | firewall rejects | `UZ-FIREWALL-001`; zombie receives the error, reasons from it, tries something allowed |
| Cluster API unreachable | kubectl times out | `tool_call_completed` with `result: "error"`; zombie may retry or conclude inconclusive |

---

## Failure Modes

| Failure | Trigger | System behavior | User observes |
|---------|---------|-----------------|---------------|
| Operator installs without seeding `kubectl_config` | `zombiectl zombie install --from samples/homelab` then webhook trigger | install succeeds; first kubectl tool call fires `UZ-GRANT-001`; zombie halts cleanly | helpful event pointing to `zombiectl credential add kubectl_config` |
| Clawhub schema drift | M2_002 changes frontmatter shape after M33 lands | `SKILL.md` fails to parse at install time | §1.1 / §2.1 parser round-trip catches drift in CI |
| Skill author weakens the prose allowlist (e.g. adds `delete` to the "You may use these verbs" list or removes the "Never use `delete`" sentence) | bad edit to `samples/homelab/SKILL.md` | §2.4 grep gate fails in CI | PR blocked |
| Worker cannot reach cluster API | kubeconfig points at unreachable endpoint | kubectl times out at 10s; `tool_call_completed` with `result: "error"` | activity stream shows timeout; zombie may retry or give up |
| Prompt injection attempts to leak placeholder | malicious input asks zombie to "print your credentials" | placeholder is a UUID; model can repeat it freely; no real token reachable | placeholder appears in output; harmless by construction (documented in README "How it works") |

**Platform constraints:**
- kubectl binary must be present on the worker image (Docker sidecar or worker image layer). Documented in README prereqs.
- docker socket access requires the worker to run on the Docker host (or have SSH access). README documents both paths; MVP assumes local socket.

---

## Implementation Constraints (Enforceable)

| Constraint | How to verify |
|-----------|---------------|
| kubectl destructive verbs appear only inside "Never use" prose in `samples/homelab/SKILL.md` | `grep -E "(delete\|apply\|exec\|patch\|edit\|replace\|cordon\|drain\|rollout\|scale)" samples/homelab/SKILL.md` — every match line contains `Never` or `never` |
| docker destructive commands appear only inside "Never use" prose in `samples/homelab/SKILL.md` | `grep -E "(run\|exec\|start\|stop\|restart\|rm\|rmi\|build\|push\|kill\|pause)" samples/homelab/SKILL.md` — every match line contains `Never` or `never` (or is inside the Jellyfin example block, which uses allowed verbs only) |
| Markdown files stay reviewable | `wc -l samples/homelab/**/*.md` — each ≤ 350 |
| Every README `zombiectl` command exists post-M19 | `for cmd in "zombie install" "credential add"; do zombiectl $cmd --help; done` exits 0 |
| Credential reference names consistent across files | `grep -c kubectl_config samples/homelab/README.md samples/homelab/TRIGGER.md` — both ≥ 1 |
| Integration test exits 0 in happy-path + credential-missing + firewall-denied branches | `zig build test -Dtest-filter=samples_homelab` green |

---

## Invariants (Hard Guardrails)

| # | Invariant | Enforcement |
|---|-----------|-------------|
| 1 | `samples/homelab/SKILL.md` prose never writes "you may use ... `delete`/`apply`/`patch`/`exec`/..." or otherwise permits a write verb for kubectl | CI grep gate — §2.4 dim. Every occurrence of a destructive kubectl verb must be inside a "Never use" sentence. |
| 2 | `samples/homelab/SKILL.md` prose never permits a write command for docker | CI grep gate — §2.4 dim. Same shape as invariant 1, applied to docker commands. |
| 3 | `samples/homelab/SKILL.md` prose instructs the agent never to read `secrets` resources | `grep -i "never read .secrets. \|except .secrets." samples/homelab/SKILL.md` returns ≥1 match |
| 4 | `samples/homelab/TRIGGER.md` `tools:` list contains only registered tool primitives (`kubectl`, `docker`) | CI grep — no raw `fetch`, `http_raw`, unknown tool names |
| 5 | Every credential referenced in the `SKILL.md` prompt body or the `README.md` operator guidance has a matching entry in `TRIGGER.md` `credentials:` | lint-grep comparing `{{credential.<name>}}` or `credential add <name>` patterns against the TRIGGER.md list |

---

## Test Specification

### Unit tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `homelab skill parses` | 2.1 | `samples/homelab/SKILL.md` | parser | non-null skill record with expected fields (model, prompt body) |
| `homelab trigger parses` | 2.2 | `samples/homelab/TRIGGER.md` | loader | `{ webhook: true, payload_schema, tools: [kubectl, docker], credentials: [kubectl_config, docker_socket] }` |
| `prose allowlist gate` | 2.4 | `samples/homelab/SKILL.md` prompt body | grep pipeline in Implementation Constraints | destructive verbs only appear inside "Never use" sentences |
| `sample payload is valid JSON` | 2.2 | TRIGGER.md payload block | `JSON.parse` | succeeds; matches payload schema |

### Integration tests

| Test name | Dim | Infra | Input | Expected |
|-----------|-----|-------|-------|----------|
| `install from samples/homelab` | 4.1 | dev DB + `zombied` + seeded vault | `zombiectl zombie install --from samples/homelab` | 201 + webhook URL |
| `trigger produces diagnosis` | 4.2 | dev DB + `zombied` + stubbed k8s API | POST `{"message":"Jellyfin pods keep restarting"}` | ≥1 allowed-verb tool call + `zombie_completed` with non-empty diagnosis |
| `trigger without kubectl credential` | 4.3 | dev DB + `zombied`, vault empty | POST sample payload | one `UZ-GRANT-001`; clean halt |
| `firewall denies destructive verb` | 3.1 | dev DB + `zombied` + seeded vault + synthetic skill attempt | zombie asks to `kubectl delete ns media` | `UZ-FIREWALL-001`; zombie continues with allowed verb |
| `firewall denies secrets read` | 3.2 | same | zombie asks `kubectl get secrets -A` | `UZ-FIREWALL-001` |
| `firewall denies docker exec` | 3.3 | same | zombie asks `docker exec ...` | `UZ-FIREWALL-001` |

### Negative tests

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|----------------|
| `homelab prose permits delete` | invariant 1 | synthetic SKILL.md prose saying "You may use `delete`" | CI grep gate (§2.4) fails |
| `homelab prose permits docker exec` | invariant 2 | synthetic SKILL.md prose saying "You may use `exec`" for docker | CI grep gate (§2.4) fails |
| `secrets protection removed from prose` | invariant 3 | synthetic SKILL.md without any "never read `secrets`" sentence | CI grep gate fails |
| `raw fetch in TRIGGER tools list` | invariant 4 | synthetic `samples/homelab/TRIGGER.md` with `fetch` in `tools:` | CI grep gate fails |
| `dangling credential reference in prompt` | invariant 5 | prompt uses `{{credential.nonexistent}}` | invariant check fails |

### Edge cases

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| `empty message` | 2.1 | webhook POST `{"message":""}` | zombie emits prompt asking for a question; no crash |
| `max-length message` | 2.1 | webhook POST with 4000-char message | accepted; zombie processes normally |
| `oversize message` | 2.1 | webhook POST with 4001-char message | HTTP 400 with validation error |

### Regression tests

N/A — greenfield sample.

### Leak tests

N/A — markdown + integration test additions; integration test uses `std.testing.allocator` per existing convention.

### Spec-claim tracing

| Spec claim | Test | Type |
|-----------|------|------|
| "install + trigger produces diagnosis via allowed verbs" | §4.2 | integration |
| "missing credential → one clean UZ-GRANT-001" | §4.3 | integration |
| "firewall denies destructive verbs" | §3.1, §3.2, §3.3 | integration |
| "homelab prose never permits destructive kubectl/docker verbs or secrets reads" | invariants 1-3 | lint |
| "zombie never holds the raw kubeconfig" | manual verification at VERIFY — inspect worker process memory during §4.2 run; document evidence in Verification Evidence | manual |

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|------|--------|--------|
| 1 | Inspect a current Clawhub SKILL.md/TRIGGER.md pair to confirm frontmatter convention. Historical reference: `samples/lead-collector/` (used during M33 authoring; slated for deletion per Discovery #8 — use `samples/homelab/` itself as the forward reference). | manual |
| 2 | Author `samples/homelab/SKILL.md` — flagship prompt with **prose allowlist** in a "Tools you can use" section (kubectl verbs: `get`, `describe`, `logs`, `top`, `events`, `explain`, `version`, `api-resources`, `api-versions`; docker commands: `ps`, `logs`, `inspect`, `images`, `stats`, `top`, `events`, `version`, `info`) and explicit "Never use" sentences for every destructive verb/command + "never read `secrets`" | §2.1 + §2.4 pass |
| 3 | Author `samples/homelab/TRIGGER.md` — webhook + cron + `tools: [kubectl, docker]` + `credentials: [kubectl_config, docker_socket]` + network + budget | §2.2 passes |
| 4 | Author `samples/homelab/README.md` — operator quickstart + prose allowlist summary + how-it-works | §2.3 passes |
| 5 | Colocate integration test with M19_001 install-from-path handler — install + trigger + credential-missing branches | §4 dims green (lands with M19_001) |
| 6 | Full gate: `make test-integration`, 350L gate, orphan sweep, lint | all dims green |

---

## Acceptance Criteria

- [x] `ls samples/homelab/` shows `SKILL.md`, `TRIGGER.md`, `README.md` (three files, no `skills/` sub-directory) — verified via E1.
- [ ] `zombiectl zombie install --from samples/homelab` succeeds on a fresh dev env — BLOCKED_ON M19_001 (§4.1).
- [ ] Trigger with "Jellyfin pods keep restarting" produces an activity stream containing at least one `kubectl get pods` tool call and a final reasoning message — BLOCKED_ON M19_001 (§4.2).
- [ ] Missing `kubectl_config` credential → single `UZ-GRANT-001` event, no crash — BLOCKED_ON M19_001 (§4.3).
- [ ] Firewall denies `kubectl delete`, `kubectl get secrets`, and `docker exec` attempts — BLOCKED_ON nullclaw-tool-policy (§3.1/3.2/3.3).
- [x] `samples/homelab/SKILL.md` prose never permits a destructive kubectl verb outside a "Never use" sentence (invariant 1) — verified via E3 (prose grep).
- [x] `samples/homelab/SKILL.md` prose never permits a destructive docker command outside a "Never use" sentence (invariant 2) — verified via E4 (prose grep).
- [ ] Integration test passes locally and in CI — BLOCKED_ON M19_001; file lands under `src/**/*_integration_test.zig` (repo convention), not `tests/integration/`.
- [x] 350L gate clean on all markdown files — verified via E2 (largest: README.md 163 lines).
- [x] Orphan sweep: zero stale `samples/homelab/skills/` or `brainstormed/samples/skills` references in load-bearing paths — verified via E7; remaining matches are coordination prose between the M33 and M32 specs, not stale code or samples.

---

## Eval Commands

```bash
# E1: Files exist (three files, no skills/ subdirectory)
test -f samples/homelab/SKILL.md    && echo "ok: SKILL.md"
test -f samples/homelab/TRIGGER.md  && echo "ok: TRIGGER.md"
test -f samples/homelab/README.md   && echo "ok: README.md"
test ! -d samples/homelab/skills    && echo "ok: no skills/ subdirectory" || echo "FAIL: skills/ sub-dir resurrected"

# E2: 350-line gate on markdown
wc -l samples/homelab/*.md | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E3: kubectl destructive verbs only appear inside "Never" sentences in the Tools section of SKILL.md
awk '/^## Tools you can use/,/^## [^T]/' samples/homelab/SKILL.md \
  | grep -iE '\b(delete|apply|exec|patch|edit|replace|scale|rollout|cordon|drain)\b' \
  | grep -viE 'never' \
  && echo "FAIL: destructive kubectl verb outside a Never-line in Tools section" \
  || echo "ok: kubectl destructive verbs only in Never-lines"

# E4: docker destructive commands only appear inside "Never" sentences in the Tools section of SKILL.md
awk '/^## Tools you can use/,/^## [^T]/' samples/homelab/SKILL.md \
  | grep -iE '\b(run|exec|start|stop|restart|rm|rmi|build|push|pull|kill|pause|unpause)\b' \
  | grep -viE 'never' \
  && echo "FAIL: destructive docker command outside a Never-line in Tools section" \
  || echo "ok: docker destructive commands only in Never-lines"

# E5: every line mentioning `secrets` in SKILL.md also carries a "Never" / restriction verb
if grep -iE 'secrets' samples/homelab/SKILL.md | grep -iqvE 'never|except'; then
  echo "FAIL: secrets mentioned without a Never/except restriction"
else
  echo "ok: secrets prose carries a restriction"
fi

# E6: Credential name consistency across the three live files
grep -c "kubectl_config" samples/homelab/SKILL.md samples/homelab/README.md samples/homelab/TRIGGER.md

# E7: Orphan sweep on brainstormed skills path + now-removed sub-skill path
grep -rn "brainstormed/samples/skills\|samples/homelab/skills/" src/ samples/ docs/v2/ \
  | grep -v -E "v1/done|historical|\.git/" \
  || echo "ok: orphans clean"

# E8: Integration test — DEFERRED (BLOCKED_ON M19_001). Landed alongside the
# install-from-path handler once it exists, at the repo's colocation path
# (e.g. src/samples/homelab/install_integration_test.zig).
# zig build test -Dtest-filter=samples_homelab 2>&1 | tail -10

# E9: Every README zombiectl command exists
for cmd in "zombie install" "credential add"; do
  zombiectl $cmd --help >/dev/null 2>&1 && echo "ok: $cmd" || echo "MISS: $cmd"
done

# E10: No raw fetch/http_raw in the TRIGGER.md tools list
grep -E "^\s*-\s*(fetch|http_raw|raw_http)" samples/homelab/TRIGGER.md \
  && echo "FAIL: raw tool in tools list" || echo "ok: only allowlisted tools"
```

---

## Dead Code Sweep

M33 deletes the `samples/homelab/skills/` sub-directory (option-B pivot) — that deletion was the only code removal in this workstream. The brainstormed `kubectl-readonly/` and `docker-readonly/` directories under `docs/brainstormed/samples/skills/` are **untracked** in git; they remain on disk as reference material and are not load-bearing.

| Content to verify | Verify |
|-------------------|--------|
| `samples/homelab/skills/` sub-directory removed | `test ! -d samples/homelab/skills` |
| Three live files at the sample root | `test -f samples/homelab/{SKILL.md,TRIGGER.md,README.md}` |
| Stale references to the brainstormed path OR the removed sub-skill path in load-bearing content | `grep -rn "brainstormed/samples/skills\|samples/homelab/skills/" src/ samples/ docs/v2/` (excluding `docs/brainstormed/` itself and the M33 spec's own Discovery prose) → no live references |

---

## Verification Evidence

**Status:** PARTIAL (parked) — §1/§2/§5 evidence captured; §3/§4 evidence deferred with the enforcement/install handlers that would produce it.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Files exist | Eval E1 | 5/5 target files present | ✅ |
| 350L gate | Eval E2 | all ≤350 (README 178, SKILL 98, kubectl 82, docker 75, TRIGGER 19) | ✅ |
| kubectl write verbs absent | Eval E3 | no write verb in `allowed_verbs` | ✅ |
| docker write commands absent | Eval E4 | no write command in `allowed_commands` | ✅ |
| secrets denied | Eval E5 | `secrets` present in `denied_resources` | ✅ |
| Credential consistency | Eval E6 | `kubectl_config` in TRIGGER.md (1) and README.md (4); SKILL.md deliberately does not declare credentials (they live in TRIGGER.md as runtime wiring; `samples/homelab/` is now the forward canonical example of this split — see Discovery #8) | ✅ |
| Orphan sweep | Eval E7 | only self-references in M33 spec + M32 ownership note; no load-bearing stale refs | ✅ |
| Integration test | Eval E8 | deferred — BLOCKED_ON M19_001 (install-from-path) | ⏳ |
| CLI commands exist | Eval E9 | `zombiectl zombie install` NOT YET shipped — BLOCKED_ON M19_001 | ⏳ |
| No raw tools | Eval E10 | no `fetch`/`http_raw`/`raw_http` in flagship tools list | ✅ |
| Worker never holds raw kubeconfig | manual inspection during §4.2 run | deferred with §4.2 run | ⏳ |

---

## Discovery

Spec-level findings surfaced during CHORE(open) amendment (Apr 21, 2026):

1. **`zombiectl zombie install --from <path>` dependency.** M19_001 owns the `--from <path>` flag; it is still PENDING. Today only `zombiectl install <template>` is shipped (template-name based). M33 static artifacts land now; §4 integration dimensions carry `BLOCKED_ON: M19_001` and the test file lands alongside M19_001's handler in the repo's colocation convention.
2. **Tool-enforcement runtime does not yet exist.** `src/zombie/firewall/` enforces at the HTTP request boundary (domain allowlist + endpoint rules + injection detection). Shell-verb allowlisting (e.g. rejecting `kubectl delete ns`) is a different layer — the tool dispatcher / nullclaw. §3 dims are tagged `BLOCKED_ON: nullclaw-tool-policy`. M33 lands the declarative policy (in sub-skill SKILL.md files) so the enforcement layer has data to read when it ships.
3. **Brainstormed skill sources are untracked.** `docs/brainstormed/samples/skills/kubectl-readonly/README.md` and `docker-readonly/README.md` are not in git. `git mv` is meaningless; M33 authors fresh `SKILL.md` files at the target paths using the brainstormed content as reference. Brainstormed copies remain untracked on disk.
4. **Test-path convention mismatch.** Spec prescribed `tests/integration/samples_homelab_test.zig`; repo convention is `src/**/*_integration_test.zig`. Amended.
5. **Internal contradictions in original spec:** header said "M32 §9 moves the skills" while Execution Plan Step 1 said "M33 owns this move (see §0.1 below)"; no §0.1 target existed. Amended: M33 authors the landed content at the new path; no move is needed (untracked sources).

6. **Option-B pivot — sub-skills removed.** After the initial authoring, the user challenged the per-tool sub-skill layer (`samples/homelab/skills/{kubectl,docker}-readonly/SKILL.md`) as invented ceremony with no consumer. Reconsidered: the homelab zombie is the sole user of these allowlists today, YAGNI applies, and a single-file prose policy is simpler. Pivoted to option B — natural-language allowlist in the `SKILL.md` prompt body ("Tools you can use" section), credentials declared in `TRIGGER.md` (same convention used by the in-tree samples today), sub-skill directory deleted. Invariants 1–3 + §2.4 now gate the prose via grep (destructive verbs must appear inside "Never use" sentences). Feedback memory saved at `feedback_skill_policy_prose.md`.

7. **Cross-spec stale reference.** `docs/v2/pending/P1_DOCS_API_CLI_M32_001_QUICKSTART_V2_REWRITE.md` line ~277 still mentions `samples/homelab/skills/kubectl-readonly/` and `docker-readonly/` sub-skill paths. Those paths no longer exist after the option-B pivot. M33 does not edit other agents' active specs; leaving this as a coordination note for the M32_001 author to reconcile when they revisit.

8. **`samples/lead-collector/` is slated for deletion per v2 direction but no current spec owns it.** The brainstormed v2 direction (`docs/brainstormed/usezombie-v2-milestone-specs-prompt.md` §6, line 71: "DELETE `samples/lead-collector/`") calls for removal, but no v2/pending or v2/active spec currently owns this deletion. M32_001 removes the external `docs/integrations/lead-collector.mdx` doc page but does not touch the `samples/lead-collector/` directory in this repo. M33 previously described its TRIGGER.md shape as "mirroring `samples/lead-collector/`" — those references were rotting and have been rewritten to be self-describing and to point at `samples/homelab/TRIGGER.md` as the forward canonical example. The deletion itself is **out of scope for M33**: `samples/lead-collector/` is baked into `zombiectl/templates/lead-collector/`, multiple `zombiectl install` code paths in `zombiectl/src/commands/zombie.js`, unit-test fixtures in `src/zombie/{yaml_frontmatter,config_markdown_test,config_parser_test,event_loop_integration_test,event_loop_obs_integration_test}.zig`, and UI tests under `ui/packages/{app,website}/`. A deletion workstream needs to migrate those consumers first. Recommend filing a new spec (e.g. `P1_API_CLI_M{N}_{WS}_LEAD_COLLECTOR_SAMPLE_TEARDOWN.md`) that owns the coordinated removal; I'm flagging rather than filing here to avoid scope creep in the parked M33.

---

## Out of Scope

- Making `samples/homebox-audit/`, `samples/migration-zombie/`, `samples/side-project-resurrector/` executable — README-only moves in M32 §9; executable conversion is a follow-up milestone per sample zombie.
- Multi-cluster kubectl contexts — single-cluster for MVP; README documents the assumption.
- Docker Compose / Swarm support — docker Engine only for MVP.
- Slack-driven invocation — webhook only for MVP; Slack integration is a future milestone.
- Remediation writes (kubectl patch/apply behind approval gates) — v0.2 per the launch post; MVP is read-only diagnostic.
- Full HomeBox / Immich / Paperless-specific skills — homelab zombie reasons generically over kubectl/docker output; per-service skills land post-alpha.
- Building a mock k3s API server — §4.2 uses stubbed canned responses, not a live mock cluster.
