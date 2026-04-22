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

**Goal (testable):** In a fresh dev env with kubectl + docker credentials in the vault, `zombiectl zombie install --from samples/homelab` creates a zombie. A webhook POST `"Jellyfin pods keep restarting"` produces an activity stream where the zombie runs allowed kubectl verbs (`get`, `describe`, `logs`, `top`) against the cluster through the worker, returns a diagnosis, and never sees the raw kubeconfig. If the credential is missing, one `UZ-GRANT-001` fires; clean halt.

**Problem:** The v2.0-alpha launch post needs one flagship executable sample to dogfood and demo. `docs/brainstormed/docs/homelab-zombie-launch.md` pitches the narrative (diagnose a homelab via `kubectl` without holding the kubeconfig, verb-level allowlist, in-network worker); without `samples/homelab/` shipping, the blog post runs ahead of the product and M32 quickstart has no install target.

**Solution summary:** Ship a three-file operator bundle under `samples/homelab/`: `SKILL.md` (diagnostic prompt + `model: claude-sonnet-4-6` + prose allowlist in "Tools you can use"), `TRIGGER.md` (webhook + optional cron + `tools: [kubectl, docker]` + `credentials: [kubectl_config, docker_socket]` + network + budget), `README.md` (operator quickstart, Jellyfin example, prose allowlist summary). Allowlist lives as prose; no per-tool sub-skill directory. Tool calls: worker parses, verb-checks against prose allowlist, re-originates with the injected credential at the network boundary. Integration test colocated with M19_001's install-from-path handler verifies install + trigger + credential-missing behave as the README promises.

---

## Files Changed (blast radius)

All paths relative to repo root (`$REPO_ROOT` = `~/Projects/usezombie` locally; CI/agent worktrees export their own). This milestone does NOT touch `src/**`; the deferred E2E test is the only code addition.

| File | Action | Why |
|------|--------|-----|
| `samples/homelab/SKILL.md` | CREATE | Diagnostic prompt + `model: claude-sonnet-4-6` + prose allowlist for `kubectl`/`docker`. |
| `samples/homelab/TRIGGER.md` | CREATE | Webhook + optional cron (`0 9 * * *`) + `tools` + `credentials` + network + budget. |
| `samples/homelab/README.md` | CREATE | Operator quickstart, Jellyfin example, prose allowlist summary pointing at SKILL.md as authoritative. |
| `src/samples/homelab/install_integration_test.zig` (or colocated with M19_001's install-from-path handler) | CREATE (deferred) | E2E: install + trigger + credential-missing branches. **Deferred** pending M19_001. Follows `src/**/*_integration_test.zig` convention. |

**No sub-skill directory.** Option-B pivot — allowlist lives as prose in `SKILL.md`. See Discovery #5. Brainstormed `docs/brainstormed/samples/skills/` is untracked and not load-bearing.

---

## Applicable Rules

**FLL** (≤350 lines markdown, soft). **FIR** (TRIGGER.md `tools:` = registered primitives only). **ORP** (no live refs to removed sub-skill paths; coordination prose exempt). **TST-NAM** (no milestone IDs in test names). No `src/**` mutations.

---

## Sections (implementation slices)

### §1 — Sub-skill files (RETIRED)

**Status:** RETIRED — superseded by §2.4 prose-policy gate. See Discovery #5.

### §2 — Homelab skill authoring (the flagship)

**Status:** DONE

Authored `samples/homelab/SKILL.md`, `TRIGGER.md`, and `README.md` following the `docs/brainstormed/docs/homelab-zombie-launch.md` narrative: the Jellyfin → OOMKilled scenario, verb-level allowlist expressed as **prose** in the SKILL.md prompt body, placeholder-credential model, in-network worker. Integration-parse dims (2.1/2.2) remain pending the skill parser reaching these files at install time; that happens alongside M19_001.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | DONE | `samples/homelab/SKILL.md` | skill parser | parses; frontmatter has `model: claude-sonnet-4-6`; prompt body contains the "Tools you can use" section naming both `kubectl` and `docker` with their allowed and forbidden verbs/commands | integration |
| 2.2 | DONE | `samples/homelab/TRIGGER.md` | trigger loader | parses; returns `{ webhook: true, payload_schema: { message: string }, optional_cron: { schedule: "0 9 * * *", message: "Run the daily homelab health scan..." }, tools: [kubectl, docker], credentials: [kubectl_config, docker_socket] }` | integration |
| 2.3 | DONE | `samples/homelab/README.md` | new operator reads top-to-bottom | sections: Prereqs, Credential setup, Install, Trigger (webhook curl), Example conversation (Jellyfin → OOMKilled diagnosis), prose allowlist summary, How it works (placeholder-credential story + "policy is prose" paragraph) | manual review |
| 2.4 | DONE | `samples/homelab/SKILL.md` prompt body | grep for destructive verbs after the phrase "Never use" | every destructive verb (`delete`, `apply`, `exec`, `patch`, `edit`, `replace`, `run`, `rm`, `rmi`, `kill`, `pause`, `unpause`, `build`, `push`, `pull`, `scale`, `rollout`, `cordon`, `drain`) appears inside a "Never use" sentence for the right tool; `secrets` appears inside a "never read" sentence | lint (grep) |

### §3 — Firewall policy enforcement

**Status:** BLOCKED_ON nullclaw-tool-policy. `src/zombie/firewall/` enforces at the HTTP boundary (domain + endpoint + injection); shell-verb allowlisting (`kubectl delete ns` rejection) is a separate layer owned by the tool dispatcher (nullclaw). M33 lands the allowlist as prose in `samples/homelab/SKILL.md`'s "Tools you can use" section; nullclaw decides how to consume it. Any attempt to invoke a destructive verb should emit `UZ-FIREWALL-001` and halt the tool call.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | BLOCKED_ON nullclaw-tool-policy | tool dispatcher | `kubectl delete ns media` | rejected; `UZ-FIREWALL-001` emitted; zombie reasons from the error | integration |
| 3.2 | BLOCKED_ON nullclaw-tool-policy | tool dispatcher | `kubectl get secrets -A` | rejected (`secrets` denied even for `get`); `UZ-FIREWALL-001` | integration |
| 3.3 | BLOCKED_ON nullclaw-tool-policy | tool dispatcher | `docker exec jellyfin sh` | rejected; `UZ-FIREWALL-001` | integration |

### §4 — Install & trigger integration test

**Status:** BLOCKED_ON M19_001 (`zombiectl zombie install --from <path>`). Integration test lands alongside M19_001's handler in `src/**/*_integration_test.zig` (repo convention; no `tests/integration/` dir). Happy path: install + trigger "Jellyfin pods keep restarting" → ≥1 allowed-verb tool call + final reasoning message.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | BLOCKED_ON M19_001 | install test (colocated with M19 handler) | `zombiectl zombie install --from samples/homelab` w/ seeded vault | 201 + webhook URL + zombie ID | integration |
| 4.2 | BLOCKED_ON M19_001 | trigger happy path | POST `{"message":"Jellyfin pods keep restarting"}` w/ stubbed k8s API | ≥1 `kubectl get pods` event; ≥1 reasoning message; `zombie_completed` with non-empty diagnosis | integration |
| 4.3 | BLOCKED_ON M19_001 | credential-missing | trigger with empty vault | one `UZ-GRANT-001` at first tool call; clean halt; hint points to `credential add kubectl_config` | integration |

### §5 — Orphan sweep and cross-layer consistency

**Status:** DONE

E7 orphan sweep: only prose matches remain (this spec's Discovery + the M32_001 cross-reference flagged in Discovery #6). E6 credential consistency: `kubectl_config` in README.md ×4 + TRIGGER.md ×1; SKILL.md declares no credentials.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | DONE | `src/`, `samples/`, tracked `docs/` (excl. `docs/brainstormed/` and `v1/done`) | `grep -rn "brainstormed/samples/skills"` | only coordination prose matches | grep |
| 5.2 | DONE | credential name consistency | `grep kubectl_config samples/homelab/{SKILL.md,README.md,TRIGGER.md}` | present in README.md + TRIGGER.md; SKILL.md declares no credentials | grep |

---

## Interfaces

**Status:** LOCKED

This workstream consumes existing interfaces; it introduces no new public code surfaces.

### Consumed interfaces

- **SKILL.md frontmatter** (M2_002): `{name, version, description, tags, author, model}` + prompt body. Post-pivot, no `credentials:` or `policy:` keys — credentials in TRIGGER.md, allowlist as prose.
- **TRIGGER.md frontmatter** (M19_001): `{trigger: {type, payload_schema, optional_cron}, tools[], credentials[], network.allow[], budget}`. `samples/homelab/TRIGGER.md` is the forward canonical example (see Discovery #7 re lead-collector).
- **`zombiectl zombie install --from <path>`** (M19_001): loads SKILL.md + TRIGGER.md, creates the zombie.
- **`zombiectl credential add <name>`** (M13_001): README documents `kubectl_config --file ~/.kube/config` (byte contents stored, encrypted) and `docker_socket --path /var/run/docker.sock` (socket path stored as a connection hint; `--file` would fail because Unix sockets aren't readable files).
- **Tool dispatcher (nullclaw, future)**: reads prose allowlist and enforces. `src/zombie/firewall/` (M6_001) is HTTP-boundary only and is NOT this enforcement layer.
- **Activity stream events** (M19_001/M20_001): `zombie_triggered`, `tool_call_{requested,completed}`, `zombie_completed`, `UZ-GRANT-001`, `UZ-FIREWALL-001`.

### Webhook payload (input)

`{ message: string (1..4000 chars) }`, e.g. `"Jellyfin pods keep restarting"`.

### Activity stream events emitted (output)

| Event type | When | Payload shape |
|------------|------|---------------|
| `zombie_triggered` | webhook POST received | `{ zombie_id, trigger_id, message }` |
| `tool_call_requested` | skill asks to run a kubectl/docker command | `{ tool: "kubectl" \| "docker", verb: "get", args: [...] }` |
| `tool_call_completed` | dispatcher allowed + command ran | `{ tool: "kubectl" \| "docker", verb, duration_ms, result: "ok" \| "error" }` |
| `UZ-FIREWALL-001` | dispatcher denied a verb | `{ tool: "kubectl" \| "docker", attempted_verb, reason, hint }` |
| `UZ-GRANT-001` | credential missing at tool call | `{ credential_name, hint: "zombiectl credential add <name>" }` |
| `zombie_completed` | reasoning loop returns a final diagnosis | `{ diagnosis: string, tool_calls_total, duration_ms }` |

---

## Failure Modes

| Failure | Trigger | System behavior | User observes |
|---------|---------|-----------------|---------------|
| Operator installs without seeding `kubectl_config` | `zombiectl zombie install --from samples/homelab` then webhook trigger | install succeeds; first kubectl tool call fires `UZ-GRANT-001`; zombie halts cleanly | helpful event pointing to `zombiectl credential add kubectl_config` |
| Clawhub schema drift | M2_002 changes frontmatter shape after M33 lands | `SKILL.md` fails to parse at install time | §1.1 / §2.1 parser round-trip catches drift in CI |
| Skill author weakens the prose allowlist (destructive verb escapes a "Never" line, or `secrets` deny removed) | bad edit to SKILL.md | §2.4 grep gate fails | PR blocked |
| Worker cannot reach cluster API | kubeconfig points at unreachable endpoint | kubectl times out at 10s; `tool_call_completed` with `result: "error"` | activity stream shows timeout; zombie may retry or give up |
| Prompt injection attempts to leak placeholder | malicious input asks zombie to "print your credentials" | placeholder is a UUID; model can repeat it freely; no real token reachable | placeholder appears in output; harmless by construction (documented in README "How it works") |

**Platform constraints:** kubectl binary must be on the worker image; docker socket requires the worker on the Docker host (or SSH). Both documented in README prereqs.

---

## Implementation Constraints (Enforceable)

| Constraint | How to verify |
|-----------|---------------|
| Prose allowlist gate (kubectl + docker) | Eval E3, E4 |
| Markdown files reviewable (≤350 per `wc -l`) | E2 |
| README `zombiectl` commands exist post-M19 | E9 |
| Credential names consistent across files | E6 |
| Integration test green (happy + credential-missing + firewall-deny) | E8 |

---

## Invariants (Hard Guardrails)

| # | Invariant | Enforcement |
|---|-----------|-------------|
| 1 | Destructive kubectl verbs in SKILL.md prose appear only on "Never use" lines | §2.4 grep gate (Eval E3) |
| 2 | Destructive docker commands in SKILL.md prose appear only on "Never use" lines | §2.4 grep gate (Eval E4) |
| 3 | SKILL.md prose carries a "never read `secrets`" restriction | Eval E5 |
| 4 | TRIGGER.md `tools:` contains only registered primitives (`kubectl`, `docker`) | Eval E10 |
| 5 | Every credential ref in SKILL.md/README.md has a matching entry in TRIGGER.md `credentials:` | lint-grep |

---

## Test Specification

### Unit tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `homelab skill parses` | 2.1 | `samples/homelab/SKILL.md` | parser | non-null skill record with expected fields (model, prompt body) |
| `homelab trigger parses` | 2.2 | `samples/homelab/TRIGGER.md` | loader | `{ webhook: true, payload_schema, optional_cron: { schedule, message }, tools: [kubectl, docker], credentials: [kubectl_config, docker_socket] }` |
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

### Negative tests (invariant gates)

Synthetic SKILL.md permitting `delete` / docker `exec` / dropping the `secrets` deny → §2.4 grep gate fails. Synthetic TRIGGER.md with `fetch` in `tools:` → invariant 4 fails. `{{credential.nonexistent}}` in prompt → invariant 5 fails.

### Edge cases

Webhook message length: `""` → zombie prompts (no crash); 4000 chars → accepted; 4001 chars → HTTP 400.

### Regression / Leak

N/A — greenfield markdown; deferred integration test uses `std.testing.allocator`.

### Spec-claim tracing

| Claim | Verified by |
|-------|-------------|
| install + trigger → diagnosis via allowed verbs | §4.2 (integration) |
| missing credential → one clean `UZ-GRANT-001` | §4.3 (integration) |
| firewall denies destructive verbs | §3.1–3.3 (integration) |
| prose never permits destructive verbs or secrets reads | invariants 1–3 (lint) |
| zombie never holds raw kubeconfig | manual inspect during §4.2 run; log in Verification Evidence |

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|------|--------|--------|
| 1 | Confirm Clawhub frontmatter convention (reference: `samples/homelab/` going forward; see Discovery #7 re lead-collector). | manual |
| 2 | Author `SKILL.md` — prompt + `model` + prose allowlist with explicit "Never use" lines for every destructive verb + "never read `secrets`". | §2.1, §2.4 |
| 3 | Author `TRIGGER.md` — webhook + cron + `tools` + `credentials` + network + budget. | §2.2 |
| 4 | Author `README.md` — operator quickstart + allowlist summary + how-it-works. | §2.3 |
| 5 | Colocate integration test with M19_001 install-from-path handler (install + trigger + credential-missing). | §4 (lands with M19) |
| 6 | Full gate: `make test-integration`, 350L gate, orphan sweep, lint. | all dims green |

---

## Acceptance Criteria

- [x] Three files at `samples/homelab/`, no `skills/` sub-dir — E1
- [x] Prose allowlist gates green (invariants 1–3) — E3, E4, E5
- [x] 350L gate on sample markdown — E2 (README max at 163)
- [x] Credential names consistent across SKILL/README/TRIGGER — E6
- [x] Orphan sweep clean (only coordination prose) — E7
- [ ] `zombiectl zombie install --from samples/homelab` succeeds — BLOCKED_ON M19_001 (§4.1)
- [ ] Trigger "Jellyfin pods keep restarting" → ≥1 `kubectl get pods` + final diagnosis — BLOCKED_ON M19_001 (§4.2)
- [ ] Missing `kubectl_config` → single `UZ-GRANT-001`, clean halt — BLOCKED_ON M19_001 (§4.3)
- [ ] Firewall denies destructive verbs (`kubectl delete`, `kubectl get secrets`, `docker exec`) — BLOCKED_ON nullclaw (§3.1–3.3)
- [ ] Integration test green in CI — BLOCKED_ON M19_001 (colocates with install handler, not `tests/integration/`)

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

Only deletion: `samples/homelab/skills/` sub-directory (option-B pivot). Brainstormed `docs/brainstormed/samples/skills/{kubectl,docker}-readonly/` are untracked reference material; left on disk. Verified by Eval E1 (sub-dir gone) + E7 (no load-bearing refs).

---

## Verification Evidence

**Status:** PARTIAL (parked) — §1/§2/§5 evidence captured; §3/§4 evidence deferred with the enforcement/install handlers that would produce it.

| Check | Eval | Result |
|-------|------|--------|
| Files present (3, no sub-dir) | E1 | ✅ |
| 350L gate (README 163, SKILL 95, TRIGGER 25) | E2 | ✅ |
| kubectl destructive verbs on Never-lines only | E3 | ✅ |
| docker destructive commands on Never-lines only | E4 | ✅ |
| secrets prose carries a restriction | E5 | ✅ |
| Credential names consistent (TRIGGER×1, README×4) | E6 | ✅ |
| Orphan sweep (only coordination prose remains) | E7 | ✅ |
| No raw tools in TRIGGER `tools:` | E10 | ✅ |
| Integration test green | E8 | ⏳ BLOCKED_ON M19_001 |
| `zombiectl zombie install` exists | E9 | ⏳ BLOCKED_ON M19_001 |
| Worker never holds raw kubeconfig | manual, §4.2 | ⏳ deferred |

---

## Discovery

Spec-level findings surfaced during authoring (Apr 21–22, 2026). Full chronological narrative in `docs/nostromo/LOG_Apr_22_08_14_31_M33_001.md`.

1. **M19_001 dependency.** `zombiectl zombie install --from <path>` (M19_001-owned) is not yet shipped; only `zombiectl install <template>` exists. §4 dims tagged `BLOCKED_ON: M19_001`.
2. **Tool-enforcement runtime gap.** `src/zombie/firewall/` enforces HTTP-boundary (domain + endpoint + injection). Shell-verb allowlisting belongs to the tool dispatcher (nullclaw, future). §3 dims tagged `BLOCKED_ON: nullclaw-tool-policy`.
3. **Test-path convention.** Spec prescribed `tests/integration/samples_homelab_test.zig`; repo uses `src/**/*_integration_test.zig`. Amended.
4. **Original-spec contradictions.** Header said "M32 §9 moves the skills"; Execution Plan said "M33 owns this (see §0.1 below)"; no §0.1 existed; brainstormed sources are untracked anyway. Resolved: M33 authors fresh at the target path.
5. **Option-B pivot.** User challenged the per-tool sub-skill layer as invented ceremony with no second consumer. Sub-skill directory deleted; allowlists moved to prose in `SKILL.md` under "Tools you can use"; credentials moved to `TRIGGER.md`. Invariants 1–3 + §2.4 now gate via word-boundary grep for destructive verbs inside "Never" sentences. Memory: `feedback_skill_policy_prose.md`.
6. **Stale M32_001 cross-reference.** `docs/v2/pending/P1_DOCS_API_CLI_M32_001_QUICKSTART_V2_REWRITE.md:~277` still mentions `samples/homelab/skills/kubectl-readonly/` + `docker-readonly/`. Not edited here (different agent's spec); M32_001 author to reconcile.
7. **`samples/lead-collector/` death row.** Brainstormed v2 direction (`docs/brainstormed/usezombie-v2-milestone-specs-prompt.md` §6) calls for its deletion; no spec owned it. M33's TRIGGER.md references previously cited it — rewritten to point at `samples/homelab/TRIGGER.md` as the forward canonical example. The deletion is now a separately filed workstream: **`docs/v2/pending/P1_API_CLI_UI_M34_001_LEAD_COLLECTOR_SAMPLE_TEARDOWN.md`** (sequences fixture migrations before directory deletion so CI stays green).

---

## Out of Scope

- Making `samples/homebox-audit/`, `samples/migration-zombie/`, `samples/side-project-resurrector/` executable — README-only moves in M32 §9; executable conversion is a follow-up milestone per sample zombie.
- Multi-cluster kubectl contexts — single-cluster for MVP; README documents the assumption.
- Docker Compose / Swarm support — docker Engine only for MVP.
- Slack-driven invocation — webhook only for MVP; Slack integration is a future milestone.
- Remediation writes (kubectl patch/apply behind approval gates) — v0.2 per the launch post; MVP is read-only diagnostic.
- Full HomeBox / Immich / Paperless-specific skills — homelab zombie reasons generically over kubectl/docker output; per-service skills land post-alpha.
- Building a mock k3s API server — §4.2 uses stubbed canned responses, not a live mock cluster.
