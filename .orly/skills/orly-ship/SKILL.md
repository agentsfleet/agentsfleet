---
name: orly-ship
description: |
  Review, close and ship a built stream: gstack review plus the
  orly-standards-reviewer agent, CHORE(close), `orly gate pr`, the Pull Request
  body, babysitting CI and greptile, and landing after the owner merges. Use on
  "ship it", "open the PR", "close M{N}", "babysit", after any push, or "it merged".
---

# orly-ship

This skill owns the steps from review to land. Pull Request rules live in
`.orly/standards/pr.md`, changelog rules in `.orly/standards/changelog.md`.

## Steps

1. **Check the setup.** Run `orly doctor` and stop on any red line.
2. **Review.** Run gstack `/review` on the diff (`gstack-review` outside Claude
   Code). Then give `orly-standards-reviewer` the diff, the spec, the
   standards for the touched files and the repository's own instructions. Fix
   each finding, or record the owner's verbatim deferral quote.
3. **Baselines.** Copy the baseline counts and run URL from CI on the base
   commit into the Session Notes, beside the branch counts. Measure any lane CI
   did not run on that commit at the merge base, in an isolated checkout; never
   borrow another branch's counts.
4. **Close (CHORE(close)).** Every Dimension is `DONE`. Move the spec to
   `done/` with `Status: DONE`. Write the changelog entry and the docs pages the
   change affects, sweep orphans, and leave `git status -uall` empty.
5. **Push.** Push the branch. The gate checks that `HEAD` is the pushed
   revision, so it runs after this.
6. **Gate.** Run `orly gate pr` once on the final pushed tree. With a reviewed
   hermetic policy, `--reuse-verification` may reuse valid matching completion
   evidence; keep the original run, counts and provenance in the Pull
   Request. Every required criterion passes or carries an owner-recorded override.
7. **Open the Pull Request.** Open it with a body that follows the PR standard.
   This needs an explicit ask, or auto mode with a start instruction.
8. **Babysit.** Poll CI checks and greptile threads, fix what this diff broke,
   and answer every thread. Stop after two consecutive quiet polls with CI
   green. Never use `gh pr checks --watch`.
9. **Land.** After the owner merges, pull the default branch. Delete the merged
   branch with `git branch -d` and remove its worktree without `--force`.

## Codex cloud tasks

Follow the user's explicit publishing authorization in cloud runtimes. Opening the Pull Request includes the feature-branch push and required final gate.
It does not authorize merge, release or a consumer pin update.

When publishing is not authorized, stop with concrete prepared changes and verification evidence for the owner.
