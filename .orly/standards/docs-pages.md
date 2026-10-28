# Documentation pages

This standard covers what a published page carries, from front matter and its type's skeleton to content facts, structure and freshness. Contributor architecture sets add reading rules of their own. Scope, language and checks live in [the documentation standard](docs.md).

## Front matter and freshness

Every published page carries these fields.

```yaml
type: tutorial | how-to | reference | explanation | troubleshooting
audience: user | operator | contributor
verified: YYYY-MM-DD
product_version: x.y.z
executable: true | false
```

### DOC-F1 — Front matter uses only the values the block above lists.
- **Applies to:** front matter of published pages; generated API pages and fragments carry none
- **Checked by:** review
- **Bad:** `type: guide`
- **Good:** `type: how-to`
- **Why:** Fixed values let tools sort pages and tell readers what each page holds.

### DOC-F2 — `verified` records the last check against source, not the last prose edit, and `product_version` matches the repository's configured docs version.
- **Applies to:** front matter of published pages
- **Checked by:** review
- **Bad:** `verified` moved forward for a typo fix, with no source check
- **Good:** `verified: 2026-10-28` after rereading the code path the page describes
- **Why:** The date tells readers how far to trust the page.

### DOC-F3 — `executable: true` marks only a page whose runnable commands the repository checker can validate.
- **Applies to:** front matter of published pages
- **Checked by:** review
- **Bad:** `executable: true` on a page whose commands need a live production account
- **Good:** `executable: false` on that page, with each command's expected output shown
- **Why:** A flag the checker cannot honor gives false confidence.

### DOC-31 — `verified` and `product_version` are mandatory, and a page two releases stale is rejected until it is rechecked.
- **Applies to:** published pages
- **Checked by:** review
- **Bad:** `product_version: 0.24.0` on a page never rechecked since, with 0.26.0 released
- **Good:** `product_version: 0.26.0`, with `verified` set on the recheck date
- **Why:** A stale page describes behavior that may no longer ship.

### DOC-32 — Each release runs the fenced commands on every `executable: true` page, and a failing example blocks that release.
- **Applies to:** pages with `executable: true`
- **Checked by:** review
- **Bad:** `A release ships while an example on an executable page fails.`
- **Good:** `The release waits until the example passes or the page is fixed.`
- **Why:** A broken example fails readers on their first try.

The repository's own documentation checks run the commands; the orly gate does not.

## Page shape

Each page type uses the relevant headings below, in order.

| Type | Headings, in order |
|---|---|
| tutorial | What you will build → Before you begin → Steps → Verify it works → What you learned → Related pages |
| how-to | What this does → Before you begin → Steps → Verify it works → Common problems → Remove or undo → Related pages |
| reference | Synopsis → Example with output → Options → Errors → Related pages |
| explanation | What it is → Why it exists → How it behaves → Limits → Related pages |
| troubleshooting | Symptom → Why it happened → How to fix it → How to prevent it |

### DOC-P1 — A page serves one reader goal through one page type, whose relevant headings appear in the table's order.
- **Applies to:** published pages
- **Checked by:** review
- **Bad:** `A tutorial that ends with every flag of every command.`
- **Good:** `A tutorial that links to the command reference.`
- **Why:** Each page type answers one kind of question.

Repository-specific heading requirements still apply.

### DOC-P2 — A page carries only the sections its task needs, keeps prerequisites, verification, recovery and warnings, and invents nothing to fill a heading.
- **Applies to:** published pages
- **Checked by:** review
- **Bad:** `A "Common problems" heading followed by "None yet."`
- **Good:** `No such heading until a real problem is known.`
- **Why:** Empty sections cost reading time and teach readers to skip.

## Content

### DOC-15 — Every runnable command is followed by its expected output, with changing output shown as `<varies>`.
- **Applies to:** fenced `bash` blocks on documentation pages
- **Checked by:** review
- **Bad:** `A bash block with no output block after it.`
- **Good:** `A bash block followed by a text block of its output.`
- **Why:** Readers confirm success by matching what they see.

### DOC-16 — Placeholders are written `<UPPER_SNAKE>`, and the sentence before or after the example says where each value comes from.
- **Applies to:** commands, configuration and code examples on documentation pages
- **Checked by:** review
- **Bad:** `export TOKEN=your-token-here`
- **Good:** `export TOKEN=<API_TOKEN>`, then a sentence saying `<API_TOKEN>` comes from Settings → API Keys
- **Why:** Readers must know which parts to replace and where to find each value.

### DOC-17 — Example credentials are plainly fake, such as `af_test_00000000`.
- **Applies to:** credentials, tokens and keys in examples
- **Checked by:** review
- **Bad:** `A real-looking key pasted from a terminal session.`
- **Good:** `af_test_00000000`
- **Why:** A real-looking key teaches readers to paste secrets and trips secret scanners.

### DOC-18 — A destructive command follows a warning that states what is lost, and its `--dry-run` form comes first when the command has one.
- **Applies to:** commands that delete, overwrite or revoke
- **Checked by:** review
- **Bad:** `A delete command shown with no warning.`
- **Good:** `A warning naming the lost data, then the --dry-run form, then the delete command.`
- **Why:** Readers copy commands, and a deletion cannot be undone from the page.

### DOC-19 — Every option states its effect, default, unit and valid range, with "none" where no default or range exists.
- **Applies to:** option, flag and setting references
- **Checked by:** review
- **Bad:** `--timeout  Sets the timeout.`
- **Good:** `--timeout  Seconds to wait for the daemon. Default 30. Range 1 to 600.`
- **Why:** Readers set options from the page, not from the source.

### DOC-20 — Every resource states who creates, owns and deletes it, its limit, and what happens at that limit.
- **Applies to:** pages that introduce a resource a user can create
- **Checked by:** review
- **Bad:** `Each account has API keys.`
- **Good:** `You create and delete API keys; an account holds 10, and creating an eleventh fails with an error.`
- **Why:** Readers plan capacity and cleanup from these facts.

### DOC-21 — Every feature states what fails, what retries with what count and delay, what data is lost and what survives.
- **Applies to:** feature, how-to and reference pages
- **Checked by:** review
- **Bad:** `Deliveries are retried on failure.`
- **Good:** `A failed delivery retries 3 times, 30 seconds apart, then is dropped; its delivery record survives.`
- **Why:** Operators recover only from failures they can predict.

### DOC-22 — User pages state guarantees rather than internals, and operator pages state exact dependencies and versions.
- **Applies to:** pages by their `audience` value
- **Checked by:** review
- **Bad:** `Requires a recent Postgres.`
- **Good:** `Requires Postgres 16 or later.`
- **Why:** Each audience acts on different facts.

### DOC-23 — Numbers carry units, dates on a published page use ISO 8601, and times use Coordinated Universal Time (UTC).
- **Applies to:** prose and tables on published pages
- **Checked by:** review
- **Bad:** `Runs every 5 at 9pm on 10/28.`
- **Good:** `Runs every 5 minutes, at 21:00 UTC on 2026-10-28.`
- **Why:** Unitless numbers and local dates mean different things to different readers.

### DOC-24 — Every product error has a stable identifier and an anchor that says why it happened, how to fix it and how to prevent it.
- **Applies to:** error references and pages that show product errors
- **Checked by:** review
- **Bad:** `Error: request failed`
- **Good:** `AUTH-003`, linked to an anchor with its cause, fix and prevention
- **Why:** Readers search for the code and should land on the fix.

## Structure

### DOC-25 — A page has one level-one heading, skips no heading level, and redirects its old path when renamed.
- **Applies to:** `*.md` and `*.mdx` pages; fragments are exempt
- **Checked by:** gate: docs.language
- **Bad:** `# Setup` followed directly by `### Options`
- **Good:** `# Setup` followed by `## Options`
- **Why:** Headings are the page outline for readers, search and screen readers.

The gate checks heading levels; review checks redirects.

### DOC-26 — A procedure is a numbered list with one action per item and one command per runnable block.
- **Applies to:** steps on tutorial and how-to pages
- **Checked by:** review
- **Bad:** `1. Install the CLI, log in and create a project.`
- **Good:** `1. Install the CLI.` `2. Log in.` `3. Create a project.`
- **Why:** Readers track progress by step number and recover at the step that failed.

### DOC-27 — Tables hold comparisons, and procedures never sit in a table.
- **Applies to:** tables on documentation pages
- **Checked by:** review
- **Bad:** `A table whose rows are setup steps.`
- **Good:** `A numbered list of steps, and a table comparing plans.`
- **Why:** Steps in a table lose their order and their room for output.

### DOC-28 — A fence names its intent, `bash` for runnable commands, `yaml` or `json` for configuration and `text` for output.
- **Applies to:** fenced code blocks on documentation pages
- **Checked by:** review
- **Bad:** `Command output fenced as bash.`
- **Good:** `Command output fenced as text.`
- **Why:** The tag tells readers and checkers which blocks to run.

### DOC-29 — A page completes its stated task alone, and its links add depth without hiding a prerequisite.
- **Applies to:** tutorial and how-to pages
- **Checked by:** review
- **Bad:** `See the setup page first.` in place of the one step that needs a token
- **Good:** `Create a token in Settings → API Keys.`, with a link for more detail
- **Why:** A reader sent elsewhere for a prerequisite often does not come back.

### DOC-30 — Interface paths are written as **Settings → API Keys → Create Key**, and screenshots never carry unique instructions.
- **Applies to:** pages that describe a user interface
- **Checked by:** review
- **Bad:** `Click the gear, then the keys tab, as the screenshot shows.`
- **Good:** `Open Settings → API Keys → Create Key.`
- **Why:** Text paths survive redesigns and screen readers, and screenshots do not.

## Contributor architecture sets

These rules add to the rules above for `docs/architecture/**`, `AUTH*.md` and `development.md` in product repositories, wherever the two do not conflict.

### DOC-36 — An architecture page opens with its answer, a facts table or a "read this when" line, before any reasoning.
- **Applies to:** `docs/architecture/**`, `AUTH*.md` and `development.md`
- **Checked by:** review
- **Bad:** `Three paragraphs of history before the page says which store holds sessions.`
- **Good:** `A facts table naming the store, the key and the expiry, then the reasoning.`
- **Why:** Readers arrive with a question and should leave with the answer quickly.

### DOC-37 — The set's index maps each reader question or symptom to one section, and no two index rows share a target.
- **Applies to:** the index page of a contributor architecture set
- **Checked by:** review
- **Bad:** `Two index rows, "Login fails" and "Token expired", pointing at one section.`
- **Good:** `One row per section, phrased as the question that section answers.`
- **Why:** An index that splits one answer across rows sends readers in circles.

### DOC-38 — Each fact lives on the page that owns it, and every other mention is a pointer to that page.
- **Applies to:** contributor architecture sets
- **Checked by:** review
- **Bad:** `The lease timeout restated on four pages.`
- **Good:** `The lease page states the timeout, and the other pages link to it.`
- **Why:** A fact stated once has nothing to reconcile and cannot drift.

### DOC-39 — Topology gets a box diagram, flows get a sequence, comparisons get a table, and reasoning stays prose.
- **Applies to:** contributor architecture sets
- **Checked by:** review
- **Bad:** `A paragraph tracing one request through five services.`
- **Good:** `A sequence diagram of the request, with every box border aligned.`
- **Why:** Structure reads faster as a picture than as prose.

A diagram's borders align, and a broken box is a defect.

### DOC-40 — Typed callouts mark asides by kind, real brand marks name products, and concept emoji appear only in tables and lists, never in headings.
- **Applies to:** contributor architecture sets
- **Checked by:** review
- **Bad:** `## 🚀 Deploying`
- **Good:** `## Deploying`, with a warning callout before the destructive step
- **Why:** Specs cite heading text verbatim, and color that means nothing is noise.

### DOC-41 — A documented failure gives its code, its cause and the exact command that fixes it, linking to the published error registry for more.
- **Applies to:** contributor architecture sets and the error sections they hold
- **Checked by:** review
- **Bad:** `If it fails, check your setup.`
- **Good:** `CFG-001 means the config file is missing, and the init command creates it.`
- **Why:** A reader facing a failure needs the next command to run.

### DOC-42 — A story told at several zoom levels names each lens and when to read it, and the lenses do not repeat each other.
- **Applies to:** contributor architecture sets
- **Checked by:** review
- **Bad:** `An overview page and a detail page that walk the same flow twice.`
- **Good:** `An overview for operators and a detail page for contributors, each saying whom it serves.`
- **Why:** Repeated lenses drift apart and double the reading.
