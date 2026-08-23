# Release doc template — `changelog.mdx` `<Update>` block

Source of truth: `~/Projects/docs/changelog.mdx`. New `<Update>` blocks go at the top, after the leading `<Tip>`/`<Note>`. Labels are date-only — never a semver prefix. `VERSION` is the single source of truth for binary version (binaries via `make sync-version` propagate to `build.zig.zon`, `agentsfleet/package.json`, `agentsfleet/src/cli.js`); changelog stays chronological. Decoupled by design — avoids parallel-branch collisions.

## Block template

```mdx
<Update label="MMM DD, YYYY" tags={["What's new" | "Breaking" | "Bug fixes", "API" | "CLI" | "UI" | "Security" | "Performance" | "Integrations" | "Observability" | "Internal", ...]}>
  ## {Short user-facing feature title — no milestone IDs, no codenames}

  {One paragraph from the user's perspective. No workstream numbers, branch names, RULE references.}

  ## Upgrading
  {Breaking changes only. ALWAYS first when present. Each break: explicit migration step + whether CLI+server must upgrade together.}

  ## What's new
  {New capabilities — what a user/operator can now do.}

  ## API reference
  {New/changed endpoints, shapes, error codes. Include JSON/route examples. Omit if no API change.}

  ## Bug fixes
  {User-visible bugs fixed — observed behavior before/after. Omit if none.}

  ## CLI
  {`agentsfleet` additions or shape changes. Omit if none.}
</Update>
```

## Hard rules

- Label is `MMM DD, YYYY` exactly — no `vX.Y.Z —` prefix, no release name. Two releases on the same date → one merged `<Update>` block OR a disambiguator inside the title (`## Morning release — …`, `## Follow-up — …`); the label stays the date.
- Section order is fixed: Upgrading → What's new → API reference → Bug fixes → CLI. Omit empty sections; never leave empty headings.
- No milestone/workstream IDs, branch names, spec filenames, or `RULE XXX` references in the body.
- User-centric verbs ("we added", "X now does Y"), not implementation prose.
- Every breaking change appears under `Upgrading` with a migration step, even if also mentioned elsewhere.
- Body copy may reference a past entry by date (`"…that shipped on Apr 22, 2026"`); do not reference past releases by semver (`"shipped in v0.27.0"`) — that drags the two timelines back together.

## Version bumps (`VERSION`, not the changelog label)

- Feature milestone → minor (`0.7.0` → `0.8.0`).
- Bug fix → patch.
- Pre-v1.0 breaking → minor (semver 0.x carve-out); call out under Upgrading.
- Post-v1.0 breaking → major.
- Internal-only refactor: terse `<Update>` with `tags={["Internal", ...]}`, one-paragraph summary, skip section structure. Prefer folding into the next user-visible release.
- Parallel branches bumping `VERSION` do not coordinate through the changelog — whichever lands second rebases `VERSION` and re-runs `make sync-version`.
