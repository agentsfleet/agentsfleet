# `agentsfleet` repository instructions

- Write the product as `agentsfleet`; binaries are `agentsfleetd` and
  `agentsfleet-runner`. API entities use `fleet`, `fleet_id`, and `/fleets`.
- `make harness-verify` satisfies CONFORM only. Use the repository-command table
  above for behavioral verification; REVIEW remains a separate lifecycle stage.
- A fresh linked worktree requires `bun install`, followed by
  `(cd cli && bun install && bun run build)` before repository tests.
- Public endpoint, command, flag, or behavior changes require a matching branch
  in `~/Projects/docs`; never edit that repository through this worktree.
