# M14_002: Memory Archetype Policies — Skill Templates That Teach Zombies What to Remember

**Prototype:** v2
**Milestone:** M14
**Workstream:** 002
**Date:** Apr 12, 2026: 03:45 PM
**Status:** PENDING
**Priority:** P1 — Without opinionated policies, the memory-store API is neutral and zombies store garbage or nothing
**Batch:** B2
**Depends on:** M14_001 (storage layer + `memory_*` tools wired)

---

## Overview

**Goal (testable):** A Lead Collector zombie running under the `memory.policies.lead_collector` template stores exactly one `core` entry per distinct company across a fixed 10-email corpus (no duplicate keys, no tool-call noise), recalls before outreach on every email, and populates the daily category with scheduled follow-ups for any email where a next-action date is implied.

**Problem:** M14_001 ships neutral primitives (`memory_store`, `memory_recall`). A zombie given neutral primitives will either forget to store (cold-start forever) or store garbage (tool-call payloads, full transcripts, 200 OKs). Both kill the product value of persistent memory. The harness must teach each archetype *what* to store, *when* to recall, and *how* to distill.

**Solution summary:** Ship opinionated memory policy templates per archetype (Lead Collector, Hiring Agent, Ops, Customer Support). Each policy is a declarative skill fragment embedded into the system prompt: key-convention rules, what-to-store rules, what-not-to-store rules, recall-before-action rules, PII redaction rules. Policies are tested with scripted runs over canonical input corpora, asserting stored-entry shape and recall behavior.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `skills/memory/lead_collector.yaml` | CREATE | Lead Collector memory policy |
| `skills/memory/hiring_agent.yaml` | CREATE | Hiring Agent memory policy |
| `skills/memory/ops_zombie.yaml` | CREATE | Ops Zombie memory policy |
| `skills/memory/customer_support.yaml` | CREATE | Customer Support memory policy |
| `src/executor/skill_merge.zig` | MODIFY | Merge memory policy into zombie system prompt at run start |
| `test/fixtures/memory_corpora/` | CREATE | Canonical inputs per archetype for policy tests |

---

## Applicable Rules

- **RULE FLL** — 350-line gate on every touched file
- **RULE HLP** — don't ship a policy template without at least one test consumer
- **RULE CTX** (from M14_001) — redaction policies enforced at policy layer, not just skill author discretion

---

## §1 — Policy Schema

**Status:** PENDING

Define the declarative YAML shape for a memory policy: key conventions, store
rules, recall triggers, redaction rules.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `src/executor/skill_merge.zig:MemoryPolicy` struct | valid YAML | parses into struct with fields `key_patterns`, `store_rules`, `recall_triggers`, `redact_rules` | unit |
| 1.2 | PENDING | YAML schema validation | malformed YAML (missing required field) | validation error names the missing field | unit |

---

## §2 — Four Archetype Policies

**Status:** PENDING

One policy per archetype, shipped as YAML + prompt fragment.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `skills/memory/lead_collector.yaml` | 10-email corpus with 4 distinct companies | zombie stores 4 `core` entries, 1 per company; keys match `lead_{slug}` | integration |
| 2.2 | PENDING | `skills/memory/hiring_agent.yaml` | 5 `#hiring` messages about 2 candidates | zombie stores 2 `candidate_*` entries with stage field populated | integration |
| 2.3 | PENDING | `skills/memory/ops_zombie.yaml` | 3 identical alert payloads 10s apart | zombie creates 1 `incident_sig_*` entry; 2 are deduplicated into counter | integration |
| 2.4 | PENDING | `skills/memory/customer_support.yaml` | message from Pro-plan customer | zombie recalls `customer_*` before drafting; response references plan tier | integration |

---

## §3 — Recall-Before-Action Discipline

**Status:** PENDING

Every archetype policy enforces a recall step before the reasoning step.
Tests assert recall happens, not just that storage happens.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `src/executor/skill_merge.zig` prompt injection | any archetype run | system prompt contains the recall-first directive from the policy | unit |
| 3.2 | PENDING | event log inspection | 10-email run with memory populated | every run has a `memory_recall` tool call before the first action-taking tool call | integration |

---

## Interfaces

**Status:** PENDING

### Policy YAML Shape

```yaml
version: 1
archetype: lead_collector
key_patterns:
  - template: "lead_{company_slug}"
    category: core
    required_from_input: sender_domain
store_rules:
  - when: meaningful_interaction
    category: core
    distill: [contact, preferences, stage, budget, next_action]
    never_store: [raw_email_body, tool_responses]
recall_triggers:
  - before: outreach_action
    query: { key: "lead_{slug_from_input}" }
  - before: scoring
    query: { category: core, tag: workspace }
redact_rules:
  - patterns: [credit_card, ssn, bearer_token]
    action: reject_store
```

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Invalid YAML policy | Executor fails startup loudly | `UZ-POLICY-INVALID` with line number |
| Redact pattern match on store | Store rejected, logged | `UZ-MEM-REDACTED` to agent |
| Missing recall-before-action | Runtime warning, does not block | activity log warning |

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Policy file missing | Archetype configured but YAML absent | Executor errors at startup | Clear error listing missing file |
| Agent ignores recall directive | Prompt drift or model regression | Recall-hit rate metric drops | M14_004 metric alerts |
| Agent stores PII despite redact rule | Pattern didn't match; novel PII shape | Entry stored | Run `zombiectl memory scrub` |

---

## Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Every archetype has ≥ 1 integration test | Grep for `skill.memory.{archetype}` in tests |
| Policy YAML validates at startup (not first-use) | Negative test: invalid YAML → startup fails |
| No policy file > 200 lines | `wc -l skills/memory/*.yaml` |

---

## Invariants (Hard Guardrails)

**Status:** PENDING

N/A — policies are configuration, not compiled Zig. Runtime validation instead.

---

## Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `policy_parse_valid` | 1.1 | skill_merge.zig | valid YAML | parses |
| `policy_parse_missing_field` | 1.2 | skill_merge.zig | missing `key_patterns` | named error |

### Integration Tests

| Test name | Dim | Infra | Input | Expected |
|-----------|-----|-------|-------|----------|
| `lead_collector_10_email_corpus` | 2.1 | memory DB | fixture corpus | 4 core entries, keys match pattern |
| `hiring_stage_tracked` | 2.2 | memory DB | 5 hiring messages | 2 candidate entries with stage field |
| `ops_noise_dedupe` | 2.3 | memory DB | 3 identical alerts | 1 entry + counter=3 |
| `support_plan_recalled` | 2.4 | memory DB | pro-plan message | response cites plan tier |
| `recall_before_action` | 3.2 | memory DB | any archetype run | recall precedes action in event log |

### Spec-Claim Tracing

| Spec claim | Test that proves it | Test type |
|-----------|-------------------|-----------|
| Neutral primitives become opinionated per archetype | Four policy integration tests | integration |
| Zombies always recall before acting | `recall_before_action` | integration |

---

## Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Design policy YAML schema + parser | `policy_parse_valid` passes |
| 2 | Write Lead Collector policy + corpus test | `lead_collector_10_email_corpus` passes |
| 3 | Write Hiring Agent policy + test | `hiring_stage_tracked` passes |
| 4 | Write Ops Zombie policy + test | `ops_noise_dedupe` passes |
| 5 | Write Customer Support policy + test | `support_plan_recalled` passes |
| 6 | Wire prompt injection in `skill_merge.zig` | `recall_before_action` passes |
| 7 | Cross-compile + gate | Eval block PASS |

---

## Acceptance Criteria

**Status:** PENDING

- [ ] All four archetype policies ship with integration tests — verify: `make test-integration | grep memory_policy`
- [ ] Invalid policy YAML fails startup loudly — verify: `make test` (`policy_parse_missing_field`)
- [ ] Recall happens before first action-taking tool call — verify: `recall_before_action` test
- [ ] No policy file > 200 lines — verify: `wc -l skills/memory/*.yaml`

---

## Eval Commands

**Status:** PENDING

```bash
make test 2>&1 | tail -5; echo "test=$?"
make test-integration 2>&1 | grep memory_policy | tail -10
wc -l skills/memory/*.yaml | awk '$1 > 200 { print "OVER: " $2 }'
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

**Status:** PENDING

N/A — greenfield policy files.

---

## Verification Evidence

**Status:** PENDING

Filled in during VERIFY.

---

## Out of Scope

- Policy-driven redaction with PII classifiers (regex only for now)
- Dynamic policy updates without zombie restart
- Policy A/B testing infrastructure
- LLM-assisted policy generation from example corpora
