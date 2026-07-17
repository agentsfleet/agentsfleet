# Oracle rules architecture

Oracle rules use one canonical registry, explicit profiles, generated agent-home
instructions, and tracked repository snapshots. This removes implicit worktree
mutation while keeping every repository's existing verification commands.

## Topology

```text
oracle-rules/core/operating-model.md
oracle-rules/packs/**
oracle-rules/profiles/*.json
oracle-rules/registry.json
              │
              ▼
       bin/oracle-rules
          │         │
          │         └── sync ──> repository snapshot + ruleset lock
          │
          └── render global ──> oracle-rules/generated/global/AGENTS.md
                                      │
                                      └── agent-home symbolic links
```

`AGENTS.project.md` is the only repository-authored input appended to a
snapshot. Generated files are ordinary tracked files, never cross-repository
symbolic links.

## Reference closure

Every repository snapshot is reference-closed: each relative Markdown link and
each `dispatch/*.md` target named by generated prose resolves inside that
repository. Profile markers remove operating-model rows and blocks whose packs
are not selected. Pack façades remain self-contained canonical files rather than
depending on dotfiles-only design documents.

The `dotfiles` source profile deliberately selects the full pack inventory so its
invariance audit exercises every route. Consumer profiles remain selective.

Rendering fails before writing a lock when a selected snapshot contains a
missing or escaping reference. The `global` profile is the deliberate exception:
it is an agent-home routing overlay whose targets are supplied by the active
repository snapshot, not a repository snapshot itself.

## Propagation

Global agent-home instructions update in place after:

```bash
oracle-rules render --profile global --output ~/Projects/dotfiles/oracle-rules/generated/global
oracle-rules link-agent-homes
```

The agent-home links point at that generated file, so later renders become
visible immediately to installed agents.

Repository snapshots update only through an explicit clean-tree operation:

```bash
oracle-rules status --all
oracle-rules sync --repository <NAME>
```

Synchronization refuses a dirty repository, an absent `AGENTS.project.md`, an
unmanaged destination, and an unknown profile. It never walks or mutates sibling
worktrees.

## Repository setup

1. Add the repository path and profile to `oracle-rules/repositories.json`.
2. Run `oracle-rules init --profile <PROFILE> --repository <PATH>`.
3. Fill `AGENTS.project.md` with local commands, terms, and architecture triggers.
4. Commit `AGENTS.project.md` and `.oracle/profile.json` in that repository.
5. Run `oracle-rules sync --repository <NAME>` from dotfiles.
6. Review and commit the generated snapshot and `.oracle/ruleset.lock`.
7. Run `oracle-rules doctor --repository <NAME>`.

The clean-tree boundary makes every propagation reviewable. A repository cannot
receive a partial update while unrelated work is present.

## Lifecycle command mapping

`CONFORM`, `VERIFY`, and `REVIEW` are responsibilities, not command names.
Profiles map those responsibilities to repository commands. The `agentsfleet`
profile maps `CONFORM` to `make harness-verify`; that command remains unchanged.

`VERIFY` runs the profile's correctness commands. `REVIEW` challenges the diff
after those commands pass. No additional lifecycle stage is needed.

## Evidence

`oracle-rules verify --all` validates every profile and renders each profile
twice. Byte-identical output proves deterministic generation. With
`--write-evidence`, the command writes `.oracle/evidence.json` containing the
source commit, registry digest, checks, and live comprehension result.

Repository snapshots carry `.oracle/ruleset.lock`. `oracle-rules doctor` verifies
each managed file's bytes and normalized mode (`0644` or `0755`), rejects managed
symbolic links, and reports an outdated profile-specific ruleset digest.
