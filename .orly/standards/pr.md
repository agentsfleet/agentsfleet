# Pull requests

This standard covers a Pull Request (PR) from branch to merge: its base, what it carries when it opens, its description and its follow-up.
A coding agent reads it before opening or updating a PR, and a reviewer judges the PR against it.
GitLab merge requests follow the same rules.

## Branch and commits

### PR-BASE — A branch is cut from a freshly fetched `origin/main`, and review of it starts at `git log origin/main..HEAD`.
- **Applies to:** every new branch, and every review of a branch
- **Checked by:** review
- **Bad:** `git switch -c feat/x main` on a local `main` last pulled days ago
- **Good:** `git fetch origin && git switch -c feat/x origin/main`, then `git log origin/main..HEAD` before review
- **Why:** a stale base mixes other people's commits into the range, and review reads changes the branch never made.

### PR-BRANCH — Work for a PR happens on its own branch, never on the default branch.
- **Applies to:** every commit meant for a PR
- **Checked by:** gate: git.branch
- **Bad:** committing a fix on `main` and opening the PR from there
- **Good:** `git switch -c fix/empty-name origin/main` before the first commit
- **Why:** the default branch changes only through a merged PR.

### PR-PATHS — A commit made while the index is shared names its paths.
- **Applies to:** commits in a checkout where another agent or process also stages files
- **Checked by:** review
- **Bad:** `git commit -m "fix: refuse an empty name"`, which also commits whatever another agent staged
- **Good:** `git commit -m "fix: refuse an empty name" -- src/name.ts src/name.test.ts`
- **Why:** a bare commit takes the whole shared index, including another agent's unfinished work.

### PR-CURRENT — A branch contains the current `origin/main` HEAD before its PR opens, and an open PR's branch is never force-pushed.
- **Applies to:** every branch at the PR boundary and after it
- **Checked by:** review
- **Bad:** `git push --force-with-lease` after rebasing a branch whose PR is open
- **Good:** a rebase before the first push; after it, `git merge origin/main` and a plain push
- **Why:** a force-push rewrites history that reviewers have already commented on.

### PR-BUDGET — A milestone ships in one ready PR plus at most one follow-up.
- **Applies to:** every milestone, and scope found while its PR is open
- **Checked by:** review
- **Bad:** a third PR for scope found during review
- **Good:** the new scope folded into the open PR, with the spec moved back from `done/` to `active/` if needed
- **Why:** each extra PR splits the review context and repeats the boundary checks.

## Contents at open

### CHR — A PR leaves no reference outside history to any symbol, path or command it renamed or deleted.
- **Applies to:** every PR that renames, moves or deletes something
- **Checked by:** review
- **Bad:** fix commits after the merge for stale references a final sweep would have found
- **Good:** a repository-wide search for each old name, before the PR opens, that finds only changelogs and finished specs
- **Why:** the PR that changes a symbol owns its cleanup, and deferred orphans compound across PRs.

### PR-TREE — `git status -uall` prints nothing after the last commit, before the PR opens or updates.
- **Applies to:** the checkout a PR is opened or updated from
- **Checked by:** gate: git.tree
- **Bad:** a PR opened with an untracked `notes.txt` and a lockfile a hook rewrote left unstaged
- **Good:** every changed, untracked and hook-written file in the closing commit
- **Why:** a file left out is work the PR appears to include but does not.

### PR-PUSHED — The PR's head is the exact revision that was verified, reviewed and pushed.
- **Applies to:** every PR at opening and after every update
- **Checked by:** gate: git.pushed
- **Bad:** a local fix commit made after the gate ran and never pushed
- **Good:** `HEAD` equal to `@{upstream}` when the PR opens or updates
- **Why:** review and Continuous Integration (CI) judge the pushed revision, so a local-only commit ships unreviewed.

### PR-SPEC — A finished spec sits in `docs/v*/done/` in the PR's diff with every Dimension `DONE`, and a parked spec stays in `active/` as `IN_PROGRESS`.
- **Applies to:** PRs on a branch that carries a spec
- **Checked by:** review
- **Bad:** `Status: DONE` on a spec still under `docs/v2/active/`
- **Good:** the spec moved to `docs/v2/done/` in the closing commit, with `Status: DONE`
- **Why:** the spec's folder tells the next agent whether the work is finished.

### PR-VERSION — The repository's version-sync check passes before the PR opens.
- **Applies to:** repositories that declare `verify.version`
- **Checked by:** gate: cmd.verify.version
- **Bad:** `package.json` at `0.26.0` while `VERSION` still reads `0.25.0`
- **Good:** every declared version file agrees, and `make check-version` exits 0
- **Why:** mismatched version files ship a release that reports the wrong number.

### PR-HANDOFF — No handoff file reaches a PR.
- **Applies to:** `HANDOFF*.md` and `handoff*.md` at any depth
- **Checked by:** review
- **Bad:** `docs/HANDOFF_OCT_28.md` committed in the closing commit
- **Good:** the handoff deleted, and its open items moved into Session notes or the spec
- **Why:** a handoff briefs the next agent and goes stale once the PR records the state.

### PR-DEFER — A deferral claimed in a PR carries the owner's verbatim quote naming the item.
- **Applies to:** any item a PR body, Session notes, handoff or chat calls deferred to a follow-up
- **Checked by:** review
- **Bad:** `Dimension 2.1 deferred to follow-up` with no quote from the owner
- **Good:** `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<verbatim quote>" — context: <item, why>` in Session notes or the spec's Discovery
- **Why:** a deferral only the agent decided is unfinished scope, not a decision.

A pickup agent treats a deferred item without a quote as in scope and raises the contradiction with the owner first.

## Description

### PR-UNIT — Each working session appends exactly one numbered unit to the PR body after review and verification: `## Session notes <N>`, `## Review` and `## Make`.
- **Applies to:** every PR or merge request body update, through `gh`, `glab` or any helper
- **Checked by:** review
- **Bad:** rewriting `## Session notes 2` into a combined summary, or appending a second `## Session notes 2`
- **Good:** `## Session notes 3` appended in one body update, with `N` read from the forge body just before the write
- **Why:** append-only numbered units let the next agent see what each session did.

Earlier units are never renumbered, rewritten, reordered or merged, and text before the first unit is kept.

### PR-NOTES — Session notes open with two or three plain lines that lead with what was fixed, then the decisions and open deferrals.
- **Applies to:** every `## Session notes <N>` section
- **Checked by:** review
- **Bad:** `This session we looked into the failing tests and made several improvements.`
- **Good:** `Fixed the tenant-scope property: it now fails with the scope clause deleted. The load-test rewrite is deferred, quoted below.`
- **Why:** the next agent needs the outcome and the open decisions before the detail.

The closing session's notes also record assumptions and dead ends.

### PR-DIAGRAM — A diagram in a PR body sits in a fence, fits in 78 columns, labels its axes and units, and shows measured values.
- **Applies to:** text diagrams in PR and merge request bodies
- **Checked by:** review
- **Bad:** a bar chart with illustrative numbers, or a box that repeats its caption
- **Good:** a fenced before/after of `671 tests PASS` against `scope property FAIL`, copied from the real run
- **Why:** an invented number in a picture reads as evidence.

### PR-VISUAL — A change to a surface people look at carries visual evidence from a run that already happened, retrievable without the image.
- **Applies to:** PRs that change rendered pages, dialogs or terminal output, and failures a picture explains
- **Checked by:** review
- **Bad:** `see screenshot`, with no image attached and no run named
- **Good:** the run id, the downloaded file and its path, and the decisive line quoted, such as `error-context.md:27 records unexpected value "hidden"`
- **Why:** a reviewer who sees the state decides in seconds what prose makes them reconstruct.

One image carries one assertion, named in its caption, and no failure is cropped out of its frame.
A countable before/after belongs in a fenced diagram, not in two images.
The person attaches images, because no forge command-line tool uploads them to a PR body.

### PR-REVIEW — The Review section leads with the result and lists every finding with severity, location, defect and disposition.
- **Applies to:** every `## Review` section
- **Checked by:** review
- **Bad:** an empty `## Review` heading, or `Looks good` when no review ran
- **Good:** `P1 query_load_test.rs:183 only assertion was arithmetic. FIXED`, or `No findings.`, or `Review not run — reviewer unavailable.`
- **Why:** a missing or vague review reads as clean when it may not be.

Severity is P0, P1 or P2, and the disposition is `FIXED` or `DEFERRED` with the quote PR-DEFER requires.

### PR-MAKE — The Make section lists each command actually run, with its exit status and decisive output line.
- **Applies to:** every `## Make` section
- **Checked by:** review
- **Bad:** `Tests pass.`
- **Good:** `make test-unit-all  exit 0  860 passed; 0 failed  (baseline 846, +14)`
- **Why:** a claim without its command and output cannot be checked.

A command that could not run is named with its reason, and a local run is labelled local, never shown as a green pipeline.
The section also records the outcome of the review and of the post-push follow-up, or why either could not run.

### PR-CLAIMS — Every factual claim in a PR body traces to a command output line or a named measurement, and an unchecked one opens with `Unverified:`.
- **Applies to:** the whole PR or merge request body
- **Checked by:** review
- **Bad:** `This removes every stale reference.` with no search shown
- **Good:** `git grep -n -w old_name` with its empty output in Make, or `Unverified: Codex loads the skill — a cloud task transcript would settle it.`
- **Why:** a reviewer can trust only what the body lets them check.

Each section leads with its answer, with no marketing adjectives, throat-clearing opener or closing recap.

## Follow-up

### PR-BABYSIT — A pushed PR is followed until two consecutive quiet polls with CI green on the same pushed revision.
- **Applies to:** every push to a branch with an open PR or merge request
- **Checked by:** review
- **Bad:** stopping after one quiet poll while a check is pending, or `gh pr checks --watch` to wait for the review bot
- **Good:** polling CI checks, the bot's inline threads and its PR-level summary on a backoff, stopping after two quiet polls with CI green
- **Why:** review bots and CI report asynchronously, so findings that arrive late get missed.

CI red from the diff is fixed and pushed; CI red from elsewhere goes to the owner.
A fetch failure, a disputed finding or a new revision restarts the quiet count, and a review not yet posted is pending, not quiet.
A finding in code the agent did not write goes to the owner before any fix.
