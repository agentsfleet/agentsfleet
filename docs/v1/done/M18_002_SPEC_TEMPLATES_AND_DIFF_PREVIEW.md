# M18_002: Spec Templates and Run Diff Preview

**Prototype:** v1.0.0
**Milestone:** M18
**Workstream:** 002
**Date:** Mar 28, 2026
**Status:** DONE
**Branch:** m18-002-spec-templates-diff-preview
**Priority:** P2 — Lowers friction of creating specs and provides impact visibility before running agents
**Batch:** B4
**Depends on:** M16_001 (Gate Loop)

---

## 1.0 Spec Template Generator

**Status:** DONE

`zombiectl spec init` scans the target repo and generates a markdown spec template. Detection covers: language (from file extensions), available make targets (parse Makefile), existing test patterns (test file naming), and project structure (`src/`, `tests/`, `docs/`). The generated template includes frontmatter placeholders, section stubs for what to implement, detected gates (which make targets exist), and a suggested scope block derived from the scan output.

**Dimensions:**
- 1.1 ✅ Scan repo for language (file extensions), make targets (Makefile parse), test patterns (test file naming), and project structure (`src/`, `tests/`, `docs/`)
- 1.2 ✅ Generate markdown spec template with detected context populated into frontmatter placeholders, section stubs, and detected gates
- 1.3 ✅ Write template to specified output path (default: `docs/spec/new-feature.md`); print path on success
- 1.4 ✅ Handle edge cases: no Makefile present, monorepo with multiple languages detected, empty or near-empty repo

---

## 2.0 Run Diff Preview

**Status:** DONE

Before the agent runs, `zombiectl run --spec <file> --preview` shows predicted file impact. The algorithm extracts file paths and identifiers from spec markdown (regex for `src/`, known module names, quoted file references), then matches them against the repo file tree via substring. The result is displayed as a list of likely-modified files with a confidence indicator. The `--preview` flag is non-blocking by default — it prints the prediction and proceeds with the run unless `--preview-only` is also set.

**Dimensions:**
- 2.1 ✅ Parse spec markdown for file path references and identifiers (regex: `src/` prefixes, quoted paths, known module names)
- 2.2 ✅ Match extracted terms against repo file tree (local walk) using substring matching
- 2.3 ✅ Display predicted impact as a formatted list with a per-entry confidence indicator (high / medium / low based on match specificity)
- 2.4 ✅ `--preview` flag is non-blocking: prints prediction then proceeds with run; `--preview-only` halts after printing

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 `zombiectl spec init` on a Go repo with a Makefile produces a template that includes detected make targets under the gates section
- [x] 3.2 `zombiectl spec init` on a repo with no Makefile produces a valid template with an empty gates section and no error exit
- [x] 3.3 `zombiectl run --spec <file> --preview` prints a predicted file impact list before starting the run
- [x] 3.4 `zombiectl run --spec <file> --preview-only` prints the prediction and exits without triggering a run

---

## 4.0 Out of Scope

- Semantic spec analysis (understanding intent beyond string matching)
- AI-powered impact prediction (LLM-assisted file matching)
- Spec scoring or quality rating
- Automatic spec validation against existing workstream schemas
