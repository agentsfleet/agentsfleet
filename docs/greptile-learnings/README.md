# Greptile Learnings

Generic principles learned from greptile reviews, PR feedback, and production incidents.

## File

`RULES.md` — short, generic rules. Each rule covers a class of bugs, not a single instance.

## When agents read RULES.md

1. **EXECUTE phase start** — before writing any code
2. **`/review` skill** — before reviewing a diff
3. **Greptile fix workflow** — after fixing findings, check if a rule needs adding

## Adding a rule

**Before adding:** Check if an existing rule already covers the finding. Most findings
are instances of universal principles (unused code, missing parser, double-free, etc.).
If a generic rule exists, just append the incident reference to it. Only create a new
rule when the principle is genuinely novel.

Rules are generic principles, not per-incident entries:

```markdown
## N. Short principle name

One-paragraph rule in plain English. No code blocks unless the pattern
is truly non-obvious. Keep it under 5 lines.

> Incident references: MNN_NNN short description.
```

## Post-PR greptile workflow

1. Fetch greptile review comments.
2. Fix each finding (P0/P1 required; P2 at discretion).
3. Run `make lint && make test`.
4. For each finding: check if an existing rule in `RULES.md` covers it.
   - **Yes** → append the incident reference (`> MNN_NNN: description`). Done.
   - **No** → write a new generic rule that covers the class of bug, not just this instance.
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
