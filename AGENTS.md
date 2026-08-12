# `agentsfleet` repository instructions

The operating model is global: every agent home symlinks to
`~/Projects/dotfiles/AGENTS.md`, and rule pages plus gate scripts resolve from
that checkout. This file carries only project facts.

- Write the product as `agentsfleet`; binaries are `agentsfleetd` and
  `agentsfleet-runner`. API entities use `fleet`, `fleet_id`, and `/fleets`.
- Drive work with `orly gate` (work → verify → pr). `make harness-verify`
  satisfies CONFORM only; behavioral verification uses the profile's
  `verify.*` commands (`make lint-all`, `make test-unit-all`,
  `make test-integration`, `make memleak`, `make check-version`). REVIEW
  remains a separate lifecycle stage.
- **Make targets are the only repository claims — never hand-roll their
  equivalents.** CONFORM → `make harness-verify` · lint → `make lint-all` ·
  unit → `make test-unit-all` · integration → `make test-integration` ·
  leaks → `make memleak` · version → `make check-version` · dry lanes →
  `make dry-app` / `make dry` · drain audit → `make check-pg-drain`. A
  package-scoped runner (`cd ui/packages/app && bun run test`, `zig build
  test`, …) is inner-loop iteration; it proves a package, not the
  repository, and never satisfies a VERIFY row or a "tests pass" claim.
- A fresh linked worktree requires `bun install`, followed by
  `(cd cli && bun install && bun run build)` before repository tests.
  `.githooks/post-checkout` links `ui/packages/app/.env.local` and
  `.env.runner.local` from `~/.config/agentsfleet/`; a ⚠ from the hook
  means run `provision-env-1password` (dotfiles) first. The app throws on
  an unset `NEXT_PUBLIC_API_URL` instead of guessing a backend.
- Public endpoint, command, flag, or behavior changes require a matching branch
  in `~/Projects/docs`; never edit that repository through this worktree.
- **Never read a lane's result through a pipe.** `make … | tail` reports the
  exit status of `tail`, so a failing lane reads as green. Redirect to a file
  and echo `$?`, or check `PIPESTATUS`. This has produced a false "integration
  passed" more than once. Note also that `failed command:` in `zig build`
  output is NOT a failure signal — it appears on successful runs too; the exit
  code decides.
- **Commit before restructuring, especially with untracked files present.** A
  restructure that deletes an untracked file destroys it — git has no copy to
  restore, and `git status` shows nothing missing afterwards. Work that took a
  full review round to produce has been lost this way. Commit the working
  state first, then restructure on top of it.
- **One integration lane per machine.** `make test-integration` and
  `make memleak` are per-worktree but not per-machine: several worktrees
  running them at once drive load high enough that timeout-bounded tests
  (Redis reconnect, SSE latency) fail on timing alone, in files the branch
  never touched. Check `pgrep -f agentsfleetd-integration-tests` before
  starting one, and treat a failure in an untouched file under load as suspect
  until it reproduces on a quiet host.
