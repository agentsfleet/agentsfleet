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
- A fresh linked worktree requires `bun install`, followed by
  `(cd cli && bun install && bun run build)` before repository tests.
- Public endpoint, command, flag, or behavior changes require a matching branch
  in `~/Projects/docs`; never edit that repository through this worktree.
