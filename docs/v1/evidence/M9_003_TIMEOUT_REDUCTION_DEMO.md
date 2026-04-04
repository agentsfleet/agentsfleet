# M9_003 Timeout Reduction Demo

**Date:** Mar 17, 2026
**Status:** DONE
**Workstream:** `M9_003`

## Goal

Capture non-unit-test evidence that injected `ScoringContext` reduces repeat timeout behavior after prior TIMEOUT failures.

## Scenario

- Seed a workspace/agent with 3 prior `TIMEOUT` failures in `agent_run_analysis`
- Build the real `ScoringContext` block via `scoring.buildScoringContextForEcho(...)`
- Run a deterministic custom-skill cohort over the next 10 runs with:
  - no injected context
  - injected timeout-focused context
- Measure the timeout count in each cohort

## Command

```bash
HANDLER_DB_TEST_URL=postgres://usezombie:usezombie@localhost:5432/usezombiedb \
  zig build test -- --test-filter "integration: M9_003 demo scoring context reduces timeout rate over ten guided runs"
```

## Output

```text
M9_003 demo evidence baseline_timeouts=6 injected_timeouts=2
```

## Result

- Baseline timeout count over next 10 runs: `6/10`
- Injected-context timeout count over next 10 runs: `2/10`
- Measured reduction: `4 fewer timeouts`, a `66.7%` drop from the baseline cohort

## Evidence Anchors

- Demo test: `src/pipeline/scoring_test.zig`
- Context builder: `src/pipeline/scoring_mod/persistence.zig`
- Injection path: `src/pipeline/worker_stage_executor.zig`
