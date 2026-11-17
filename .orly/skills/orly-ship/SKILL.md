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
   Fetch the target base branch and record its revision. Before publishing,
   ensure this branch contains that revision and reconcile release versions against it.
2. **Review.** Run gstack `/review` on the diff (`gstack-review` outside Claude
   Code). Then give `orly-standards-reviewer` the diff, the spec, the
   standards for the touched files and the repository's own instructions. Fix
   each finding, or record the owner's verbatim deferral quote.
3. **Baselines.** Copy the baseline counts and run URL from CI on the base
   commit into the Session Notes, beside the branch counts. Measure any lane CI
   did not run on that commit at the merge base, in an isolated checkout; never
   borrow another branch's counts.
4. **Close and commit (CHORE(close)).** Verify every Dimension against its
   existing acceptance criteria, then move the completed spec to `done/` with
   `Status: DONE`. Commit it with the implementation, changelog and affected
   docs, sweep stale references, and verify `git status --porcelain=v1 -uall`
   is empty.

   Pending CI does not create unfinished implementation scope. Record failed
   verification separately; do not rewrite acceptance criteria to manufacture
   completion or invent an owner-approved deferral.
5. **Push.** Push the branch. The gate checks that `HEAD` is the pushed
   revision, so it runs after this.
6. **Gate.** Run `orly gate pr` once on the final pushed tree. With a reviewed
   hermetic policy, `--reuse-verification` may reuse valid matching completion
   evidence; keep the original run, counts and provenance in the Pull
   Request. Every required criterion passes or carries an owner-recorded override.
7. **Publish for review.** An authorized request to open a PR targets
   ready-for-review unless the owner explicitly requests a draft. Open it with
   a body that follows the PR standard. If this stream already has a draft,
   mark that same PR ready for review; do not leave a temporary draft behind.

   Read back its live draft flag and head revision after the mutation.
   Review readiness does not waive checks or authorize merging.
8. **Babysit.** Poll CI checks and greptile threads, fix what this diff broke,
   and answer every thread. Stop after two consecutive quiet polls with CI
   green. Never use `gh pr checks --watch`.
9. **Verify the handoff.** Read the final state from Git, the committed spec
   and the forge before reporting completion:

   - the completed spec exists in `done/`, says `DONE`, has every Dimension
     complete, and has no stale active copy or current plan link;
   - the tree is clean and local HEAD, upstream and the PR head agree;
   - the branch contains the freshly fetched target base, and any release version follows that base;
   - the PR is ready for review, or carries the owner's explicit draft request;
   - required companion PRs exist, are cross-linked, and each has its state verified;
   - gate, CI and review outcomes name this revision, including required
     pending, failed or unavailable checks.
   - review threads are triaged and answered, with findings introduced by the
     diff fixed; a green review check does not prove its threads are clear.

   For GitHub, inspect `gh pr view --json isDraft,headRefOid,statusCheckRollup,url`;
   unless a draft was requested, require the live-state assertion to exit 0:

   ```bash
   gh pr view "$pr_url" --json isDraft,headRefOid,url \
     --jq 'if .isDraft then error("PR is still draft") else . end'
   ```

   Expected output:

   ```text
   {"headRefOid":"<varies>","isDraft":false,"url":"<varies>"}
   ```

   Set `pr_url` from the published PR result; use equivalent live metadata on
   other forges. Do not infer readiness from a
   successful push, a closed spec or an earlier PR snapshot.

   A pending check keeps follow-up running. A blocker report names the exact
   failing check, evidence and required action; it is not a completion claim.
10. **Land.** After the owner merges, pull the default branch. Delete the merged
   branch with `git branch -d` and remove its worktree without `--force`.

## Codex cloud tasks

Follow the user's explicit publishing authorization in cloud runtimes. Opening the Pull Request includes the feature-branch push and required final gate.
It does not authorize merge, release or a consumer pin update.

When publishing is not authorized, stop with concrete prepared changes and verification evidence for the owner.
