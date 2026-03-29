# M16_002: Spec Validation and Run Dedup

**Prototype:** v1.0.0
**Milestone:** M16
**Workstream:** 002
**Date:** Mar 28, 2026
**Status:** PENDING
**Priority:** P1 — Spec submissions are not validated before agent execution; duplicate runs waste compute and produce confusing parallel results
**Batch:** B2
**Depends on:** M16_001 (Gate Loop — needs the run submission path)

---

## 1.0 Spec Validation Engine

**Status:** PENDING

On submission, the spec markdown is parsed and validated before a run is created. Validation is structural and referential — not semantic. Three classes of checks run in order: emptiness, actionability, and file reference resolution. Validation errors are printed to stderr and the CLI exits 1, blocking submission. Warnings (ambiguous references) are printed but do not block.

**Dimensions:**
- 1.1 PENDING Reject empty or whitespace-only spec: print `error: spec is empty` to stderr, exit 1
- 1.2 PENDING Reject spec with no actionable content (body consists entirely of comment lines `<!-- ... -->` or markdown headings with no body text): print `error: spec has no actionable content` to stderr, exit 1
- 1.3 PENDING Resolve file references in spec (paths matching `src/`, `pkg/`, or explicit `./` prefixes) against the target repo tree at `base_commit_sha`; any unresolved path is a hard error: print `error: referenced path not found: <path>` to stderr, exit 1
- 1.4 PENDING Ambiguous references (bare filenames without path prefix that match more than one file in the tree) emit `warning: ambiguous reference: <name> — matched <n> paths` to stderr and do not block submission

---

## 2.0 Run Dedup

**Status:** PENDING

A dedup key prevents duplicate in-flight runs for the same spec on the same repository state. The key is derived from the spec content and the exact commit being targeted, making it stable across retries of identical submissions. Terminal runs do not participate in dedup — a spec that previously failed or completed may always be resubmitted.

**Dimensions:**
- 2.1 PENDING Compute dedup key: `sha256(spec_markdown) || repo_full_name || base_commit_sha`; store as `dedup_key` column on the `runs` table (unique index, nullable for legacy rows)
- 2.2 PENDING On run creation, query for an existing run with the same `dedup_key` in a non-terminal state (`PLANNED`, `RUNNING`); if found, return the existing `run_id` with HTTP 200 and header `X-Dedup-Run: existing`; do not create a new row
- 2.3 PENDING Terminal states (`COMPLETED`, `FAILED`, `BLOCKED`) are excluded from dedup lookup — same spec on same base commit can be resubmitted freely after terminal transition
- 2.4 PENDING CLI surfaces dedup hit: print `note: duplicate submission — existing run <run_id> is already in progress` and exit 0

---

## 3.0 Verification Units

**Status:** PENDING

### 3.1 Validation Unit Tests

**Dimensions:**
- 3.1.1 PENDING Unit: empty spec returns exit 1 with expected stderr message
- 3.1.2 PENDING Unit: comment-only spec returns exit 1 with `no actionable content` message
- 3.1.3 PENDING Unit: spec with unresolved file path returns exit 1 naming the missing path
- 3.1.4 PENDING Unit: spec with ambiguous bare filename returns exit 0 with warning on stderr

### 3.2 Dedup Integration Tests

**Dimensions:**
- 3.2.1 PENDING Integration: submitting identical spec + repo + base_commit twice returns same run_id on second call
- 3.2.2 PENDING Integration: submitting same spec after prior run reaches COMPLETED creates a new run (no dedup)
- 3.2.3 PENDING Integration: dedup key column has unique index — direct DB insert of duplicate key fails with constraint error
- 3.2.4 PENDING Integration: `X-Dedup-Run: existing` header present on dedup response, absent on fresh creation

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 Empty spec submission is blocked at CLI before any network call; exit code 1
- [ ] 4.2 Spec with unresolved file reference is blocked; error names the exact unresolved path
- [ ] 4.3 Second identical in-flight submission returns existing run ID, not a new run
- [ ] 4.4 Resubmission after a FAILED run creates a fresh run (dedup does not block)
- [ ] 4.5 All unit and integration tests in §3 pass under `make test`

---

## 5.0 Out of Scope

- Semantic spec analysis (intent detection, goal extraction, NLP)
- Spec schema enforcement (YAML/JSON front-matter validation)
- Spec-to-intent verification (checking whether the spec matches the repo's domain)
- Cross-repo dedup (same spec submitted to different repos always creates independent runs)
