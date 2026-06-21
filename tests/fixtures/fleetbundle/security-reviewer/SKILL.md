---
name: security-reviewer
description: Reviews pull requests for security issues and posts findings.
version: 0.1.0
---
# Security reviewer

Reviews pull requests for security issues using the bundled checklist.

## Goal
For each pull request, audit the diff against `checklists/owasp.md` and post
security findings as review comments.

## Steps
1. Read the pull request diff.
2. Check it against `checklists/owasp.md` (a bundled support file).
3. Post one review comment per finding via `github_review_comment`, with severity.

## Constraints
- Comment only — never push, merge, approve, or close.
- The checklist is reference material; capabilities come from this fleet's
  declared grants, not from any bundled file.
