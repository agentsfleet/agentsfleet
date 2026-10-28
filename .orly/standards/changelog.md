# Changelog

This standard covers changelog entries and user-visible release notes, from what earns an entry to its voice, format and version bump. The [documentation standard](docs.md) applies first, and these rules add to it.

## What earns an entry

### CL-USER — A changelog entry exists only for a change a reader can observe, and internal-only cleanup gets none.
- **Applies to:** `changelog.mdx`, any changelog the repository declares, and user-visible release notes
- **Checked by:** review
- **Bad:** `Refactored the retry module into three files.`
- **Good:** `No entry, because nothing a reader sees changed.`
- **Why:** Entries about internals bury the changes readers must act on.

A refactor earns an entry only through a behavior change a reader can observe.

### CL-EARNED — A changelog claim names behavior a shipped code path delivers, so it would stay true if the test files vanished.
- **Applies to:** every claim in an entry
- **Checked by:** review
- **Bad:** `Webhook retries are now covered by 40 new tests.`
- **Good:** `A failed webhook delivery now retries three times.`
- **Why:** Only behavior reachable through a handler, a command or a page is something a reader can see.

## Voice

### CL-LEAD — An entry has one descriptive headline and one or two lead sentences that state the change, never an announcement.
- **Applies to:** the headline and lead of every entry
- **Checked by:** review
- **Bad:** `We're excited to announce smarter retries!`
- **Good:** `Failed deliveries now retry from the dashboard.`
- **Why:** Readers scan headlines to learn what changed.

The lead speaks from the reader's side, as in `you can now` or `X now does Y`, rather than as implementation prose.

### CL-HYPE — An entry uses none of the DOC-07 marketing words, nor `magical`, `we are pleased to` or `we're excited to`.
- **Applies to:** changelog entries and release notes
- **Checked by:** gate: docs.language
- **Bad:** `A magical new retry flow.`
- **Good:** `Retries now wait 30 seconds between attempts.`
- **Why:** The change speaks for itself.

The gate flags the phrases and the DOC-07 words; `magical` is a review check.
A vendor's marketing term quoted verbatim sits inside quotation marks.

### CL-BUDGET — An entry holds at most five bullets and 120 prose words, chosen from changes a reader can see.
- **Applies to:** every new entry
- **Checked by:** review
- **Bad:** `Nine bullets listing test counts, planning identifiers and every file the release touched.`
- **Good:** `Three bullets, with related fixes grouped, each a change a reader sees.`
- **Why:** A short entry gets read, and a long one gets skipped.

Only upgrade or safety steps justify a longer entry, and review notes outside the entry record the reason.

### CL-BULLET — Each bullet follows `**Bold lead-noun** — consequence-first clause` and states one fact, with code names in backticks.
- **Applies to:** bullets in changelog entries
- **Checked by:** review
- **Bad:** `- Retries and timeouts and logging were improved and fixed.`
- **Good:** `- **Retries** — A failed delivery retries three times.`
- **Why:** A bold noun lets readers scan, and one fact per bullet keeps each claim checkable.

A sentence with three uses of `and` splits in two.
Functions, paths, environment variables, routes, error codes and tables always sit in backticks.

## Facts readers act on

### CL-ACT — Facts a reader needs to act, such as breaking changes, migration steps, defaults, limits and money amounts, stay explicit with their exact tokens.
- **Applies to:** every entry
- **Checked by:** review
- **Bad:** `Some defaults changed; see the docs.`
- **Good:** `The max_workers default is now 8, up from 4.`
- **Why:** A reader cannot act on a change the entry only hints at.

### CL-LINK — An entry links to the canonical reference for full endpoint shapes, option tables and error lists, and a link never hides a prerequisite or upgrade step.
- **Applies to:** every entry
- **Checked by:** review
- **Bad:** `Upgrade steps are in the migration guide.` for a step every reader must take
- **Good:** `Run the migrate command before restarting, then see the reference for every new option.`
- **Why:** Links keep entries short, and required steps belong where readers act.

### CL-RATE — A money or rate claim matches the repository's canonical billing constants, and its published rate tables change with them.
- **Applies to:** entries and pages that state prices, rates or credits
- **Checked by:** review
- **Bad:** `A rate typed into the entry from memory.`
- **Good:** `A rate imported from the published rate fragment, which matches the billing constants.`
- **Why:** Constants declared once give a money claim one thing to match.

A page that shows a rate imports its named value instead of copying the amount.

## History

### CL-HISTORY — Released entries stay as written, and an owner-requested edit keeps their dates, behavior, technical tokens and required actions.
- **Applies to:** every entry already released
- **Checked by:** review
- **Bad:** `A release restyles last year's entries to the new bullet format.`
- **Good:** `Old entries untouched, with any factual correction named in its commit message.`
- **Why:** The changelog is the record of what shipped and when.

### CL-DATEREF — An entry refers to an earlier entry by its date, never by its semantic version.
- **Applies to:** entry bodies
- **Checked by:** review
- **Bad:** `the retry flag shipped in v0.27.0`
- **Good:** `the retry flag that shipped on Apr 22, 2026`
- **Why:** Dates keep the changelog's timeline apart from the version timeline.

## Format and versions

### CL-FORMAT — Entries use the repository's declared changelog, format and version source, and Mintlify block rules apply only where the changelog already uses Mintlify.
- **Applies to:** every changelog, including one published on the Mintlify docs platform
- **Checked by:** review
- **Bad:** `A Markdown changelog that gains an <Update> block.`
- **Good:** `A Markdown entry with the same upgrade, behavior and API facts.`
- **Why:** Installing a standard does not change how a repository publishes.

In a Mintlify changelog, a new `<Update>` block goes above older entries, after any leading `<Tip>` or `<Note>`.

### CL-LABEL — A Mintlify `<Update>` label is the date as `MMM DD, YYYY`, with no version prefix or release name.
- **Applies to:** Mintlify `<Update>` blocks
- **Checked by:** review
- **Bad:** `label="v0.27.0 — Apr 22, 2026"`
- **Good:** `label="Apr 22, 2026"`
- **Why:** The label is the timeline readers navigate by.

Two releases on one date share one block, or the title carries a disambiguator such as `## Follow-up — …` while the label stays the date.

### CL-ORDER — Entry sections appear in the fixed order `Upgrading`, `What's new`, `API reference`, `Bug fixes`, `CLI`, and an empty section is left out.
- **Applies to:** sections inside an entry
- **Checked by:** review
- **Bad:** `## Bug fixes` above `## Upgrading`, or an empty `## CLI` heading
- **Good:** `## Upgrading` first, and no heading without content
- **Why:** Readers find the action they must take in the same place every time.

### CL-UPGRADE — Every breaking change appears under `Upgrading` with an explicit migration step and whether the command-line interface (CLI) and server must upgrade together.
- **Applies to:** entries with a breaking change
- **Checked by:** review
- **Bad:** `A renamed flag mentioned only under What's new.`
- **Good:** `Upgrading: replace --project with --project-id, and upgrade the CLI and the server together.`
- **Why:** A break without a step leaves readers broken after they upgrade.

This holds even when another section mentions the same break.

### CL-NOREF — An entry's title and body carry no milestone or workstream identifiers, codenames, branch names, spec filenames or rule references.
- **Applies to:** entry titles and bodies
- **Checked by:** review
- **Bad:** `## M42 retry workstream, per RULE OBS`
- **Good:** `## Retry failed deliveries`
- **Why:** Internal plan names mean nothing to the readers of a changelog.

### CL-BUMP — A feature release bumps the minor version, a fix bumps the patch, and a breaking change bumps the minor before 1.0 and the major after.
- **Applies to:** the repository's version source at release
- **Checked by:** review
- **Bad:** `A breaking change released as 0.7.1.`
- **Good:** `The same change released as 0.8.0, with the break under Upgrading.`
- **Why:** The version number tells readers how careful an upgrade must be.

Internal-only cleanup gets no entry, and a reader-visible effect follows the normal entry rules.

### CL-SYNC — Parallel branches reconcile the version source against the current default branch and pass the repository's declared synchronization check.
- **Applies to:** branches that bump the version source
- **Checked by:** review
- **Bad:** `Two branches both bump to 0.8.0 and merge.`
- **Good:** `The second branch merges main, sees 0.8.0 and moves to 0.9.0.`
- **Why:** Two branches claiming one version publish two different releases under one number.

Reconciling never authorizes rebasing or force-pushing a published branch.

## Mintlify block template

A Mintlify entry fills this block.

```mdx
<Update label="MMM DD, YYYY" tags={["What's new" | "Breaking" | "Bug fixes", "API" | "CLI" | "UI" | "Security" | "Performance" | "Integrations" | "Observability" | "Internal", ...]}>
  ## {Short user-facing title, with no milestone identifiers or codenames}

  {One paragraph from the reader's side.}

  ## Upgrading
  {Breaking changes only, first when present, each with its migration step and whether CLI and server upgrade together.}

  ## What's new
  {What a user or operator can now do.}

  ## API reference
  {New or changed endpoints, shapes and error codes, with examples. Omit when the API did not change.}

  ## Bug fixes
  {User-visible bugs fixed, with behavior before and after. Omit when none.}

  ## CLI
  {Command additions or shape changes. Omit when none.}
</Update>
```

A complete entry reads like this.

```md
## Retry failed deliveries

You can retry a failed webhook delivery from the dashboard.

- **Retries** — Open the failed delivery and select **Retry**.
- **Duplicates** — A delivery already running cannot be retried.
```
