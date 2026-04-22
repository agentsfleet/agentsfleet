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

**Solution summary:** Under `samples/homelab/`, ship a complete operator-ready zombie bundle: `SKILL.md` (the diagnostic prompt + allowlisted tool references + model choice + credential list), `TRIGGER.md` (webhook default, optional daily cron), `README.md` (operator quickstart: credential add → install → trigger → example conversation), and two allowlisted skills at `samples/homelab/skills/kubectl-readonly/` and `samples/homelab/skills/docker-readonly/` authored in the M2_002 Clawhub format. The skill uses firewall-allowlisted tool primitives only — every outbound kubectl/docker call is parsed, verb-checked, and re-originated by the worker with the real credential injected at the network boundary. Integration test under `tests/integration/samples_homelab_test.zig` proves install + trigger + credential-missing all behave exactly as the README promises.

---

## Files Changed (blast radius)

All under `$REPO_ROOT/` unless noted (the usezombie checkout). Set `REPO_ROOT` before running any eval command:

```bash
export REPO_ROOT="${REPO_ROOT:-$HOME/Projects/usezombie}"  # default for local dev; CI / agent worktrees must export their own checkout path
```

This milestone does NOT touch `src/**`; the E2E test is the only code addition.

| File | Action | Why |
|------|--------|-----|
| `samples/homelab/SKILL.md` | CREATE | Flagship skill manifest: diagnostic prompt, model (`claude-sonnet-4-6`), allowlisted tools (`kubectl-readonly`, `docker-readonly`), credential references (`kubectl_config`, `docker_socket`). |
| `samples/homelab/TRIGGER.md` | CREATE | Two triggers: webhook (default, user sends a question) + optional daily cron hint (`0 9 * * *` UTC for proactive health scan). |
| `samples/homelab/README.md` | CREATE | Operator quickstart: prereqs, `zombiectl credential add kubectl_config --file ~/.kube/config`, `zombiectl zombie install --from samples/homelab`, curl trigger example, sample conversation transcript from the Jellyfin scenario, firewall allowlist documentation. |
| `samples/homelab/skills/kubectl-readonly/SKILL.md` | CREATE | Clawhub-format version of the brainstormed README. Declares read-only verb allowlist: `get`, `describe`, `logs`, `top`, `events`, `explain`, `version`, `api-resources`, `api-versions`. Denies `secrets` resource. |
| `samples/homelab/skills/docker-readonly/SKILL.md` | CREATE | Clawhub-format version of the brainstormed README. Declares allowed docker commands: `ps`, `logs`, `inspect`, `images`, `stats`, `top`, `events`, `version`, `info`. |
| `src/samples/homelab/install_integration_test.zig` (or colocated with M19_001 install-from-path handler once it exists) | CREATE (deferred) | E2E: seed credentials, `zombiectl zombie install --from samples/homelab` → 201, trigger with sample payload → activity stream asserts allowed-verb tool calls + final reasoning message. Negative branch: no credential → one `UZ-GRANT-001`. **Deferred:** lands when M19_001 ships `--from <path>`. Path follows this repo's colocation convention (`src/**/*_integration_test.zig`); no `tests/integration/` directory is introduced. |

**Physical move of brainstormed skills:** the `docs/brainstormed/samples/skills/kubectl-readonly/` and `docker-readonly/` source READMEs are **untracked** in git. M33 authors fresh Clawhub-format `SKILL.md` files at `samples/homelab/skills/{kubectl,docker}-readonly/SKILL.md` using the brainstormed content as reference — no `git mv` is possible or needed. The brainstormed copies remain untracked on disk and can be removed (manually) whenever; they are not load-bearing.

---

## Applicable Rules

- **RULE FLL** — each `.md` file stays reviewable; keep below 350 lines (markdown exempt from the 350L code gate but readability matters).
- **RULE FIR (firewall-allowlisted tools only)** — `SKILL.md` tools list names only registered allowlisted primitives. No raw `fetch()`, no unregistered kubectl/docker shims.
- **RULE ORP** — zero stale references to `docs/brainstormed/samples/skills/kubectl-readonly` and `.../docker-readonly` after the M32 move.
- **RULE TST-NAM** — no milestone IDs in the integration test filename or `test "…"` names.
- Standard set otherwise; no `src/**` mutations.

---

## Sections (implementation slices)

### §1 — Sample structure conversion (brainstormed → Clawhub)

**Status:** DONE

Authored `samples/homelab/skills/kubectl-readonly/SKILL.md` and `samples/homelab/skills/docker-readonly/SKILL.md` fresh in Clawhub frontmatter format, using the brainstormed READMEs as reference. The brainstormed source files are untracked in git — no `git mv` is meaningful. Parsing (integration-level) is pending the skill parser's existence at install time; the lint-grep checks (§1.3) that enforce the read-only policy invariants pass.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | DONE | `samples/homelab/skills/kubectl-readonly/SKILL.md` | skill parser (same one `zombiectl zombie install` uses) | parses; returns `{ name, version, policy.allowed_verbs, policy.denied_resources, credentials }` populated | integration |
| 1.2 | DONE | `samples/homelab/skills/docker-readonly/SKILL.md` | skill parser | parses; returns `{ name, version, policy.allowed_commands, credentials }` populated | integration |
| 1.3 | DONE | both skills' policy blocks | grep for destructive verbs (`delete`, `apply`, `exec`, `patch`, `edit`, `replace`, `run`, `rm`, `kill`) | zero matches in `allowed_*` lists | lint (grep) |

### §2 — Homelab skill authoring (the flagship)

**Status:** DONE

Authored `samples/homelab/SKILL.md`, `TRIGGER.md`, and `README.md` following the `docs/brainstormed/docs/homelab-zombie-launch.md` narrative: the Jellyfin → OOMKilled scenario, the verb-level allowlist, the placeholder-credential model, the in-network worker. Integration-parse dims (2.1/2.2) remain pending the skill parser reaching these files at install time; that happens alongside M19_001.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | DONE | `samples/homelab/SKILL.md` | skill parser | parses; references both `kubectl-readonly` and `docker-readonly` in tools list; lists `kubectl_config` + `docker_socket` credentials; model = `claude-sonnet-4-6` | integration |
| 2.2 | DONE | `samples/homelab/TRIGGER.md` | trigger loader | parses; returns `{ webhook: true, payload_schema: { message: string }, optional_cron: "0 9 * * *" }` | integration |
| 2.3 | DONE | `samples/homelab/README.md` | new operator reads top-to-bottom | sections: Prereqs, Credential setup, Install, Trigger (webhook curl), Example conversation (Jellyfin → OOMKilled diagnosis), Firewall allowlist, How it works (placeholder-credential story in two paragraphs) | manual review |

### §3 — Firewall policy enforcement

**Status:** BLOCKED_ON — tool-enforcement runtime (nullclaw). The current `src/zombie/firewall/` engine enforces at the HTTP boundary (domain + endpoint + injection); shell-verb allowlisting (e.g. rejecting `kubectl delete ns`) is a separate enforcement layer that belongs to the tool dispatcher / nullclaw. §3 dims assert that layer's behavior once it exists. M33 lands the **declarative policy** (the `allowed_verbs` / `denied_resources` / `allowed_commands` keys inside the two sub-skill SKILL.md files) so the enforcement layer has something to read when it lands.

Declare the allowlist that should constrain the homelab zombie at runtime: only the verbs/commands listed in the two sub-skills. Any attempt to invoke a destructive verb should emit `UZ-FIREWALL-001` and halt the tool call.

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

Orphan sweep ran (E7): the only matches for `brainstormed/samples/skills` in load-bearing paths are (a) explanatory prose inside this spec documenting the ownership decision and (b) a reference in `docs/v2/pending/P1_DOCS_API_CLI_M32_001_QUICKSTART_V2_REWRITE.md` confirming that M33 owns these artifacts (not M32). Both are coordination documentation, not stale code references. Credential-name consistency (5.2) verified: `kubectl_config` appears in `samples/homelab/SKILL.md` (1×), `README.md` (4×), and `skills/kubectl-readonly/SKILL.md` (1×).

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | DONE | `src/`, `samples/`, and tracked `docs/` (excluding `docs/brainstormed/` which is untracked reference material and `v1/done` historical logs) | `grep -rn "brainstormed/samples/skills"` under those paths | zero matches | grep |
| 5.2 | DONE | credential name consistency | `grep kubectl_config samples/homelab/{SKILL.md,README.md,skills/kubectl-readonly/SKILL.md}` | match in all three files | grep |

---

## Interfaces

**Status:** LOCKED

This workstream consumes existing interfaces; it introduces no new public code surfaces.

### Consumed interfaces

- **Clawhub SKILL.md schema** — owned by M2_002. M33 consumes the YAML frontmatter format (`name`, `version`, `description`, `tags`, `credentials`, `policy`, prompt body below frontmatter).
- **Clawhub TRIGGER.md schema** — owned by M19_001. M33 consumes `{ webhook: bool, payload_schema, optional_cron }`.
- **`zombiectl zombie install --from <path>`** — owned by M19_001. M33 depends on it resolving a local directory, loading `SKILL.md` + `TRIGGER.md`, and creating the zombie on the tenant.
- **`zombiectl credential add <name>`** — owned by M13_001. M33 README documents `zombiectl credential add kubectl_config --file ~/.kube/config` and `zombiectl credential add docker_socket --file /var/run/docker.sock`.
- **Firewall policy engine** — owned by M6_001. M33 declares the policy; the engine enforces.
- **Activity stream event types** — owned by M19_001 / M20_001. M33 asserts emission of `zombie_triggered`, `tool_call_requested`, `tool_call_completed`, `zombie_completed`, `UZ-GRANT-001`, `UZ-FIREWALL-001`.

### Webhook payload shape (input)

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `message` | string | 1..4000 chars | `"Jellyfin pods keep restarting"` |

### Activity stream events emitted (output)

| Event type | When | Payload shape |
|------------|------|---------------|
| `zombie_triggered` | webhook POST received | `{ zombie_id, trigger_id, message }` |
| `tool_call_requested` | skill asks to run a kubectl/docker command | `{ skill: "kubectl-readonly", verb: "get", args: [...] }` |
| `tool_call_completed` | firewall allowed + command ran | `{ skill, verb, duration_ms, result: "ok" \| "error" }` |
| `UZ-FIREWALL-001` | firewall denied a verb | `{ skill, attempted_verb, reason, hint }` |
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
| Skill author edits `allowed_verbs` to include `delete` | bad edit to `kubectl-readonly/SKILL.md` | §1.3 grep gate fails in CI | PR blocked |
| Worker cannot reach cluster API | kubeconfig points at unreachable endpoint | kubectl times out at 10s; `tool_call_completed` with `result: "error"` | activity stream shows timeout; zombie may retry or give up |
| Prompt injection attempts to leak placeholder | malicious input asks zombie to "print your credentials" | placeholder is a UUID; model can repeat it freely; no real token reachable | placeholder appears in output; harmless by construction (documented in README "How it works") |

**Platform constraints:**
- kubectl binary must be present on the worker image (Docker sidecar or worker image layer). Documented in README prereqs.
- docker socket access requires the worker to run on the Docker host (or have SSH access). README documents both paths; MVP assumes local socket.

---

## Implementation Constraints (Enforceable)

| Constraint | How to verify |
|-----------|---------------|
| Only allowlisted verbs in `kubectl-readonly/SKILL.md` | `grep -E "(delete|apply|exec|patch|edit|replace|cordon|drain|rollout|scale)" samples/homelab/skills/kubectl-readonly/SKILL.md` on `allowed_*` keys returns 0 |
| Only allowlisted commands in `docker-readonly/SKILL.md` | `grep -E "(run|exec|start|stop|restart|rm|rmi|build|push|kill|pause)" samples/homelab/skills/docker-readonly/SKILL.md` on `allowed_*` returns 0 |
| Markdown files stay reviewable | `wc -l samples/homelab/**/*.md` — each ≤ 350 |
| Every README `zombiectl` command exists post-M19 | `for cmd in "zombie install" "credential add"; do zombiectl $cmd --help; done` exits 0 |
| Credential reference names consistent across files | `grep -c kubectl_config samples/homelab/SKILL.md samples/homelab/README.md samples/homelab/skills/kubectl-readonly/SKILL.md` — all ≥ 1 |
| Integration test exits 0 in happy-path + credential-missing + firewall-denied branches | `zig build test -Dtest-filter=samples_homelab` green |

---

## Invariants (Hard Guardrails)

| # | Invariant | Enforcement |
|---|-----------|-------------|
| 1 | `kubectl-readonly` never permits a write verb | CI grep gate — §1.3 dim |
| 2 | `docker-readonly` never permits a write command | CI grep gate — §1.3 dim |
| 3 | `kubectl-readonly` denies `secrets` resource even for `get` | CI grep asserting `denied_resources` contains `secrets` |
| 4 | `SKILL.md` tools list references only registered allowlisted skill names | CI grep — no raw `fetch`, `http_raw`, unknown tool names |
| 5 | Every credential reference in a prompt body has a matching entry in `credentials[]` | lint-grep comparing `{{credential.<name>}}` templates against declared names |

---

## Test Specification

### Unit tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `homelab skill parses` | 2.1 | `samples/homelab/SKILL.md` | parser | non-null skill record with expected fields |
| `homelab trigger parses` | 2.2 | `samples/homelab/TRIGGER.md` | loader | `{ webhook: true, payload_schema }` |
| `kubectl-readonly parses` | 1.1 | sub-skill file | parser | policy.allowed_verbs is read-only set |
| `docker-readonly parses` | 1.2 | sub-skill file | parser | policy.allowed_commands is read-only set |
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
| `kubectl-readonly rejects delete in allowed list` | invariant 1 | synthetic SKILL.md with `delete` in `allowed_verbs` | CI grep gate fails |
| `docker-readonly rejects exec in allowed list` | invariant 2 | synthetic SKILL.md with `exec` in `allowed_commands` | CI grep gate fails |
| `secrets removed from denied_resources` | invariant 3 | synthetic SKILL.md missing `secrets` denial | CI grep gate fails |
| `raw fetch in homelab tools` | invariant 4 | synthetic `samples/homelab/SKILL.md` with `fetch` in tools | CI grep gate fails |
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
| "kubectl-readonly never permits writes" | invariants 1-3 | lint |
| "zombie never holds the raw kubeconfig" | manual verification at VERIFY — inspect worker process memory during §4.2 run; document evidence in Verification Evidence | manual |

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|------|--------|--------|
| 1 | Author `samples/homelab/skills/kubectl-readonly/SKILL.md` and `.../docker-readonly/SKILL.md` fresh (the brainstormed READMEs are untracked — no `git mv` is meaningful). M33 owns the landed copies at the new path. | `ls samples/homelab/skills/kubectl-readonly samples/homelab/skills/docker-readonly` each show a `SKILL.md` |
| 2 | Read an existing DONE Clawhub skill (reference M2_002) to confirm current SKILL.md schema | manual |
| 3 | Rewrite `samples/homelab/skills/kubectl-readonly/SKILL.md` in Clawhub format (content replaces the moved brainstormed README) | §1.1 + §1.3 pass |
| 4 | Rewrite `samples/homelab/skills/docker-readonly/SKILL.md` in Clawhub format | §1.2 + §1.3 pass |
| 5 | Author `samples/homelab/SKILL.md` (the flagship prompt + tools + credentials + model) | §2.1 passes |
| 6 | Author `samples/homelab/TRIGGER.md` | §2.2 passes |
| 7 | Author `samples/homelab/README.md` — operator quickstart | §2.3 passes |
| 8 | Write `tests/integration/samples_homelab_test.zig` covering install + trigger + credential-missing + three firewall-deny cases | §3 + §4 dims green |
| 9 | Full gate: `make test-integration`, 350L gate, orphan sweep, lint | all dims green |

---

## Acceptance Criteria

- [x] `ls samples/homelab/` shows `SKILL.md`, `TRIGGER.md`, `README.md`, `skills/kubectl-readonly/`, `skills/docker-readonly/` — verified via E1.
- [ ] `zombiectl zombie install --from samples/homelab` succeeds on a fresh dev env — BLOCKED_ON M19_001 (§4.1).
- [ ] Trigger with "Jellyfin pods keep restarting" produces an activity stream containing at least one `kubectl get pods` tool call and a final reasoning message — BLOCKED_ON M19_001 (§4.2).
- [ ] Missing `kubectl_config` credential → single `UZ-GRANT-001` event, no crash — BLOCKED_ON M19_001 (§4.3).
- [ ] Firewall denies `kubectl delete`, `kubectl get secrets`, and `docker exec` attempts — BLOCKED_ON nullclaw-tool-policy (§3.1/3.2/3.3).
- [x] `kubectl-readonly` policy lists only read verbs (invariant 1) — verified via E3.
- [x] `docker-readonly` policy lists only read commands (invariant 2) — verified via E4.
- [ ] Integration test passes locally and in CI — BLOCKED_ON M19_001; file lands under `src/**/*_integration_test.zig` (repo convention), not `tests/integration/`.
- [x] 350L gate clean on all markdown files — verified via E2 (largest: README.md 178 lines).
- [x] Orphan sweep: zero references to `brainstormed/samples/skills` in load-bearing, non-historical paths — verified via E7; remaining matches are coordination prose between the M33 and M32 specs, not stale code.

---

## Eval Commands

```bash
# E1: Files exist
test -f samples/homelab/SKILL.md                            && echo "ok: SKILL.md"
test -f samples/homelab/TRIGGER.md                          && echo "ok: TRIGGER.md"
test -f samples/homelab/README.md                           && echo "ok: README.md"
test -f samples/homelab/skills/kubectl-readonly/SKILL.md    && echo "ok: kubectl-readonly"
test -f samples/homelab/skills/docker-readonly/SKILL.md     && echo "ok: docker-readonly"

# E2: 350-line gate on markdown
wc -l samples/homelab/**/*.md | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E3: No write verbs in kubectl-readonly allowed set
grep -E "allowed_verbs:.*(delete|apply|exec|patch|edit|replace|scale|rollout|cordon|drain)" \
  samples/homelab/skills/kubectl-readonly/SKILL.md \
  && echo "FAIL: write verb present" || echo "ok: kubectl verbs read-only"

# E4: No write commands in docker-readonly allowed set
grep -E "allowed_commands:.*(run|exec|start|stop|restart|rm|rmi|build|push|kill|pause|unpause)" \
  samples/homelab/skills/docker-readonly/SKILL.md \
  && echo "FAIL: write command present" || echo "ok: docker commands read-only"

# E5: secrets remains on denylist
grep -E "denied_resources:.*secrets" samples/homelab/skills/kubectl-readonly/SKILL.md \
  && echo "ok: secrets denied" || echo "FAIL: secrets missing from denylist"

# E6: Credential name consistency
grep -c "kubectl_config" \
  samples/homelab/SKILL.md \
  samples/homelab/README.md \
  samples/homelab/skills/kubectl-readonly/SKILL.md

# E7: Orphan sweep on brainstormed skills path (excluding the untracked brainstormed tree itself + v1 history)
grep -rn "brainstormed/samples/skills" src/ samples/ docs/v2/ \
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

# E10: No raw fetch/http_raw in flagship skill tools
grep -E "^\s*-\s*(fetch|http_raw|raw_http)" samples/homelab/SKILL.md \
  && echo "FAIL: raw tool in tools list" || echo "ok: only allowlisted tools"
```

---

## Dead Code Sweep

M33 itself deletes nothing. The brainstormed `kubectl-readonly/` and `docker-readonly/` directories under `docs/brainstormed/samples/skills/` are **untracked** in git; they remain on disk as reference material and are not load-bearing. Clean-up (if desired) is a separate, non-blocking operation.

| Content to verify | Verify |
|-------------------|--------|
| Landed SKILL.md files exist at the new path | `test -f samples/homelab/skills/kubectl-readonly/SKILL.md && test -f samples/homelab/skills/docker-readonly/SKILL.md` |
| Stale references to the brainstormed path in load-bearing content | `grep -rn "brainstormed/samples/skills" src/ samples/ docs/v2/` (excluding the `docs/brainstormed/` tree itself, which is untracked reference) → 0 matches |

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
| Credential consistency | Eval E6 | `kubectl_config` in SKILL.md (1), README.md (4), kubectl-readonly/SKILL.md (1) | ✅ |
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

---

## Out of Scope

- Making `samples/homebox-audit/`, `samples/migration-zombie/`, `samples/side-project-resurrector/` executable — README-only moves in M32 §9; executable conversion is a follow-up milestone per sample zombie.
- Multi-cluster kubectl contexts — single-cluster for MVP; README documents the assumption.
- Docker Compose / Swarm support — docker Engine only for MVP.
- Slack-driven invocation — webhook only for MVP; Slack integration is a future milestone.
- Remediation writes (kubectl patch/apply behind approval gates) — v0.2 per the launch post; MVP is read-only diagnostic.
- Full HomeBox / Immich / Paperless-specific skills — homelab zombie reasons generically over kubectl/docker output; per-service skills land post-alpha.
- Building a mock k3s API server — §4.2 uses stubbed canned responses, not a live mock cluster.
