# Release documentation template

Use the consuming repository's declared changelog and version source.
Preserve its selected format; a Markdown changelog does not acquire a Mintlify
site, sibling checkout or `VERSION` file by installing this pack.
Use its declared synchronization command when it maintains generated versions.

For `agentsfleet`, the changelog is `~/Projects/docs/changelog.mdx` and `VERSION`
is the binary version source. `make sync-version` updates `build.zig.zon`,
`agentsfleet/package.json` and `agentsfleet/src/cli.js`.
New `<Update>` blocks go after the leading `<Tip>` or `<Note>`.

For a repository already using Mintlify, use the block and label rules below.
Other formats carry the same applicable upgrading, behavior and API information
in their existing entry structure.

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
  {Command additions or shape changes. Omit if none.}
</Update>
```

## Hard rules

- Label is `MMM DD, YYYY` exactly — no `vX.Y.Z —` prefix, no release name. Two releases on the same date → one merged `<Update>` block OR a disambiguator inside the title (`## Morning release — …`, `## Follow-up — …`); the label stays the date.
- Section order is fixed: Upgrading → What's new → API reference → Bug fixes → CLI. Omit empty sections; never leave empty headings.
- No milestone/workstream IDs, branch names, spec filenames, or `RULE XXX` references in the body.
- User-centric verbs ("we added", "X now does Y"), not implementation prose.
- Every breaking change appears under `Upgrading` with a migration step, even if also mentioned elsewhere.
- Body copy may reference a past entry by date (`"…that shipped on Apr 22, 2026"`); do not reference past releases by semver (`"shipped in v0.27.0"`) — that drags the two timelines back together.

## Version bumps (the repository's version source)

- Feature milestone → minor (`0.7.0` → `0.8.0`).
- Bug fix → patch.
- Pre-v1.0 breaking → minor (semver 0.x carve-out); call out under Upgrading.
- Post-v1.0 breaking → major.
- Internal-only refactor: terse `<Update>` with `tags={["Internal", ...]}`, one-paragraph summary, skip section structure. Prefer folding into the next user-visible release.
- Parallel branches reconcile their version source against the current default
  branch and run the repository's declared synchronization check. This never
  authorizes rebasing or force-pushing a published branch.
