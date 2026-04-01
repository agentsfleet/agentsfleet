# Greptile Learnings

Agent-first. One file: `.greptile-patterns`. No category files.

## How it works

`make lint` scans the current diff against every regex in `.greptile-patterns`.
A match fails the lint gate with the anti-pattern shown.

## When asked to resolve greptile PR comments

1. Fetch inline comments: `gh api repos/OWNER/REPO/pulls/N/reviews/ID/comments`
2. Fix each finding in the worktree.
3. For every P0/P1 finding: derive a grep-E regex for the anti-pattern and append it to `.greptile-patterns`.
4. Verify the new pattern matches the bad example and not the fix: `echo 'bad' | grep -Ef .greptile-patterns`
5. Commit fix + pattern update together.

## Adding a pattern

Append one grep-E regex per line to `.greptile-patterns`. No labels, no comments.
The regex must match the *diff line* (`+` prefix lines) that represents the anti-pattern.

## VERIFY

Run `make lint` — `_greptile_patterns_check` is wired in and handles the scan.
