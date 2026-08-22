# lifecycle.md — stage-runbook latent façade

Read this at a lifecycle stage transition: CHORE(open), PLAN, EXECUTE onboarding,
CHORE(close), LAND, worktree setup, or milestone bootstrap. The generated
`AGENTS.orly.md` carries each stage's binding essence; this façade carries the
runbooks — the checklists, recipes, and output formats an agent needs only at
that moment. 🤔 judgment-only — no `.sh` pair; CONFORM's deterministic gates and
`orly gate` remain the mechanical anchors.

Section-scan first (`grep -n "^## " dispatch/lifecycle.md`); read the stage you
are entering, not the whole file.

## CHORE (open) — runbook

1. Spec `docs/v*/pending/` → `active/`; `Status: IN_PROGRESS`; `Branch:` set.
2. **Test Baseline** — run the repository's declared `verify.unit` (and
   `verify.integration` where declared) from `.oracle/orly.json`; copy the
   reported counts into the spec header as `**Test Baseline:** unit=<N>
   integration=<M>` (VERIFY's Test Delta row compares against it; a product
   pack may name a dedicated counter — see the product block below).
3. Create the worktree; verify CWD is inside it (`pwd` + `git worktree list`).
4. Commit the four steps on the feature branch. No code until the commit lands.

## Worktree recipe (agentsfleet)

`git checkout main && git branch feat/mNN-name && git worktree add ../agentsfleet-mNN-name feat/mNN-name && cd ../agentsfleet-mNN-name && bun install && (cd cli && bun install && bun run build)`.
The root `bun install` hydrates the workspace (`ui/packages/*`); `cli/` is its
own Bun project needing install + build. `git worktree add` fires
`.githooks/post-checkout` → symlinks `~/.config/agentsfleet/{ui,runner}.env.local`
into the tree; on 🟠 run `provision-env-1password`, re-link. Post-merge:
`git worktree remove ../agentsfleet-mNN-name`.

**agentsfleet Test Baseline counter:** `make _lint_zig_test_depth`.

**agentsfleet CHORE(close) paths:** the `<Update>` lands in
`~/Projects/docs/changelog.mdx` (template + version-bump matrix:
`skills/release-template.md` in the governance checkout — re-source each
release, never paraphrase). Version sync: `VERSION` touched → `make
sync-version`; commit propagated `build.zig.zon` / `agentsfleet/package.json` /
`agentsfleet/src/cli.js`; `make check-version` passes.

**dotfiles takes NO worktree — feature branches in the main checkout.** Every
agent-home symlink and every consumer's `ORLY_ROOT` resolves to
`~/Projects/dotfiles`; a linked worktree's `.git` is a *file*, and the audit
chain the hooks run spawns git against other directories. Under a hook that
combination corrupted the real index and flipped `core.bare` (Aug 16, 2026).
So: `git checkout -b feat/mNN-name` in `~/Projects/dotfiles` itself, and
`orly update` always renders the one true root. One stream at a time —
a second concurrent agent coordinates in-session rather than forking a tree.
This is the dotfiles exception; product repos keep the worktree recipe above.

**Mid-stream spec → ask before hydrating (default: same tree).** A spec created
inside an active worktree → ask the user before spinning up a second one. Lean
same tree: a second tree fragments the outcome and adds a PR to babysit.
Complete the outcome in place; fold new scope into the current spec/PR (reopen
`done/`→`active/` if closed) unless the work is genuinely disjoint AND the user opts
into a separate tree.

## Bootstrap & milestone gates

- **Complete `.oracle/orly.json` on first sight.** `orly init` seeds it
  mechanically — it reads the Makefile (following `include` one level) and
  `package.json` scripts, matches a fixed list of target names, and writes what
  it matched. It finds the obvious ones and misses the rest, and it never
  guesses `surfaces`.
  First session in a repository whose config is still that seed:
  1. Read the build files properly. Fill every `verify.*` the repository really
     has — the tiering is fixed (`conform` + `verify.unit` are the fast tier,
     every other `verify.*` is slow and skips on prose-only branches).
  2. Set `surfaces.user` and `surfaces.docs` to real path prefixes, or the docs
     gate can never fire and a user-visible change ships undocumented.
  3. Add any opt-in pack the repository's own sources cannot imply
     (`persona.indy`, `product.agentsfleet`, `workflow.governance`), then run
     `orly update` so they materialise.
  4. Commit it. Every teammate and every later session reads this file, and
     `orly` never rewrites it — so an edit here is permanent.
  A seeded-but-uncompleted config is why `orly gate` reports a repository with
  no declared commands: the fix is this list, not another `orly init`.
- **Priming:** (1) Human runs `playbooks/founding/01_bootstrap/001_playbook.md`.
  (2) Agent runs `./playbooks/founding/02_preflight/00_gate.sh` (green before
  next). (3) Agent runs `playbooks/founding/03_priming_infra/001_playbook.md`.
  Milestones only after PRIMING_INFRA verified.
- **Credential gate** — milestones needing external creds start `M{N}_001`
  enumerating every downstream credential (name + fetch location). Fail loud
  listing all missing before any `M{N}_002+`.
- **Agent-first sequencing** — minimize human steps; post-handoff steps
  retryable + idempotent. Vault is the inter-step interface; never pass creds
  by argument/env.

## PLAN — expansions

**Quality-ceiling line** — would it be more performant / leaner / a better user
experience (fluid) / sounder under concurrency if built differently, and would a
larger refactor beat the patch? Default solution-size ≈ problem-size; "yes" →
surface option + cost, the user picks. Read docs when behavior is unclear.

**Surface-area checklist** — yes/no + reason each: OpenAPI changes (list paths)
· the product's CLI · user-facing docs · release notes
/ version bump · schema changes (≤100 lines/file, single-concern, update
`schema/embed.zig` + migration array) · Schema Removal Guard output ·
spec-vs-rules conflict (amend spec).

## EXECUTE — spec discipline expansions

- **Golden-path before PLAN approval** — walk the concrete end-to-end with every
  lookup/data-source/secret-storage named; any `[?]` blocks the spec.
- **DONE = called in production + tested** — grep the production entry-point for
  the named symbol; no call → not DONE.
- **Changelog claim challenge** — before any `<Update>`, ask "Would this be true
  if the test file vanished?" Only test evidence (not middleware/handler/CLI) →
  claim unearned.
- **Spec → Code → Test alignment:** every Dimension → test case (no test = not
  implemented); every Interface → exact spec signature (signature change →
  update spec first); every Acceptance Criterion → verifiable command ("works
  correctly" is not a criterion; "`make test` passes" is); no code commits
  without tests (`/orly-write-unit-test`); every Error Table row → negative test.
- **Recovery notes:** local Docker `ENOSPC` → `~/bin/mac-cleanup.sh`, verify
  `docker system df`, retry.

## CHORE (close) — required outputs

- All Dimensions/Sections `DONE` (`IN_PROGRESS` if parked); spec moved
  `docs/v*/active/` → `docs/v*/done/` iff complete.
- Changelog: a new entry on the repository's declared docs surface
  (`surfaces.docs`; `write_changelog` façade where materialised) for
  user-visible changes **AND, re-reading the spec, the affected published docs
  pages revised to match (endpoints/CLI/flags/behavior)** — an entry alone is
  insufficient when documented behavior changes. Product-specific paths: the
  product pack block above.
- `docs/architecture/**` carries a non-empty diff for flow-defining changes, or
  Session Notes says why not (`dispatch/name_architecture.md` covers both homes).
- PR `## Session notes`: decisions, assumptions, dead ends, deferrals,
  `/orly-write-unit-test` + runtime review outcomes, `orly-babysit-prs` final
  report.
- Orphan sweep complete (RULE ORP); ephemeral handoff docs deleted
  (`docs/**/HANDOFF_*.md`, `docs/**/handoff*.md`, `HANDOFF.md` at any depth —
  they brief the next agent, never the PR).
- Pre-commit `git status -uall` audit — every modified/untracked/
  conflict-resolved/hook-managed file staged into the CHORE(close) commit, or
  documented-as-excluded with reason in the commit body; `git status` MUST be
  empty post-commit before opening/updating the PR.
- Version sync where the repository defines it: declared version targets green
  before the PR. Product-specific targets: the product pack block above.

## Deferral discipline — expansion

Any claim that a spec Section/Dimension was "deferred to follow-up" — in
`HANDOFF.md`, PR description, Session Notes, or chat — requires an
**user-acked verbatim quote** in PR Session Notes (or spec Discovery). Format:
`> Indy (YYYY-MM-DD HH:MM): "<verbatim ack>" — context: <which item, why>`.
Agent-unilateral deferral = incomplete scope, not deferral; CHORE(close) blocks
until the item lands or the quote is captured. **HANDOFF.md is a faithful state
report** — a pickup agent reading a HANDOFF claiming items were deferred without
ack-quotes must treat them as in-scope and surface the contradiction to the user
before continuing.

## Pre-PR gates

Spec in `docs/v*/done/` in diff (skip iff parked); the repository's changelog
surface carries a new entry in diff (skip iff internal-only or parked);
`Status: DONE` but spec not in `done/` → do not open PR; the repository's
version-sync check passes where defined; branch contains `origin/main` HEAD
(rebase pre-push / merge post-push — never force-push an open PR branch).

**`orly gate pr` follows the spec through the close.** A spec moved to `done/`
on this branch is still discovered — its `Branch:` header names the branch —
and every spec criterion runs against it; skip-pass is only for genuinely
spec-less branches. Mechanical criteria: `spec.moved` (`Status: DONE` ⇒ the
spec sits under `done/` and was moved on this branch) · `spec.baseline`
(`Test Baseline:` recorded) · `spec.ordering` (the branch's first commit
carries the spec — no code before CHORE(open)) · `spec.deferrals` (a deferral
claim needs the `> Indy (` ack quote in the spec). An Indy-acked deferral that
leaves a Dimension open ships via `orly override spec.dimensions --reason
"<the ack>"` — visible in the PR, dead after the merge.

**Babysit detail:** `orly-babysit-prs` covers all three surfaces — CI check
runs (fix every failure your diff caused), greptile inline comments, and the
PR-level summary thread — and stops on two consecutive empty polls with CI
green. Never `gh pr checks --watch` for greptile.

## LAND (after merge, or when the user confirms it merged)

`git checkout <default> && git pull origin <default>`; prune the merged worktree
and branch; `make down` where the repository defines it. Pending files → stash,
pull, diff against the new default; already-landed → drop.
