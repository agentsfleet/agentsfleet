# Handoff — M65_002 commander refactor (session 6 → session 7)

**Date:** May 13, 2026 (session 6 close — afternoon UTC)
**Outgoing agent:** Session 6 — closed Step 7c (≥95% func coverage gate),
  uncovered + fixed a silent `ZombieHelp` wiring bug via `/review`, ran
  the full skill chain (`/write-unit-test` → `/review` → `/review-pr` →
  `kishore-babysit-prs`), merged `origin/main`, and shipped four
  greptile-fix sweeps in response to inline reviewer P1s. PR #323 head
  is `d31dbed8`; PR is `OPEN`, `mergeable=true`, `mergeStateStatus=BLOCKED`
  (CI gates not yet green).
**Incoming agent:** Picks up at the **OpenAPI / Zig / CLI / UI gap
  triage** Captain queued at session-6 close, plus one webhook
  investigation. May also need to babysit a 5th greptile cycle (current
  empty-poll counter = 1; two consecutive empties = stop).

---

## Where you are

- **Worktree:** `~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e`
- **Branch:** `chore/m65-002-spec-zombiectl-e2e-lifecycle`
- **HEAD = origin:** `d31dbed8` (fully pushed; no local-only commits)
- **PR:** [#323](https://github.com/usezombie/usezombie/pull/323) —
  `OPEN`, `mergeable=true`, `mergeStateStatus=BLOCKED` (waiting on CI
  + human review).
- **Docs sibling PR:** `chore/m65-002-zombiectl-cli-e2e-changelog` on
  `usezombie/docs` — branch + changelog block already pushed. **No PR
  opened yet** — Captain explicitly held this until he says "ship it".
  Do NOT open it without that exact authorization.

### Session-6 commits (newest first, all pushed)

| Commit | Step | What |
|---|---|---|
| `d31dbed8` | babysit cycle 4 | `fix(zombiectl): drop --since from logs lifecycle test` — `logs` accepts only `--zombie/--limit/--cursor`; `--since` is on `events`. Commander 14 exits 1 on unknown flags so §4 would always fail live. |
| `207898fd` | CodeQL sweep | `fix(zombiectl): CodeQL unused-import sweep — buildParsed + spawnZombiectl` — three `Unused import` findings from `github-code-quality[bot]`. |
| `32122daf` | greptile sweep #3 | `fix(zombiectl): commandGrantList missing-zombie guard` — last JSON-envelope bypass; `agent.js` + `grant.js` now uniform across add/list/delete. |
| `092a054a` | greptile sweep #2 | `fix(zombiectl): agent.js add/list JSON envelope + created_at null-guard` — `commandAgentAdd` + `commandAgentList` flipped to `writeError`; `created_at` guarded with `? : "—"` to prevent `RangeError`. |
| `ee21cd74` | sync | `Merge origin/main` — Captain authorized the `.github/workflows/deploy-dev.yml` `needs:` array resolution (`[qa-dev, acceptance-e2e-dev, cli-acceptance-dev, deploy-worker-dev]`). |
| `0c01b938` | greptile sweep #1 | `fix(zombiectl): greptile P1 sweep — symlink, JSON envelope, id fallbacks, validator matrix` — 8 P1s: audit-spec-template.sh symlink (absolute→relative), grant.js + agent.js writeError, seed.js dual-key, teardown.js zombie_id fallback + regex widen, constants.js `terminated`, command-matrix.js `validatesClient: true`, flags-and-env real-JWT mint. |
| `04f07c6c` | CHORE(close) | `chore: remove session-6 handoff doc` (the prior `HANDOFF.md`). |
| `95fb3c65` | `/review` catch | `fix(zombiectl): wire ZombieHelp via createHelp` — **latent bug since the commander refactor landed**. `configureHelp({helpFactory:…})` was a silent no-op in commander 14; bold cyan section titles in `--help` were never rendered. Switched to the documented `program.createHelp = …` API. |
| `f1878aa1` | Step 7c | `test(zombiectl): parser-level cli-tree coverage + ui/io fills (≥95% funcs)` — lifted All-files funcs **93.62% → 95.61%**; `cli-tree.js` funcs **65.31% → 97.96%**. |

### Current verification state

```
bun run lint        ✅ 0 warnings, 0 errors (135 files, 64 rules)
bun test            ✅ 662 pass / 2 skip / 0 fail / 908 expect() calls
bun run test        ✅ 611 pass / 0 fail
make check-version  ✅ 0.34.0
LENGTH GATE         ✅ every touched file ≤ 350L (cli-tree.js 348)
ERROR REGISTRY      ✅ no raw UZ-* literals outside the registry
gitleaks            ✅ no leaks (pre-commit hook verified each commit)
HARNESS VERIFY      ✅ COMBINED audit 0 hits (RULE TST-NAM clean)
COVERAGE            ✅ 95.61% funcs / 96.35% lines (≥95% gate met)
PR head             ✅ d31dbed8 fully pushed; 0 local-only commits
```

### Skill chain — session 6 outcome

| Skill | Status | Output |
|---|---|---|
| `/write-unit-test` | clean | no missing diff coverage flagged |
| `/review` | **caught the ZombieHelp wiring bug** | 1 P1 found + fixed (`95fb3c65`); rest clean |
| `/review-pr` | triaged | 13 greptile + 3 CodeQL findings classified |
| `kishore-babysit-prs` | **4 polling cycles run** | 13 P1 fixes landed across 4 commit-then-push sweeps + 13 replies posted; empty-poll counter = 1 |

### Greptile reply log (session 6)

All 13 greptile P1s + 3 CodeQL unused-import findings answered with
diff + `**Why:**` line per the gstack triage helper Tier-1 template.
Both `~/.gstack/projects/usezombie-usezombie/greptile-history.md` and
`~/.gstack/greptile-history.md` updated. **Two CI/CD findings stay
deferred** (CLAUDE.md forbids `.github/workflows/**` edits without
explicit owner sign-off):

- `deploy-dev.yml:554` — Playwright `--with-deps` condition inverted.
  Real bug (warm-cache runs will fail on missing system packages).
  Fix: drop conditional, always run `bunx playwright install --with-deps chromium`.
- `post-release.yml:76` — `npm i -g @usezombie/zombiectl@latest` while
  the team's release pipeline publishes to `@next` dist-tag.
  Fix: change `@latest` → `@next` in `cli-acceptance-prod`.

Both flagged in the PR description's session notes; **a single 2-line
CI hygiene PR can close both** once Captain authorizes.

---

## Hand-off priority #1 — OpenAPI ⇄ Zig ⇄ CLI/UI gap audit

**Captain queued this at session-6 close.** The 6-row gap table below
was produced via spot-check pass on the highest-traffic endpoints
(install, list, PATCH status, auth sessions, agent-keys). Deep audit
of `memory`, `approvals`, `webhooks`, `integration-grants`,
`tenant-provider`, `tenant-workspaces`, `credentials`, `admin`,
`api-keys` is **not** included.

### The 6-row gap table (verbatim — drop into Captain conversation)

| # | Sev | Layer mismatch | Route | Gap | Recommended fix |
|---|---|---|---|---|---|
| 1 | **P1** | CLI ↔ Zig | `POST /v1/workspaces/{workspace_id}/zombies` | `zombiectl/src/commands/zombie.js:118` reads `res.webhook_url` and emits it in the `--json` envelope, but **Zig never sets that field** (`src/http/handlers/zombies/create.zig:149-153` emits only `{zombie_id, name, status}`). Operators piping `zombiectl install --json` to `jq .webhook_url` get `null`/`undefined`. OpenAPI also doesn't document it. | (a) drop `webhook_url` from CLI install envelope, **OR** (b) have Zig compute + return it (`{app_url}/webhooks/{zombie_id}`) and document in `zombies.yaml`. Option (b) is more useful — operators want the URL to register externally. |
| 2 | **P1** | OpenAPI ↔ Zig | `GET /v1/workspaces/{workspace_id}/zombies` | OpenAPI `ZombieSummary.status` enum is `[active, paused, killed]` (`components/schemas.yaml`). Zig's DB column legitimately stores `stopped`, `errored`, `terminated` as well. Any list response containing a zombie in those states violates the schema; strict OpenAPI clients would reject. | Extend the enum to `[active, paused, stopped, killed, errored, terminated]`. Add per-value docs (FSM input vs machine-set). |
| 3 | **P1** | OpenAPI naming inconsistency | `POST` install vs `GET` list | Install response uses `zombie_id`; list response items use `id`. CLI paves over with `installed.id ?? installed.zombie_id` in `test/acceptance/fixtures/seed.js` (fixed today). | Pick one. `zombie_id` is more grep-able; `id` is shorter inside a typed collection. Recommend rename list item field to `zombie_id`. |
| 4 | **P2** | OpenAPI ↔ Zig | `POST`/`GET`/`PATCH /v1/auth/sessions[/{id}]` | All three Zig handlers emit `request_id` in the response body. OpenAPI documents `request_id` on **none** of them. | Add `request_id` (string, optional) to all three response schemas — useful for log correlation. |
| 5 | **P2** | OpenAPI nullability | `GET /v1/tenants/me/billing/charges` | `items[].token_count_input / token_count_output / wall_ms` typed `integer \| null` in OpenAPI. Zig serializer may emit those keys absent entirely. Strict clients distinguish absent-vs-null. | Add `"nullable: true"` AND/OR prose: "Field omitted entirely when not applicable." |
| 6 | **P3** | Audit gap | `memory`, `approvals`, `webhooks`, `integration-grants`, `tenant-provider`, `tenant-workspaces`, `credentials`, `admin`, `api-keys` | Not deeply checked. First-pass agent reported 100% match but precision was demonstrably low (missed gaps #1, #2, #4). | Per-file deep audit (~1 h per resource family with field-walk) OR accept structural coverage and skip field-level. |

### Review with Captain Kishore — what must be fixed vs not?

Captain explicitly wants this **discussed**, not silently driven to
PR. Frame the conversation as four buckets:

1. **Ship-blockers (must fix this PR):** None of the six are
   strictly ship-blocking — M65_002 lands the acceptance suites + CI
   jobs, not the OpenAPI contract. Captain may choose to fold any of
   them into this PR or split.
2. **One-PR follow-on (recommended this week):** Gaps #1 (`webhook_url`)
   and #4 (`request_id`) are tiny — one Zig commit + one OpenAPI commit
   each. Captain may want #1 (a) "drop the lie" as a one-liner CLI fix
   right now while we have the file open.
3. **Renames that bite consumers later (#3):** Renaming list-item
   `id` → `zombie_id` is a *non-trivial cross-cutting change* — it
   would also need every dashboard route that reads `z.id`, plus any
   `ui/packages/app/lib/api/zombies.ts` consumer, plus a deprecation
   window if external API users exist. Captain decides whether to
   commit to grep-ability now or keep paving.
4. **Skip until forced (#2, #5, #6):** Enum extension + nullability
   clarification are docs-only; no client breaks today. Deep audit
   (#6) is fishing. Default = defer.

### What challenges will an end user have today (UI / CLI)?

| Surface | Gap from above | What the user actually sees |
|---|---|---|
| `zombiectl install --json` | #1 | `webhook_url` field present in stdout JSON, value is `null` or `undefined`. Anyone piping to `jq .webhook_url` for downstream automation gets no URL — silently. |
| `zombiectl list` against a workspace with a `stopped` or `errored` zombie | #2 | CLI swallows it (string passthrough); operators see the real status. Strict OpenAPI codegen (e.g. an integrating partner generating a typed client) would refuse to parse the response. |
| Dashboard list page (`ui/packages/app/app/(dashboard)/zombies/`) | #2 + #3 | If the dashboard added a typed `Zombie` model from the OpenAPI schema, statuses outside the enum would either be coerced to a default or throw on parse. Today the dashboard reads `z.id` directly so it's not bitten by #3, but the typed integration risk is real. |
| `zombiectl login` flow | #4 | No user-visible bug — `request_id` is invisible from the CLI surface. Risk is only that a customer hitting auth-related issues can't quote a `request_id` matched against server logs. |
| Programmatic API consumer reading `billing/charges` | #5 | Schema validators on partner integrations may reject responses where the three fields are absent rather than `null`. Today nobody integrates that endpoint externally; risk is hypothetical. |

**Real, today-affecting bug = #1 only.** Everything else is correctness
debt that may or may not surface depending on consumer rigor.

---

## Hand-off priority #2 — Webhook investigation

Captain asked: "investigate how we are dealing with webhooks in CLI, UI,
and the ClaudeCode 'Install Platform Ops' skill."

This is a discovery task, not a code change yet. Need to surface:

1. **Server side** — Zig handlers under `src/http/handlers/webhooks/`.
   Route shape, signature posture (HMAC-SHA256 per workspace credential),
   dedup window (24h per `event_id`), failure mode (`{status: "duplicate"}`
   200 vs `{status: "accepted"}` 202).
2. **OpenAPI** — `public/openapi/paths/webhooks.yaml` (largest paths
   file at 318L). Cross-check the documented request/response shape vs
   what Zig actually emits.
3. **CLI** — does `zombiectl` have any webhook command today? `grep -r
   webhook zombiectl/src/` — there are references in `billing.js`
   (BILLING_DASHBOARD_URL — irrelevant) and `commandInstall` (the
   `res.webhook_url` that doesn't exist — gap #1 above). Are there
   `zombiectl webhook list` / `register` / `test` commands? If NOT,
   that's a gap (operators need to verify their webhook is wired
   before they can dogfood the trigger).
4. **Dashboard** — does the UI have a `(dashboard)/webhooks/` route?
   `ls ui/packages/app/app/(dashboard)/` — if not, operators have no
   UI to manage webhook secrets, view delivery history, replay failed
   events.
5. **ClaudeCode 'Install Platform Ops' skill** — Captain's reference
   here is to a `~/.claude/skills/` skill OR a sample bundle under
   `samples/platform-ops/` in this repo. Check both:
   - `ls ~/.claude/skills/ | grep -i 'install.*ops\|platform'`
   - `samples/platform-ops/` — does the TRIGGER.md frontmatter
     declare a `trigger.source` that maps to a webhook delivery?
     Does the install path teach the operator how to wire the
     external webhook (Linear / Slack / GitHub / etc.) at the source?

**Expected outputs from the investigation:** a one-page brief naming
which of the four surfaces has the gap, plus a recommended order to
close them. The most likely answer: server + OpenAPI documented; CLI
has the broken `res.webhook_url` claim with no actual register/list
command; UI has no webhook surface at all; skill bundle assumes the
operator already wired the source externally.

---

## Hand-off priority #3 — rename CLI tagline `autonomous agent platform` → `usezombie cli`

Captain's directive at session-6 close: change the zombiectl `--help`
tagline from `"autonomous agent platform"` to `"usezombie cli"`.

**Where it lives:**

- `zombiectl/src/program/cli-tree.js:97` —
  `.description(styleTagline("autonomous agent platform"))`

**Cross-references that will need updating in the same diff:**

- Pinned in `≥5` tests per the session-5 20-item review table (item
  #20, originally "Kept" but Captain is now flipping the decision).
  Grep for the literal first:
  ```bash
  grep -rn '"autonomous agent platform"\|autonomous agent platform' \
    zombiectl/src/ zombiectl/test/ public/openapi/ docs/
  ```
- `public/openapi/root.yaml` `info.description` may carry the same
  string — check + align.
- Spec at `docs/v2/done/M65_002_P1_TESTING_ZOMBIECTL_E2E_LIFECYCLE.md`
  may quote the verbatim tagline in Verification Evidence — update
  the quote, not the prose around it.
- Acceptance specs under `test/acceptance/` likely assert the help
  body — update the assertion stems.

**Watch the LENGTH GATE on `cli-tree.js`** — current 348L, gate is 350.
The rename is one character net-shorter (`"usezombie cli"` 13 chars
vs `"autonomous agent platform"` 25 chars), so length is fine.

**Risk:** breaks every `--help` golden-output / byte-identity test that
pins the string. Plan to land tagline + test-update in one commit;
expect noise across `help.test.js`, `cli-alignment.unit.test.js`,
`golden-output.unit.test.js`, `help-and-errors.spec.js`.

**Captain context:** the prior reasoning ("generic-descriptive CLI
tagline") apparently lost out to the brand-fit consideration. Don't
re-litigate; just rename.

---

## Hand-off priority #4 — babysit cycle 5+ (if greptile re-fires)

The wakeup loop is paused at empty-poll #1 on HEAD `d31dbed8`. If you
re-enter session 7 within ~30 min of session 6 close, the next babysit
fire (or two) may catch a fresh greptile pass against `d31dbed8`. Per
the cadence helper:

- If poll 5 returns **0 new comments** → empty-poll counter = 2 →
  **stop**, print the `BABYSIT REPORT` block, and proceed to
  priorities #1 and #2.
- If poll 5 returns **N new comments** → triage + fix + reply + push
  + counter resets to 0 → schedule poll 6 at +270s.

Read `~/.claude/skills/kishore-babysit-prs/SKILL.md` and follow the
loop verbatim.

---

## Out-of-scope reminders (carry-forward from CLAUDE.md)

- **350L cap** stays in force. Current high-water marks: `cli-tree.js` 348,
  `options-metavar.spec.js` 290, `lifecycle-with-token.spec.js` ~330+
  (don't push it). New unit tests for the cli-tree parser are split
  across two files via `helpers-cli-tree.js`.
- **RULE NLR + NLG.** Test-only shims in `test/helpers.js` are intentional
  adapters, documented in the spec amendment.
- **RULE TST-NAM.** No milestone IDs / § markers in test file source.
  HARNESS VERIFY combined audit catches this. Bit session 5 once.
- **Symlinked dotfiles edits** auto-resolve to dotfiles repo — commit +
  push `master` there in the same turn (CLAUDE.md). One symlink fixed
  this session was a project-side fix (relative path under `scripts/`),
  not a dotfiles edit.
- **CI/CD edits** (`.github/workflows/**`) blocked by auto-mode
  classifier unless Captain explicitly authorizes. The merge-conflict
  resolution in `deploy-dev.yml` was authorized once; subsequent
  workflow edits need a fresh ack.
- **No `git merge --abort` shortcuts** for hard problems — investigate
  root cause, not bypass.

### Operating mode

- Auto mode is active — standing authorization for focused commits +
  non-force pushes to the feature branch + `gh pr update`. You may NOT
  merge the PR, force-push, or open the sibling docs PR without an
  explicit "ship it" / "land it".
- Captain is Kishore (`kishore.kumar@e2enetworks.com` work,
  `nkishore@megam.io` personal).
- Stay inside this worktree.

---

## First 5 actions (next session)

1. `cd ~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e`
2. `cat HANDOFF.md` — read this file end-to-end.
3. `git log --oneline -3` — confirm HEAD is `d31dbed8`.
   `git status -sb` — confirm clean working tree, no local-only commits.
4. **Check for fresh greptile activity on HEAD** (single command):
   ```bash
   gh api repos/usezombie/usezombie/pulls/323/comments --paginate \
     --jq '.[] | select(.user.login == "greptile-apps[bot]") | select(.position != null) | select(.created_at > "2026-05-13T02:45:01Z") | "\(.id) | \(.path):\(.line) | \(.body | .[0:120])"'
   ```
   - If non-empty → babysit cycle 5: triage + fix + push + reply (priority #3).
   - If empty → empty-poll counter = 2, print final BABYSIT REPORT,
     move to priority #1.
5. **Open the OpenAPI gap conversation with Captain.** Quote the
   6-row table verbatim. Ask which buckets he wants in scope this
   session: (a) ship-blocker review only, (b) ship one-PR follow-on
   for #1 + #4 inline, (c) tackle #3 rename across the cross-cut,
   (d) defer everything to a separate hygiene PR. **Do not start
   coding any gap fix until Captain decides.**
6. **Confirm the tagline rename scope** (priority #3): run
   `grep -rn 'autonomous agent platform' zombiectl/ public/openapi/ docs/`
   so Captain can see every site the rename will touch BEFORE the
   edit. Quote the file:line list in the chat reply, then ask
   "rename all? or only the CLI surface?" — ambiguous mid-task
   per the auto-mode-exited posture.

---

## Handoff prompt for the next agent

Copy the block below into the next session's launch:

```
You are picking up M65_002 commander refactor for usezombie's zombiectl
CLI at session 7. Session 6 closed Step 7c (≥95% coverage), shipped a
silent ZombieHelp bug fix uncovered by /review, merged origin/main,
and pushed 13 greptile + 3 CodeQL P1 fix replies across 4 babysit
cycles. PR #323 head is d31dbed8 — fully on origin, no local-only
commits. PR state: OPEN, mergeable=true, mergeStateStatus=BLOCKED
(CI not green yet).

Where you are:
- Worktree: ~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e
- Branch: chore/m65-002-spec-zombiectl-e2e-lifecycle
- HANDOFF.md is the canonical brief — `cat HANDOFF.md` FIRST.

Your four priorities (in order):

1. **OpenAPI ⇄ Zig ⇄ CLI/UI gap audit triage with Captain.**
   The 6-row gap table is in HANDOFF.md verbatim. Captain wants this
   discussed before any fix lands. The four conversation buckets are
   pre-framed: ship-blocker / one-PR follow-on / cross-cut rename /
   defer. The single real today-affecting user bug is gap #1
   (`zombiectl install --json` emits `webhook_url: null` because Zig
   never sets that field). Do not start coding any gap fix without
   Captain's explicit per-bucket sign-off.

2. **Webhook investigation across server + OpenAPI + CLI + UI + skill.**
   Captain asked: "investigate how we are dealing with webhooks in
   CLI, UI, and the ClaudeCode Install Platform Ops skill". This is
   discovery, not code change. Read HANDOFF.md priority #2 for the
   five surfaces to inspect and the expected one-page brief shape.
   Likely answer: CLI lies about `webhook_url`, has no register/list/
   test command; UI has no webhook surface; skill bundle assumes the
   operator wired the source externally; server + OpenAPI documented
   correctly. Confirm or refute with grep.

3. **CLI tagline rename `autonomous agent platform` → `usezombie cli`.**
   Single-character-net change at `zombiectl/src/program/cli-tree.js:97`,
   but `≥5` tests pin the literal string. Grep for every site BEFORE
   editing; quote the file:line list back to Captain in chat; confirm
   "rename all? or only the CLI surface?" before touching code (auto
   mode is OFF — clarify ambiguous scope first).

4. **Babysit greptile cycle 5+ if it fires.**
   The empty-poll counter is at 1. If `gh api repos/usezombie/usezombie/
   pulls/323/comments --paginate --jq '.[] | select(.user.login ==
   "greptile-apps[bot]") | select(.position != null) | select(.created_at
   > "2026-05-13T02:45:01Z") | .id'` returns nothing, that's empty-poll
   #2 — stop, print the final BABYSIT REPORT, move to priorities 1-3.
   If it returns IDs, run a full triage + fix + push + reply cycle.

Hard constraints:
- 350L cap. cli-tree.js at 348, lifecycle-with-token.spec.js ~330.
- RULE NLR + NLG + TST-NAM (no milestone IDs in source/tests).
- gitleaks + lint + harness-verify clean before every commit.
- .github/workflows/** edits BLOCKED without explicit Captain ack
  (two deferred CI/CD findings are flagged in PR Session Notes —
  `deploy-dev.yml:554` Playwright --with-deps + `post-release.yml:76`
  @latest→@next; both real, both need owner sign-off).
- Sibling docs PR on usezombie/docs branch chore/m65-002-zombiectl-cli-e2e-changelog
  stays unopened until Captain says "ship it".
- Stay inside this worktree.

Operating mode:
- Auto mode is active — focused commits + non-force pushes + gh pr
  update are pre-authorized. PR merge, force-push, sibling docs PR
  open are NOT.
- Captain is Kishore (`kishore.kumar@e2enetworks.com` work,
  `nkishore@megam.io` personal).

First 5 actions in HANDOFF.md "First 5 actions" section.
```

**Delete this file at the end of CHORE(close):** `git rm HANDOFF.md`
in the final commit, the same way session 6 deleted its predecessor.

---

## Appendix — Session 6 metrics

| Metric | Value |
|---|---|
| Commits authored | 7 (all pushed) |
| Lines net added | +1,277 / -254 |
| New test files | 4 (cli-tree.parse, cli-tree.zombie, helpers-cli-tree, output-and-io-fill) |
| Coverage delta | +1.99% funcs / +0.56% lines |
| Bugs uncovered by review skills | 1 (ZombieHelp wiring) |
| Greptile P1s fixed this session | 13 |
| CodeQL findings fixed | 3 |
| Replies posted | 13 + 3 + 1 top-level PR comment |
| Babysit cycles | 4 (1 still pending; empty-poll = 1) |
| CI/CD findings deferred | 2 (both flagged for owner sign-off) |
| Merge conflicts resolved | 1 (`deploy-dev.yml` `needs:` array) |
