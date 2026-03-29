# M16_001: Spec-to-PR Autonomous Gate Loop

**Prototype:** v1.0.0
**Milestone:** M16
**Workstream:** 001
**Date:** Mar 28, 2026
**Status:** DONE
**Priority:** P0 — Core CEO plan: single-agent spec-to-PR autonomous loop is the primary product capability
**Batch:** B2
**Depends on:** M15_001 (Self-Serve Role — workspace onboarding must work), M12_003 (Executor NullClaw — DONE)

---

## 1.0 Gate Tool Definitions

**Status:** DONE

NullClaw tools that let the agent invoke `make lint`, `make test`, and `make build` inside the sandbox. Each tool captures both stdout and stderr. Gate result is a typed struct: pass/fail + combined output. Tools are declared in the executor `StartStage` payload tool spec — not hardcoded into the agent runtime. The worker constructs the tool list from the active profile's gate policy.

**Dimensions:**
- 1.1 DONE Define three gate tools: `run_lint`, `run_test`, `run_build` — each maps to `make lint`, `make test`, `make build` respectively; all run in the worktree root; each returns `{ exit_code: int, stdout: string, stderr: string }`
- 1.2 DONE Gate tools run as blocking synchronous tool calls inside the NullClaw turn — agent awaits the result before emitting next message; no background or fire-and-forget invocation
- 1.3 DONE Each gate tool enforces a per-command timeout (default: `GATE_TOOL_TIMEOUT_MS`, 300000 = 5 min per gate); timeout is surfaced as a tool error with `FailureClass.timeout_kill`
- 1.4 DONE Gate tool definitions are stored on the active agent profile as `gate_tools: []GateTool`; profile resolver passes them to the executor `StartStage` payload; worker does not hardcode tool names

---

## 2.0 Self-Repair Loop

**Status:** DONE

On any gate failure, the worker appends the failed gate's stderr + stdout as a new user turn and calls `StartStage` again. This is a multi-turn NullClaw conversation within a single executor session — the session stays open across repair iterations. The worker tracks loop count per gate and enforces `max_repair_loops` from the active agent profile (default: 3). When the loop count is exhausted without passing all gates, the run transitions to `FAILED` with a structured gate failure record.

**Dimensions:**
- 2.1 DONE On gate tool failure (`exit_code != 0`), append a new user turn: `"Gate '<name>' failed.\n\nstdout:\n<stdout>\n\nstderr:\n<stderr>\n\nFix the issue and re-run the gate."` — then call `StartStage` within the same session; increment per-gate loop counter
- 2.2 DONE `max_repair_loops` is an integer field on the agent profile (default: 3); worker reads it at profile resolution time; configurable without code change
- 2.3 DONE When `loop_count >= max_repair_loops` for any gate, do not call `StartStage` again; close session; transition run to `FAILED`; persist structured gate failure record: `{ gate: string, attempt: int, exit_code: int, stdout: string, stderr: string }` as a run artifact in Postgres
- 2.4 DONE Prometheus counter `zombied_gate_repair_loops_total` with label `gate` (`lint`/`test`/`build`); counter `zombied_gate_repair_exhausted_total` when max loops reached

---

## 3.0 PR Creation with Agent Explanation

**Status:** DONE

After all three gates pass, the worker pushes the branch and opens a PR via the workspace's GitHub App installation token. The PR body is the agent's plain-English change summary generated as the final NullClaw turn. A separate scorecard comment is posted after PR creation with gate results, loop counts per gate, and wall time. Worker uses the GitHub App token resolved from the workspace vault — no personal access token path.

**Dimensions:**
- 3.1 DONE Final NullClaw turn after all gates pass requests agent-generated change summary: `"All gates passed. Write a plain-English PR description covering: (1) what changed, (2) why, (3) what tests were added or modified. No markdown headers — just flowing paragraphs."` — worker captures this as `pr_body`
- 3.2 DONE Worker pushes branch via git CLI subprocess (`git push origin <branch>`) using the GitHub App installation token as the credential; branch name format: `zombie/<run_id_short>/<spec_slug>`
- 3.3 DONE Worker calls GitHub REST `POST /repos/{owner}/{repo}/pulls` with `title` from spec frontmatter (or first line of spec file), `body` from `pr_body`, `head` as pushed branch, `base` as the recorded base branch
- 3.4 DONE After PR creation, post a scorecard comment via `POST /repos/{owner}/{repo}/issues/{pr_number}/comments` with gate results table: gate name, pass/fail, repair loop count, wall seconds per gate; total wall seconds; run_id

---

## 4.0 Worktree Isolation

**Status:** DONE

Each run gets a dedicated git worktree created from the target repo's base branch HEAD at the time the run starts. The worktree path lives under a per-run temp directory managed by the worker. Base commit SHA is recorded on the run row before execution starts. Worktree is cleaned up via `git worktree remove --force` after the run completes, regardless of pass or fail.

**Dimensions:**
- 4.1 DONE Worker clones (or fetches) the target repo into a shared bare repo cache at `/tmp/zombie/repos/<repo_id>/`; creates a worktree per run at `/tmp/zombie/worktrees/<run_id>/` via `git worktree add <path> <base_branch>`; records HEAD SHA on the run row as `base_commit_sha`
- 4.2 DONE Landlock policy for the executor is set to the worktree path as the workspace root — only the run's worktree is writable; no other run's worktree is accessible
- 4.3 DONE On run completion (pass or fail), worker runs `git worktree remove --force /tmp/zombie/worktrees/<run_id>/` in a `defer` block; log cleanup result; do not propagate cleanup errors as run failures
- 4.4 DONE If worktree creation fails (disk full, clone error, network error), transition run to `FAILED` immediately with `FailureClass.startup_posture` and a structured error record naming the git operation and exit code

---

## 5.0 Acceptance Criteria

**Status:** DONE

- [x] 5.1 Worker clones repo, creates isolated worktree, records base commit SHA (CLI submission deferred to M18_001)
- [x] 5.2 NullClaw agent implements the spec, invokes gate tools in sequence; all three gates (`make lint`, `make test`, `make build`) must pass for PR creation to proceed
- [x] 5.3 On gate failure, agent receives stderr/stdout as a new user turn and self-repairs; repair attempts are bounded by `max_repair_loops` from the active profile
- [x] 5.4 After all gates pass, worker pushes branch and opens PR via GitHub App; PR body is agent-generated change summary; scorecard comment is posted
- [x] 5.5 If repair loops exhausted on any gate, run is marked `FAILED` with a structured gate failure artifact; no PR is opened
- [x] 5.6 Worktree is removed after every run (pass or fail); no orphaned worktrees survive a worker restart

---

## 6.0 Out of Scope

- Auto-merge after PR creation (separate policy workstream)
- Multi-agent parallel gate execution (single agent per run in v1)
- PR scoring or quality rating dashboard (separate observability workstream)
- Repo selection UI or workspace-level repo registry (deferred to v3 Mission Control)
- Mid-token session migration for repair loops (v1 durability is stage-boundary only, per ARCHITECTURE.md)
