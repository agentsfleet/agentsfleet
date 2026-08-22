---
name: orly-babysit-prs
description: |
  Post-push follow-up for PRs/MRs: backoff-cadence polls over CI check runs
  (fix every failure your diff caused), every greptile inline thread, and the
  PR-level summary; classifies via gstack greptile-triage, suppresses via
  greptile-history, stops after two consecutive empty polls with CI green.
  Forge-aware (gh PR / glab MR). Use after `gh pr create` / `glab mr create`,
  after every push to a branch with an open PR/MR, or on "babysit", "watch
  greptile", "poll the PR/MR", "watch reviews", "follow up on review
  feedback". Cross-agent: Claude, Codex, OpenCode, Amp.
---

# orly-babysit-prs

Review bots (greptile on GitHub; greptile or another review bot on GitLab)
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
[ -r "$GREPTILE_TRIAGE" ] || GREPTILE_TRIAGE="$HOME/Projects/dotfiles/.unified-skills/review/greptile-triage.md"
[ -r "$GREPTILE_TRIAGE" ] || { echo "BABYSIT: greptile-triage.md missing — abort cycle"; exit 1; }
```

Then walk it section by section:

| Triage section | What this skill triggers |
|---|---|
| `## Fetch` | Run per detected review thread (multi-thread loop below), via the forge layer |
| `## Suppressions Check` | Read `$HOME/.gstack/projects/<slug>/greptile-history.md`; skip lines tagged `fp` matching `repo + file-pattern + category` |
| `## Classify` | Read file ±10 lines around each comment's `path:line`; classify VALID & ACTIONABLE / VALID BUT ALREADY FIXED / FALSE POSITIVE / SUPPRESSED |
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

Both forges' bots are filtered by author name matching `greptile` (or the
project's configured review bot — see the per-project history `bot:` note if
present).

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
gh pr checks "$PR_NUMBER" --json name,state,bucket,link 2>/dev/null \
  | jq -r '.[] | "\(.bucket)\t\(.state)\t\(.name)\t\(.link)"'   # bucket ∈ pass|fail|pending|skipping|cancel
```

### GitLab

```bash
glab ci status 2>/dev/null \
  || glab api "projects/$PROJ_ENC/merge_requests/$MR_IID/pipelines" \
       | jq -r '.[0] | "\(.status)\t\(.web_url)"'               # status ∈ success|failed|running|canceled|pending
```

| CI state | Action |
|---|---|
| all `pass`/`skipping` (GitLab `success`) | Record `ci=green`. A done-declaration is now allowed (if greptile is also quiet). |
| any `pending`/`running` | Record `ci=pending`; re-poll on the cadence. **Never declare done while a check is still running.** |
| any `fail`/`cancel` **from your changes** | In-scope finding you OWN — treat like a greptile fix. Fetch the failing log (`gh run view <run-id> --log-failed`, run id from the check `link`; GitLab `glab ci trace <job-id>`), fix the code, re-verify with the relevant `make` target, commit, push, re-poll. |
| any `fail` **NOT from your changes** (pre-existing red, infra flake, unrelated job) | Surface to the user with job name + link. Do **not** fix blindly, do **not** edit CI config. Pause the done-declaration until they decide. |

CI failures caused by your commit are not "surface and wait" — they are the
same contract as a greptile P0: diagnose, fix the code (never the CI config),
push, and re-poll until green.

## Triggers

- After `gh pr create` / `glab mr create` for a feature branch.
- After every `git push` to a branch with an open PR/MR.
- User says: "babysit", "watch greptile", "poll the PR", "poll the MR",
  "watch reviews", "follow up on review feedback", "what did greptile say".

## Output contract

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
REPO=$(gh repo view --json nameWithOwner --jq '.nameWithOwner')
PR_NUMBER=$(gh pr view --json number --jq '.number')

# Walk EVERY review id — greptile may post more than one review per push.
REVIEW_IDS=$(gh api "repos/$REPO/pulls/$PR_NUMBER/reviews" \
              --jq '.[] | select(.user.login | test("greptile"; "i")) | .id')

for rid in $REVIEW_IDS; do
  # Follow greptile-triage.md (STEP 0): fetch line+top-level comments for THIS
  # review id; Suppressions Check; Classify; Reply (Tier 1/Tier 2); history write.
  # Fetch: gh api "repos/$REPO/pulls/$PR_NUMBER/reviews/$rid/comments"
  # Reply: gh api -X POST "repos/$REPO/pulls/$PR_NUMBER/comments/<cid>/replies" -f body="..."
  :
done

# ALSO triage greptile's SUMMARY feedback — it puts actionable findings in two
# places the line-comment walk above misses:
#   (1) the review BODY (the review's own summary text), and
#   (2) the top-level PR issue comment (greptile's "description reply" / summary,
#       often with collapsible per-file sections listing concerns inline).
# Run each through greptile-triage.md exactly like a line comment.
gh api "repos/$REPO/pulls/$PR_NUMBER/reviews" \
  --jq '.[] | select(.user.login|test("greptile";"i")) | select(.body != "") | .body'
gh api "repos/$REPO/issues/$PR_NUMBER/comments" \
  --jq '.[] | select(.user.login|test("greptile";"i")) | {id, body}'
# Reply to a summary issue comment: gh api -X POST "repos/$REPO/issues/$PR_NUMBER/comments" -f body="..."
```

### GitLab (`FORGE=gitlab`)

```bash
# Project full path (e.g. gpu/console), URL-encoded for the API.
PROJECT_PATH=$(glab repo view -F json 2>/dev/null | jq -r '.path_with_namespace' \
               || git remote get-url origin | sed -E 's#.*[:/]([^:/]+/[^/]+)$#\1#; s#\.git$##')
PROJ_ENC=$(printf '%s' "$PROJECT_PATH" | sed 's#/#%2F#g')
MR_IID=$(glab mr view -F json | jq -r '.iid')

# Walk EVERY discussion thread from the review bot. GitLab has no "review"
# object — a bot posts discussion threads (each with notes[]).
THREAD_IDS=$(glab api "projects/$PROJ_ENC/merge_requests/$MR_IID/discussions" --paginate \
             | jq -r '.[] | select(any(.notes[]; .author.username | test("greptile"; "i"))) | .id')

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
glab api "projects/$PROJ_ENC/merge_requests/$MR_IID/notes" \
  --jq '.[] | select(.author.username|test("greptile";"i")) | select(.type == null) | {id, body}'
# Reply to a summary note: glab api -X POST "projects/$PROJ_ENC/merge_requests/$MR_IID/notes" -f body="..."
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
| `$HOME/.gstack/projects/<slug>/greptile-history.md` | Per-project. Drives Suppressions Check on next cycle — `fp` lines suppress matching comments forever. |
| `$HOME/.gstack/greptile-history.md` | Global aggregate. Cross-project retro / pattern-mining. |

Line format (from the triage helper's "History File Writes" section):

```
<YYYY-MM-DD> | <owner/repo> | <type:fp|fix|already-fixed> | <file-pattern> | <category>
```

- **`fp`** — false positive. Suppressed on future polls.
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
   `docs/greptile-learnings/RULES.md` in the same commit as the fix.
   Otherwise the history line alone is the durable record.
7. Re-schedule the next poll using the cadence table above.

## Stop conditions

- **Done = two consecutive empty polls AND CI green.** Both must hold: no new
  reviews/threads/summary findings for two polls **and** STEP 0.75 reports
  `ci=green`. A quiet bot with a still-`pending` or red CI is NOT done — keep
  polling on the cadence.
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
  reply / suppressions / history-write mechanics. Also at
  `~/Projects/dotfiles/.unified-skills/review/greptile-triage.md`.
- `$HOME/.gstack/projects/<slug>/greptile-history.md` — per-project
  history; drives Suppressions Check.
- `$HOME/.gstack/greptile-history.md` — global aggregate.
- `docs/greptile-learnings/RULES.md` — project-level named rules
  (durable principles). New rule added here only when the finding
  generalizes; per-incident rows go to greptile-history.md.
- `~/Projects/dotfiles/AGENTS.md` — CHORE(close) skill chain step 4
  cites this skill.
- `gh api` docs: https://cli.github.com/manual/gh_api
- `glab api` docs: https://gitlab.com/gitlab-org/cli (`glab api --help`)
