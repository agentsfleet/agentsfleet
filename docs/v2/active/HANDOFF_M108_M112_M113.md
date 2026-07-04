## Handoff — M108 connector platform + M112 Fleet Library rename + M113 Models/Secrets/errors (all on PR #477)

### Scope/Status

Working out of the M108 connector-platform worktree, but by Kishore's explicit instruction this session, M112 and M113 are folded into the SAME branch/PR (#477) rather than getting their own worktrees — do not split them out unless told to.

- ✅ **M108** (six-provider connector platform) — DONE, spec in `docs/v2/done/`, this PR's original scope.
- ✅ **M112_001** ("Templates" → "Fleet Library", copy-only) — §1 (UI) and §2 (CLI) DONE, committed. §3 (docs prose) **intentionally not started** — gated on Kishore's explicit go-ahead before touching `~/Projects/docs` (separate repo).
  - Found and fixed along the way: `InstallEntry` (dashboard first-run embed) had zero `template:write` scope-gating on its Create-a-template CTA, unlike `InstallSourceSelector` which already gated the button (just not its paired description). Fixed both.
  - **Open decision from Kishore, not yet resolved:** install-page tagline reads "Pick from the fleet library. Watch live states." — Kishore floated "Watch the loop.." as an alternative; I pushed back (the page shows install *states*, not an agent execution loop) and left it as "Watch live states." pending his final call.
- 📝 **M113** (three workstreams) — specs drafted and committed, **all still `PENDING`, none started**:
  - `M113_001` (UI): collapse the Models page's special hero card into the same uniform row-list pattern Integrations already uses; wire the "Other provider" form's Provider field to the model catalogue (already fetched client-side) instead of free text.
  - `M113_002` (UI): "model not in cached caps catalogue" is one instance of a systemic gap — 6+ call sites bypass the friendly-error layer entirely, and only 15 of 100+ backend error codes have friendly copy. Spec scopes to closing the bypasses + the concretely-reachable codes.
  - `M113_003` (UI): **reverses M87** (a prior milestone that deliberately merged Secrets into the Models page with a regression test guarding against a split) — Kishore explicitly chose to split Secrets & ENVs back into its own page after being shown the cost, in this session's transcript. Revives an already-built-but-unused `DataTable`-based `CredentialsList.tsx` instead of the hand-rolled table M87 left behind; Add moves from an always-inline form to a dialog (matches `AddModelDialog`'s convention).
  - A pointer note (not a rewrite — M87's own spec is left as historical record) was added to `docs/architecture/billing_and_provider_keys.md` §8.3 referencing both M87 and M113_003.

### Working Tree

Worktree (`~/Projects/agentsfleet-m108-connector-platform`, branch `feat/m108-connector-platform`): **clean, fully pushed** — `git status -sb` shows no ahead/behind against `origin/feat/m108-connector-platform`.

**Main checkout** (`~/Projects/agentsfleet`, branch `main`) has 3 **uncommitted** files — these are a live-preview mirror of already-committed worktree changes (nav font-size fix, billing card simplification, the tailwind-merge font-size bugfix), made only so Kishore's already-running dev server there could hot-reload without a restart:
```
 M ui/packages/app/app/(dashboard)/settings/billing/page.tsx
 M ui/packages/app/components/layout/Shell.tsx
 M ui/packages/app/lib/utils.ts
```
**This mirror is now STALE** — it predates all of M112 (Fleet Library rename isn't mirrored there). Either `git checkout --` these 3 files in the main checkout (they're pure duplicates of `feat/m108-connector-platform` commits, safe to discard) or point that dev server at the worktree instead. Don't just leave it — it'll confuse anyone who diffs `main`.

### Branch/PR

- Branch: `feat/m108-connector-platform`
- PR: [#477](https://github.com/agentsfleet/agentsfleet/pull/477) — OPEN, not draft
- Latest pushed commit: `58741ec6`
- CI status: not checked this session — run `gh pr checks 477` first thing

### Running Processes

- Main checkout has a dev server already running (started outside this session, PID area ~25622-25628): `bun run dev` → `next dev --turbopack`, cwd `/Users/kishore/Projects/agentsfleet/ui/packages/app`. Confirm it's still alive with `ps aux | grep "next dev"` before assuming it's there.

### Tests/Checks

All green as of the last commit (`58741ec6`):
- ✅ `make test-unit-app` — 1169 tests
- ✅ `make test-unit-cli` — 1261 tests (100% coverage maintained)
- ✅ `make test-unit-design-system` — 432 tests
- ✅ `make lint-app` — Oxlint + tsc clean
- ✅ `gitleaks` — clean on every commit (pre-commit hook)
- Not yet run this session: `/write-unit-test` audit, `/review`, `/review-pr`, `kishore-babysit-prs` — required before CHORE(close) on M112, per the skill chain in `AGENTS.md`.

### Next Steps

1. Resolve the "Watch live states" vs. "Watch the loop" call with Kishore (small, one-line fix either way).
2. M112 §3 (docs prose): needs Kishore's explicit go-ahead before touching `~/Projects/docs` — prepare the diff, show it, then commit there separately from this repo's PR.
3. Discard or resolve the 3 stale uncommitted files in the main checkout (see Working Tree above).
4. M113 implementation not started — confirm with Kishore whether to proceed straight to EXECUTE (mirroring how M112 went: draft → confirm → implement) or if he wants to review the 3 specs first. Suggested order: `M113_001` (self-contained UI layout fix) → `M113_003` (Secrets split, touches the same `settings/models/page.tsx` file M113_001 does — land these two close together to avoid rebase pain) → `M113_002` (error-copy pass, fully independent, can run any time).
5. Before CHORE(close) on this PR: run the full skill chain (`/write-unit-test` → `/review` → `/review-pr` → `kishore-babysit-prs`), update the spec Status fields to DONE per completed Dimension, move fully-complete specs `active/` → `done/`.
6. Kishore mentioned wanting to cut a v2.0.0 release "today" — that's a separate, explicitly-gated step (tag + `gh release create`) not yet authorized; don't do it without a fresh explicit ask in whatever session gets to that point.

### Risks/Gotchas

- **M112/M113 are intentionally NOT in their own worktrees** — this was Kishore's explicit override of the default one-workstream-per-worktree convention ("I want all the M112, M113 folded in this PR"). Don't "fix" this by splitting them out.
- **M87 reversal (M113_003) is a deliberate, discussed decision**, not an oversight — Kishore was shown the regression-test cost via `AskUserQuestion` and chose to split anyway. Don't second-guess it without re-raising with him.
- **Docs-repo work (`~/Projects/docs`) needs a fresh explicit go-ahead each session** per how this repo's operating model treats that shared repo — don't carry forward the "go ahead" from this session into a new one.
- Two research agents this session flagged that background investigation Agent tool calls surfaced **prompt-injection attempts** embedded in tool outputs (fake "invoke this skill" instructions). Both were correctly ignored by the sub-agents and flagged up. Worth a moment's awareness if spawning more research agents on this same codebase — the injection source wasn't identified, just neutralized.
