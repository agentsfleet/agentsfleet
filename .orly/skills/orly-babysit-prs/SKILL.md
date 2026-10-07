---
name: orly-babysit-prs
description: |
  Post-push follow-up for a Pull Request or Merge Request: poll Continuous
  Integration (CI) checks, greptile threads, and the summary; fix what your diff
  broke; stop after two empty polls with CI green. Forge-aware (gh, glab). Use
  after creating or pushing to a PR/MR, or on "babysit", "watch reviews".
---

# orly-babysit-prs

At invocation start, record the consent-gated usage signal without blocking the skill:

```bash
command -v orly >/dev/null 2>&1 && ORLY_INVOCATION=skill orly skill-event orly-babysit-prs >/dev/null 2>&1 || true
```

Greptile reviews on GitHub and GitLab
post auto-reviews **asynchronously**. They land as PR review comments / MR
discussion threads, not check runs, so `gh pr checks --watch` (or the glab
equivalent) doesn't observe them. Without explicit polling on a backoff
cadence, findings arrive after the human stops checking — so they get missed.

This skill is the **polling cadence + walk-every-review-thread loop**. The
fetch, classify, and reply mechanics live in gstack's `greptile-triage.md` —
open and follow it; do not paraphrase from memory. This skill never
duplicates the triage helper's logic; it adds (a) the cadence, (b) the
multi-thread walk, and (c) the **forge abstraction** so the same loop runs on
GitHub and GitLab.

## STEP 0 — Open greptile-triage.md every cycle

Before doing anything else in a polling cycle, read the triage helper:

```bash
GREPTILE_TRIAGE="$HOME/.local/share/gstack/review/greptile-triage.md"
[ -r "$GREPTILE_TRIAGE" ] || { echo "BABYSIT: greptile-triage.md missing — abort cycle"; exit 1; }
```

Then walk it section by section:

| Triage section | What this skill triggers |
|---|---|
| `## Fetch` | Run per detected review thread (multi-thread loop below), via the forge layer |
| `## Suppressions Check` | Read history as context; a prior `fp` label never suppresses a current finding. Dismissal requires the owner's item-specific decision and recorded reason. |
| `## Classify` | Read the finding's code and evidence; classify actionable, already fixed or disputed. A disputed finding stays open for the owner. |
| `## Reply APIs` | Tier 1 first response, Tier 2 on re-flag — via the forge layer |
| `## Reply Templates` | Use Tier 1 / Tier 2 template verbatim — do not invent new wording |
| `## Severity Assessment & Re-ranking` | Re-rank the bot's severity against the project's actual risk profile |
| `## History File Writes` | Append to BOTH per-project and global history files (see below) |
| `## Output Format` | Use the triage helper's report shape, augmented with our cadence line |

If the triage helper is missing on the host, abort with `BABYSIT: greptile-triage.md missing` rather than guessing — the abort is visible, a silent re-implementation is not.

## STEP 0.5 — Detect the forge

`git remote` decides which CLI + review-object model to use. Everything
downstream branches on `$FORGE`.

```bash
ORIGIN=$(git remote get-url origin 2>/dev/null)
case "$ORIGIN" in
  *github.com*)  FORGE=github ;;
  *)             FORGE=gitlab ;;   # gitlab.com OR self-hosted (e.g. awakeninggit.e2enetworks.net)
esac
echo "BABYSIT: forge=$FORGE origin=$ORIGIN"
```

| Concept | GitHub (`gh`) | GitLab (`glab`) |
|---|---|---|
| Change unit | Pull Request (PR) | Merge Request (MR) |
| Create cmd | `gh pr create` | `glab mr create` |
| Review object | a *review* with line comments | a *discussion* (thread) with notes |
| "walk every…" | every review id | every discussion thread id |
| Reply | reply to the review comment | post a note to the discussion |

Both forges filter bot authors by a case-insensitive `greptile` match.
Completion, review threads and summaries use that same selection.

## STEP 0.75 — CI checks must go green (poll + fix)

Greptile is not the only async signal: the PR/MR's **CI check runs** land on
their own schedule and are part of "done". The loop is not finished until CI is
green **and** the review bot is quiet. This is the one place `gh pr checks`
(NOT `--watch`) is the right tool — it observes *check runs*, which is exactly
what CI is. (The "never `gh pr checks --watch` for greptile" rule stands:
greptile posts review comments, not checks, so `pr checks` never sees it. CI
jobs are the opposite — `pr checks` is precisely how you see them.)

Each cycle, after the review-thread walk, poll CI and act by cause:

### GitHub

```bash
set -euo pipefail
CHECKS=$(gh pr checks "$PR_NUMBER" --json name,state,bucket,link) \
  || { echo "BABYSIT: check fetch incomplete; reset quiet polls" >&2; exit 1; }
printf '%s' "$CHECKS" | jq -e 'type == "array" and length > 0 and all(.[]; .bucket == "pass" or .bucket == "skipping")' >/dev/null \
  || { echo "BABYSIT: checks empty or not green; reset quiet polls" >&2; exit 1; }
CHECK_HEAD=$(gh pr view "$PR_NUMBER" --json headRefOid --jq '.headRefOid')
[ "$CHECK_HEAD" = "$POLL_HEAD" ] \
  || { echo "BABYSIT: revision changed during checks; reset quiet polls" >&2; exit 1; }
echo "BABYSIT: ci=green revision=$CHECK_HEAD"
```

### GitLab

```bash
set -euo pipefail
PIPELINES=$(glab api "projects/$PROJ_ENC/merge_requests/$MR_IID/pipelines" --paginate | jq -sc 'add') \
  || { echo "BABYSIT: pipeline fetch incomplete; reset quiet polls" >&2; exit 1; }
printf '%s' "$PIPELINES" | jq -e --arg revision "$POLL_HEAD" \
  '[.[] | select(.sha == $revision)] | sort_by(.id) | last | .status == "success"' >/dev/null \
  || { echo "BABYSIT: current revision has no successful pipeline; reset quiet polls" >&2; exit 1; }
CHECK_HEAD=$(glab mr view -F json | jq -er '.sha // .diff_refs.head_sha')
[ "$CHECK_HEAD" = "$POLL_HEAD" ] \
  || { echo "BABYSIT: revision changed during check fetch; reset quiet polls" >&2; exit 1; }
printf 'ci=green revision=%s\n' "$CHECK_HEAD"
```

| CI state | Action |
|---|---|
| nonempty, completely fetched checks all `pass`/`skipping` (GitLab current-revision `success`) | Record `ci=green` for the unchanged pushed revision. |
| fetch failed, malformed response, or no checks found | Record `ci=incomplete`; reset quiet polls and retry. An empty response is never green. |
| any `pending`/`running` | Record `ci=pending`; re-poll on the cadence. **Never declare done while a check is still running.** |
| any `fail`/`cancel` **from your changes** | In-scope finding you OWN — treat like a greptile fix. Fetch the failing log (`gh run view <run-id> --log-failed`, run id from the check `link`; GitLab `glab ci trace <job-id>`), fix the code, re-verify with the relevant `make` target, commit, push, re-poll. |
| any `fail` **NOT from your changes** (pre-existing red, infra flake, unrelated job) | Surface to the user with job name + link. Do **not** fix blindly, do **not** edit CI config. Pause the done-declaration until they decide. |

CI failures caused by your commit are not "surface and wait" — they are the
same repair duty as a greptile P0: diagnose, fix the code (never the CI config),
push, and re-poll until green.

## Triggers

- After `gh pr create` / `glab mr create` for a feature branch.
- After every `git push` to a branch with an open PR/MR.
- User says: "babysit", "watch greptile", "poll the PR", "poll the MR",
  "watch reviews", "follow up on review feedback", "what did greptile say".

## Output rules

Per cycle, print one line (`PR` on GitHub, `MR` on GitLab):

```
BABYSIT <PR|MR> #<n> @ <SHA>: poll <i> | reviews=<count> | new=<m> | actioned=<k> | ci=<green|pending|red> | next=<delay>
```

`actioned` = findings whose code fix landed in this cycle (greptile OR CI).
`ci` = the aggregate check-run state from STEP 0.75.

## Cadence (backoff)

| Time since last push (or last review) | Poll interval |
|---|---|
| 0–10 min | +180 s after each push |
| 10–30 min | +300 s |
| 30–60 min | +600 s |
| > 60 min | +1200 s, then stop after 2 consecutive empty polls |

Claude Code: use `ScheduleWakeup(delaySeconds, "re-poll review on <PR|MR>
#<n> <SHA>")`. Other agents: `sleep <delay>` in a shell loop, or schedule
via the agent's native cron/wakeup mechanism.

## Polling loop

### GitHub (`FORGE=github`)

```bash
set -euo pipefail
REPO=$(gh repo view --json nameWithOwner --jq '.nameWithOwner')
PR_NUMBER=$(gh pr view --json number --jq '.number')
POLL_HEAD=$(gh pr view "$PR_NUMBER" --json headRefOid --jq '.headRefOid')
[ "$POLL_HEAD" = "$(git rev-parse HEAD)" ] \
  || { echo "BABYSIT: local and pushed revisions differ; reset quiet polls" >&2; exit 1; }

REVIEWS=$(gh api "repos/$REPO/pulls/$PR_NUMBER/reviews" --paginate --slurp | jq -c 'add')
COMMENTS=$(gh api "repos/$REPO/pulls/$PR_NUMBER/comments" --paginate --slurp | jq -c 'add')
SUMMARIES=$(gh api "repos/$REPO/issues/$PR_NUMBER/comments" --paginate --slurp | jq -c 'add')
for payload in "$REVIEWS" "$COMMENTS" "$SUMMARIES"; do
  printf '%s' "$payload" | jq -e 'type == "array" and all(.[]; (.id | type == "number") and (.user.login | type == "string"))' >/dev/null \
    || { echo "BABYSIT: malformed review response; reset quiet polls" >&2; exit 1; }
done
printf '%s' "$REVIEWS" | jq -e --arg revision "$POLL_HEAD" --arg bot "greptile" \
  'any(.[]; (.user.login | test($bot; "i")) and .commit_id == $revision and (.state == "COMMENTED" or .state == "APPROVED" or .state == "CHANGES_REQUESTED"))' >/dev/null \
  || { echo "BABYSIT: current revision review pending; reset quiet polls" >&2; exit 1; }
END_HEAD=$(gh pr view "$PR_NUMBER" --json headRefOid --jq '.headRefOid')
[ "$POLL_HEAD" = "$END_HEAD" ] \
  || { echo "BABYSIT: revision changed during review fetch; reset quiet polls" >&2; exit 1; }
jq -n --arg revision "$POLL_HEAD" --argjson reviews "$REVIEWS" \
  --argjson comments "$COMMENTS" --argjson summaries "$SUMMARIES" \
  '{revision:$revision,reviews:$reviews,comments:$comments,summaries:$summaries}'

# Walk EVERY review id — greptile may post more than one review per push.
REVIEW_IDS=$(printf '%s' "$REVIEWS" | jq -r \
              '.[] | select(.user.login | test("greptile"; "i")) | .id')

for rid in $REVIEW_IDS; do
  # Follow greptile-triage.md (STEP 0): fetch line+top-level comments for THIS
  # review id; Suppressions Check; Classify; Reply (Tier 1/Tier 2); history write.
  # Fetch from the complete COMMENTS snapshot by pull_request_review_id.
  # Reply: gh api -X POST "repos/$REPO/pulls/$PR_NUMBER/comments/<cid>/replies" -f body="..."
  :
done

# ALSO triage greptile's SUMMARY feedback — it puts actionable findings in two
# places the line-comment walk above misses:
#   (1) the review BODY (the review's own summary text), and
#   (2) the top-level PR issue comment (greptile's "description reply" / summary,
#       often with collapsible per-file sections listing concerns inline).
# Run each through greptile-triage.md exactly like a line comment.
printf '%s' "$REVIEWS" | jq -r '.[] | select(.user.login|test("greptile";"i")) | select(.body != "") | .body'
printf '%s' "$SUMMARIES" | jq -c '.[] | select(.user.login|test("greptile";"i")) | {id, body}'
# Reply to a summary issue comment: gh api -X POST "repos/$REPO/issues/$PR_NUMBER/comments" -f body="..."
```

### GitLab (`FORGE=gitlab`)

```bash
set -euo pipefail
# Project full path (e.g. gpu/console), URL-encoded for the API.
PROJECT_PATH=$(glab repo view -F json | jq -er '.path_with_namespace | strings | select(length > 0)')
PROJ_ENC=$(printf '%s' "$PROJECT_PATH" | jq -sRr '@uri')
MR_IID=$(glab mr view -F json | jq -er '.iid | numbers')
POLL_HEAD=$(glab mr view -F json | jq -er '.sha // .diff_refs.head_sha')
[ "$POLL_HEAD" = "$(git rev-parse HEAD)" ] \
  || { echo "BABYSIT: local and pushed revisions differ; reset quiet polls" >&2; exit 1; }

# Walk EVERY discussion thread from the review bot. GitLab has no "review"
# object — a bot posts discussion threads (each with notes[]).
DISCUSSIONS=$(glab api "projects/$PROJ_ENC/merge_requests/$MR_IID/discussions" --paginate | jq -sc 'add')
NOTES=$(glab api "projects/$PROJ_ENC/merge_requests/$MR_IID/notes" --paginate | jq -sc 'add')
printf '%s' "$DISCUSSIONS" | jq -e 'type == "array" and all(.[]; (.id | type == "string") and (.notes | type == "array") and all(.notes[]; (.author.username | type == "string")))' >/dev/null \
  || { echo "BABYSIT: malformed discussion evidence; reset quiet polls" >&2; exit 1; }
printf '%s' "$NOTES" | jq -e 'type == "array" and all(.[]; (.id | type == "number") and (.author.username | type == "string"))' >/dev/null \
  || { echo "BABYSIT: malformed summary evidence; reset quiet polls" >&2; exit 1; }
THREAD_IDS=$(printf '%s' "$DISCUSSIONS" | jq -r '.[] | select(any(.notes[]; .author.username | test("greptile"; "i"))) | .id')

for tid in $THREAD_IDS; do
  # Follow greptile-triage.md (STEP 0): fetch the thread's notes (body +
  # position.new_path/new_line for line-level); Suppressions Check; Classify;
  # Reply (Tier 1/Tier 2); history write.
  # Fetch: glab api "projects/$PROJ_ENC/merge_requests/$MR_IID/discussions/$tid"
  # Reply: glab api -X POST "projects/$PROJ_ENC/merge_requests/$MR_IID/discussions/$tid/notes" -f body="..."
  :
done

# ALSO triage greptile's SUMMARY feedback — the MR-level notes greptile posts
# outside a positioned discussion thread (the "description reply" / overview
# note, often with inline per-file concerns). Run each through greptile-triage.md.
printf '%s' "$NOTES" | jq -c '.[] | select(.author.username|test("greptile";"i")) | select(.type == null) | {id, body}'
# Reply to a summary note: glab api -X POST "projects/$PROJ_ENC/merge_requests/$MR_IID/notes" -f body="..."
END_HEAD=$(glab mr view -F json | jq -er '.sha // .diff_refs.head_sha')
[ "$POLL_HEAD" = "$END_HEAD" ] \
  || { echo "BABYSIT: revision changed during review fetch; reset quiet polls" >&2; exit 1; }
jq -n --arg revision "$POLL_HEAD" --argjson discussions "$DISCUSSIONS" --argjson summaries "$NOTES" \
  '{revision: $revision, discussions: $discussions, summaries: $summaries}'
```

### History paths (forge-independent)

```bash
REMOTE_SLUG=$(~/.claude/skills/gstack/browse/bin/remote-slug 2>/dev/null \
              || basename "$(git rev-parse --show-toplevel 2>/dev/null || pwd)")
PROJECT_HISTORY="$HOME/.gstack/projects/$REMOTE_SLUG/greptile-history.md"
GLOBAL_HISTORY="$HOME/.gstack/greptile-history.md"
mkdir -p "$(dirname "$PROJECT_HISTORY")" "$(dirname "$GLOBAL_HISTORY")"
```

**Critical: walk every review thread.** Greptile (or the bot) may post
multiple reviews/threads per push (one structural, one security, etc.). The
triage helper fetches once per call; the multi-thread walk lives here, on
both forges.

## History file discipline

Every triaged comment writes one line to BOTH:

| File | Purpose |
|---|---|
| `$HOME/.gstack/projects/<slug>/greptile-history.md` | Per-project context; no historical label automatically dismisses a finding. |
| `$HOME/.gstack/greptile-history.md` | Global aggregate. Cross-project retro / pattern-mining. |

Line format (from the triage helper's "History File Writes" section):

```
<YYYY-MM-DD> | <owner/repo> | <type:fp|fix|already-fixed|disputed> | <file-pattern> | <category>
```

- **`fp`** — owner-confirmed dismissal for a named finding, with its quote,
  reason and reviewed revision recorded in Session Notes. Reassess on later revisions.
- **`disputed`** — awaiting the owner's decision; remains open and blocks done.
- **`fix`** — real issue, fixed in this cycle. Not suppressed.
- **`already-fixed`** — real issue, fixed in a prior commit on this branch. Not suppressed.

The categories are a fixed set: `race-condition`, `null-check`, `error-handling`, `style`, `type-safety`, `security`, `performance`, `correctness`, `other`. Don't invent new categories — if a finding doesn't fit, use `other` and mention the actual concern in the reply, not the history line.

## After a fix lands

1. Fix the code per the triage helper's classification.
2. Re-verify with `make lint` + `make test` (or relevant tier).
3. Reply via the triage helper's "Reply APIs" section (Tier 1 / Tier 2
   templates) through the forge layer above. Include the fix SHA in the body.
4. Commit and push.
5. Append the history line(s) to both files.
6. If the finding generalizes beyond this PR/MR (recurring pattern, new
   class of bug), capture as a named rule in the project's
   `.orly/docs/greptile-learnings/RULES.md` in the same commit as the fix.
   Otherwise the history line alone is the durable record.
7. Re-schedule the next poll using the cadence table above.

## Stop conditions

- **Done = two complete, consecutive empty polls AND CI green on the same pushed revision.** Both must hold: no new
  reviews/threads/summary findings for two polls **and** STEP 0.75 reports
  `ci=green`. A quiet bot with a still-`pending` or red CI is NOT done — keep
  polling on the cadence.
- Every cycle fetches all pages of reviews, line comments and summaries.
  Fetch failure, missing checks, a disputed finding or a changed revision resets
  the quiet counter; compare the remote revision again after check collection.
- Missing review completion for the current pushed revision is `review=pending`,
  even when all fetches succeed. Surface it after the recorded retry cadence;
  two empty responses alone cannot prove the reviewer completed its work.
- **PR/MR merged or closed** → stop, report.
- **CI red FROM your changes** → fix it (STEP 0.75): diagnose, fix the code,
  push, re-poll. Not a stop condition — it's work to do.
- **CI red and NOT from your changes** → surface to the user; pause the
  done-declaration until they decide.
- **User says stop / pause** → print one final
  `BABYSIT <PR|MR> #<n>: paused per user` line and stop.

## Report format

At end of each cycle:

```
BABYSIT REPORT — <PR|MR> #<n> @ <SHA>
  Polls run: <i>
  Reviews seen: <count>  (line: <l>, summary/description: <s>)
  Findings actioned: <k> (P0: <a>, P1: <b>)
  Findings deferred: <d> (with reasons)
  CI: <green | pending: <job…> | red: <job> — <fixed SHA | surfaced to user>>
  Rules added/updated: <list of RULE refs>  # via triage helper's history-write
  Next poll: in <delay>s OR stopped (<reason>)
```

## What this skill does NOT do

- Does **not** re-implement fetch/classify/reply — see
  `~/.local/share/gstack/review/greptile-triage.md`.
- Does **not** merge the PR/MR — use `/land-and-deploy` after the bot is green.
- Does **not** modify CI configuration or skip the review bot.
- Does **not** apply P2/P3/nit findings without explicit user approval
  (the triage helper's classification drives this).

## Failure modes

| Surface | Action |
|---|---|
| `gh api` / `glab api` rate-limited | Back off the poll interval by 2× and retry next cycle |
| Review missing on a push | Re-poll once at +180 s; if still missing, surface to user |
| `gh pr view` / `glab mr view` returns nothing (PR/MR not yet visible) | Skip cycle; retry next interval |
| `glab` not authed for a self-hosted host | Surface `BABYSIT: glab not authed for <host> — run glab auth login`; pause |
| Multiple reviews with conflicting suggestions | Apply the union; surface conflict only if both can't coexist |
| Finding is in code the agent did not author | Surface to user before fixing — never edit changes the agent did not create |

## References

- `~/.local/share/gstack/review/greptile-triage.md` — fetch / classify /
  reply / suppressions / history-write mechanics (installed by gstack).
- `$HOME/.gstack/projects/<slug>/greptile-history.md` — per-project
  history; drives Suppressions Check.
- `$HOME/.gstack/greptile-history.md` — global aggregate.
- `.orly/docs/greptile-learnings/RULES.md` — project-level named rules
  (durable principles). New rule added here only when the finding
  generalizes; per-incident rows go to greptile-history.md.
- this repository's `.orly/AGENTS.md` / `.orly/AGENTS.md` — CHORE(close) skill
  chain step 4 cites this skill.
- `gh api` docs: https://cli.github.com/manual/gh_api
- `glab api` docs: https://gitlab.com/gitlab-org/cli (`glab api --help`)
