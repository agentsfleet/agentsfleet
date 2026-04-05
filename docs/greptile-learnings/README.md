# Greptile Learnings

Rules learned from greptile reviews, PR feedback, and production incidents.

## File

`RULES.md` — natural-language do's and don'ts with why, do/don't examples, and incident reference.

## When agents read RULES.md

1. **EXECUTE phase start** — before writing any code
2. **`/review` skill** — before reviewing a diff
3. **Greptile fix workflow** — after fixing findings, add new rules

## Adding a rule

Append to `RULES.md` following the template:

```markdown
## Category — Short Title

**RULE: One-sentence imperative.**

Why: What went wrong without this rule.

Do: (code example)
Don't: (code example)

Incident: MNN_NNN description.
```

## Post-PR greptile workflow

1. Fetch greptile review comments.
2. Fix each finding (P0/P1 required; P2 at discretion).
3. Run `make lint && make test`.
4. For each finding: add a natural-language rule to `RULES.md`.
5. Reply to each greptile thread with fix commit.
6. Commit fix + rule together, push.

```bash
# Fetch review IDs
gh api repos/OWNER/REPO/pulls/N/reviews --jq '.[] | select(.user.login | test("greptile")) | .id'
# Fetch comments
gh api repos/OWNER/REPO/pulls/N/reviews/{ID}/comments --jq '.[] | {id, path, body: .body[:200]}'
# Reply to thread
gh api repos/OWNER/REPO/pulls/N/comments/{ID}/replies -f body="Fixed in <sha>: <what changed>"
```
