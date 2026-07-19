# Orly architecture

Orly uses one canonical registry, explicit profiles, generated agent-home
instructions, and tracked repository snapshots. This removes implicit worktree
mutation while keeping every repository's existing verification commands.

## Topology

```text
orly/core/operating-model.md
orly/packs/**
orly/profiles/*.json
orly/registry.json
              │
              ▼
        bin/orly
          │
          ├── sync repository ──> tracked snapshot + ruleset lock
          │
          └── sync global ──────> orly/generated/global/AGENTS.md
                                      │
                                      └── agent-home symbolic links
```

`AGENTS.project.md` is the only repository-authored input appended to a
snapshot. Generated files are ordinary tracked files, never cross-repository
symbolic links.

## Reference closure

Every repository snapshot is reference-closed: each relative Markdown link and
each `dispatch/*.md` target named by any rendered Markdown file resolves inside
that repository. Profile markers remove operating-model rows and blocks whose
packs are not selected, and filter managed Markdown documents the same way —
excluded lines are dropped while included lines keep their marker verbatim, so
the source profile's self-render stays byte-stable. Pack façades remain
self-contained canonical files rather than depending on dotfiles-only design
documents.

The `dotfiles` source profile deliberately selects the full pack inventory so its
invariance audit exercises every route. Consumer profiles remain selective.

Rendering fails before writing a lock when a selected snapshot contains a
missing or escaping reference. The `global` profile is the deliberate exception:
it is an agent-home routing overlay whose targets are supplied by the active
repository snapshot, not a repository snapshot itself.

## Propagation

Update global agent-home instructions:

```bash
orly sync --global
```

The agent-home links point at that generated file, so later renders become
visible immediately to installed agents.

Update registered repository snapshots:

```bash
orly sync --all
```

Synchronization refuses a dirty repository, an absent `AGENTS.project.md`, an
unmanaged destination, and an unknown profile. It never walks or mutates sibling
worktrees.

## Repository setup

1. Add the repository path and profile to `orly/repositories.json`.
2. Run `orly adopt <REPOSITORY_NAME>` from dotfiles.
3. Review and commit the generated snapshot in the target repository.
4. Run `orly doctor <REPOSITORY_NAME>`.

The clean-tree boundary makes every propagation reviewable. A repository cannot
receive a partial update while unrelated work is present.

## Lifecycle command mapping

`CONFORM`, `VERIFY`, and `REVIEW` are responsibilities, not command names.
Profiles map those responsibilities to repository commands. The `agentsfleet`
profile maps `CONFORM` to `make harness-verify`; that command remains unchanged.

`VERIFY` runs the profile's correctness commands. `REVIEW` challenges the diff
after those commands pass. No additional lifecycle stage is needed.

The REVIEW command is runtime-specific. Codex first uses native `/review`, or
`codex review` when invoked non-interactively, and then invokes gstack
`$review`. Claude, OpenCode, and Amp use gstack `/review`. The Codex sequence
provides two independent local pre-commit reviews; post-push reviewer triage
remains separate.

## Evidence

`orly verify --all` validates every profile and renders each profile
twice. Byte-identical output proves deterministic generation. With
`--write-evidence`, the command writes `.oracle/evidence.json` containing the
source commit, registry digest, checks, and live comprehension result.

Repository snapshots carry `.oracle/ruleset.lock`. `orly doctor` verifies
each managed file's bytes and normalized mode (`0644` or `0755`), rejects managed
symbolic links, and reports an outdated profile-specific ruleset digest.
