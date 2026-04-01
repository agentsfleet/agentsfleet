# Greptile Learnings

Agent-first. One file: `.greptile-patterns`. No category files.

## How it works

`make lint` scans the current diff against every regex in `.greptile-patterns`.
A match fails the lint gate with the anti-pattern shown.

## Pre-PR (automatic)

`make lint` runs `_greptile_patterns_check` which scans `git diff origin/main` additions against `.greptile-patterns`. No separate step needed.

## Post-PR — when asked to "resolve greptile on PR #N"

1. Fetch review ID and inline comments:
   ```bash
   gh api repos/OWNER/REPO/pulls/N/reviews | python3 -c "import sys,json; [print(r['id']) for r in json.load(sys.stdin) if 'greptile' in r['user']['login']]"
   gh api repos/OWNER/REPO/pulls/N/reviews/{ID}/comments
   ```
2. Fix each finding in the worktree (P0/P1 required; P2 at discretion).
3. Run `make lint && make test` and `make test-integration-db` if DB-backed files were touched.
4. For every P0/P1 finding: derive a grep-E regex and append to `.greptile-patterns`. **IMPORTANT**: Ensure the pattern does NOT match additions to `.greptile-patterns` itself (self-matching). Verify before committing.
5. Verify the new pattern matches the bad example and not the fix: `echo 'bad' | grep -Ef .greptile-patterns`
6. Reply to each greptile thread: `gh api repos/OWNER/REPO/pulls/N/comments/{comment_id}/replies -f body="..."` — state what was fixed and which commit.
7. Confirm all threads have replies: re-fetch comments and check.
8. Commit fix + pattern append together, push the branch.
9. Report: list each finding, severity, fix applied, pattern added (or why not), and thread reply ID.

## Adding a pattern

Append one grep-E regex per line to `.greptile-patterns`. No labels, no comments.
The regex must match the *diff line* (`+` prefix lines) that represents the anti-pattern.

**WARNING**: Patterns that match generic text (like paths) may match themselves when added to the patterns file. Test with `make lint` before committing.

## VERIFY

Run `make lint` — `_greptile_patterns_check` is wired in and handles the scan.
