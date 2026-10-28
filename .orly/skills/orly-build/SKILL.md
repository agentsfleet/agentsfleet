---
name: orly-build
description: |
  Open a spec's stream and build it Section by Section: move the spec to
  active/, cut the branch, read the standards for the files each Section
  touches, write code and tests, and commit through `orly gate work`. Use on
  "start M{N}", "implement §2", "build this spec", or "continue M{N}".
---

# orly-build

This skill owns the steps from CHORE(open) to the last Section commit. The rules
it applies are the standards listed in `.orly/AGENTS.md`; tests follow
`.orly/standards/tests.md`.

## Steps

1. **Check the setup.** Run `orly doctor` and stop on any red line.
2. **Open the stream (CHORE(open)).** Fetch `origin/main`. A product repository
   gets a worktree on `feat/m{N}-<slug>`; a rules repository cuts the branch in
   its main checkout.
3. **Record the opening.** Move the spec to `active/` and set
   `Status: IN_PROGRESS`, `Branch:`, the full `Baseline revision:` and a pending
   `Test Baseline:`. Commit before any code.
4. **Plan.** Restate the intent and list `ASSUMPTIONS I'M MAKING`. A mismatch
   with the spec's Intent stops the work. Keep the restatement for the Pull
   Request Session Notes.
5. **Build each Section.** Read the standards for the files the Section touches.
   Write a test per Dimension and a negative test per failure mode. Add an
   integration test when the diff crosses a module boundary with real
   input/output.
6. **Commit.** The pre-commit hook runs `orly gate work`; if hooks did not run,
   run it yourself. Mark the Section's Dimensions `DONE` in the same commit as
   their code and tests. Never use `--no-verify`.
7. **When stuck.** Use gstack `/investigate` for a failure you cannot explain,
   and gstack `/qa` to exercise a user-facing path. Outside Claude Code their
   names carry a `gstack-` prefix.
8. **Hand off.** When every Dimension is `DONE`, continue with `orly-ship`.

## Codex cloud tasks

A cloud task may run without Git hooks and pushes nothing. Run `orly gate work`
and `orly gate verify` yourself and stop before `orly gate pr`. CI runs the
heavy lanes on the Pull Request.
