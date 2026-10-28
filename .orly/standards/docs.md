# Documentation

This standard covers the scope, language, checks and editing pass for every documentation page a repository writes, renders or installs. What a page must carry lives in [the page standard](docs-pages.md), and changelog entries add [their own rules](changelog.md). A page is complete when a human or a large language model (LLM) can answer how, what and why without guessing.

## Scope and precedence

### DOC-S1 — Every Markdown or Markdown JSX (MDX) page a repository writes, renders or installs follows every applicable documentation rule.
- **Applies to:** published pages, installed rule pages, skill pages and `README.md` files
- **Checked by:** review
- **Bad:** `A skill page skips the language rules because nobody publishes it.`
- **Good:** `The skill page meets the same language rules as a published page.`
- **Why:** Readers meet installed pages as often as published ones.

Narrower standards, such as the changelog standard, add rules on top of these.

### DOC-S2 — OpenAPI source carries reference-page intent through its summaries, descriptions, parameters, examples, responses and stable errors.
- **Applies to:** public OpenAPI documents and the API pages generated from them
- **Checked by:** review
- **Bad:** `summary: Handles projects`, with no error responses listed
- **Good:** `summary: Create a project`, with each error response and its stable code
- **Why:** A generated page can only say what its source says.

Generated API pages need no MDX front matter.

### DOC-S3 — A reusable fragment follows the language and content rules but carries no page front matter or page skeleton.
- **Applies to:** `snippets/*.mdx`
- **Checked by:** review
- **Bad:** `A fragment that opens with front matter and a "Before you begin" heading.`
- **Good:** `A fragment that holds the shared paragraph and nothing page-shaped.`
- **Why:** A fragment lands inside many pages, and each page owns its own structure.

### DOC-S4 — The changelog follows the changelog standard, and its historical entries keep their wording and their original shape.
- **Applies to:** `changelog.mdx` and any changelog the repository declares
- **Checked by:** review
- **Bad:** `A release restyles last year's entries into the page skeletons.`
- **Good:** `A release adds its own entry and leaves older entries untouched.`
- **Why:** History records what shipped, and restyling it loses that record.

The [changelog standard](changelog.md) holds the voice and format rules.

### DOC-S5 — Pages spell the product and its terms the way the repository glossary does, and the glossary wins where it is stricter.
- **Applies to:** every documentation page
- **Checked by:** review
- **Bad:** `AcmeCloud` on a page whose glossary spells `acmecloud`
- **Good:** `acmecloud`, as the glossary spells it
- **Why:** One spelling lets readers and search find every mention.

### DOC-S6 — Runtime code is the source of truth, OpenAPI and command-line help follow it, and pages follow all three.
- **Applies to:** every behavior claim on a documentation page
- **Checked by:** review
- **Bad:** `A page copies a default from an old spec.`
- **Good:** `A page states the default the code sets, read from the source.`
- **Why:** Code is what users run, so every other source can drift.

### DOC-S7 — A historical record keeps its original wording and is never rewritten to a later rule.
- **Applies to:** spec records under `docs/v*/` and closed changelog entries
- **Checked by:** review
- **Bad:** `A cleanup pass rewrites a closed spec to today's word list.`
- **Good:** `The closed spec keeps its words, and new pages follow the new rule.`
- **Why:** A record shows what was decided at the time.

## Language

### DOC-01 — A page assumes a first-day reader with no product knowledge.
- **Applies to:** every documentation page
- **Checked by:** review
- **Bad:** `Rotate the key the usual way.`
- **Good:** `Open Settings → API Keys and select Rotate.`
- **Why:** The reader who needs a page most often has the least context.

### DOC-02 — Each prose sentence carries one idea in at most 25 words.
- **Applies to:** prose in `*.md` and `*.mdx` files; code and tables are exempt
- **Checked by:** gate: docs.language
- **Bad:** `Run the command and check the output, and if it fails read the logs, which live in the data directory unless you moved it.`
- **Good:** `Run the command. If it fails, read the logs in the data directory.`
- **Why:** Long sentences hide the action the reader came for.

### DOC-03 — A prose paragraph holds at most three sentences.
- **Applies to:** prose paragraphs and list items in `*.md` and `*.mdx` files
- **Checked by:** gate: docs.language
- **Bad:** `One paragraph covering setup, defaults, limits and recovery in six sentences.`
- **Good:** `Two short paragraphs, or a list, with at most three sentences each.`
- **Why:** Short paragraphs let a reader find the one fact they need.

### DOC-04 — Sentences use active voice with a named actor, the reader or the product, never "the system".
- **Applies to:** prose on documentation pages
- **Checked by:** review
- **Bad:** `The token is rotated by the system.`
- **Good:** `The daemon rotates the token every 24 hours.`
- **Why:** Readers need to know who acts, themselves or the product.

### DOC-05 — Prose uses plain words in place of `utilise`, `utilize`, `facilitate`, `leverage`, `orchestrate`, `instantiate`, `terminate`, `provision`, `execute`, `persist`, `hydrate` and `artifact`.
- **Applies to:** prose on documentation pages, including inflections such as `executed` and `persistence`
- **Checked by:** gate: docs.language
- **Bad:** `Leverage the CLI to provision a project.`
- **Good:** `Use the command-line interface (CLI) to create a project.`
- **Why:** Plain words read faster and translate better.

The plain choices are `use`, `help`, `manage`, `create`, `stop`, `run`, `save`, `load` and `file`; `executable` stays allowed.

### DOC-06 — A simplified sentence keeps its exact meaning, and a necessary technical term is defined in the glossary.
- **Applies to:** every edit that shortens or rewords documentation
- **Checked by:** review
- **Bad:** `Deletes are permanent.` when deleted projects stay recoverable for 30 days
- **Good:** `Deleted projects stay recoverable for 30 days, then are removed.`
- **Why:** A shorter sentence with a different meaning is a wrong sentence.

### DOC-07 — Prose uses none of the marketing words `powerful`, `revolutionary`, `enterprise-grade`, `seamless`, `cutting-edge`, `robust`, `next-generation`, `world-class` or `intelligent`.
- **Applies to:** prose on documentation pages
- **Checked by:** gate: docs.language
- **Bad:** `A powerful, seamless way to run agents.`
- **Good:** `Run agents on your own servers.`
- **Why:** Marketing words claim value without a fact the reader can check.

### DOC-08 — A page that says "automatically" or "as needed" states the trigger, the timing and the effect.
- **Applies to:** prose on documentation pages
- **Checked by:** review
- **Bad:** `Tokens refresh automatically.`
- **Good:** `The CLI refreshes a token when it has less than five minutes left.`
- **Why:** Readers cannot plan around behavior whose trigger is hidden.

### DOC-09 — Each concept has one canonical name across all pages, and the project glossary decides it.
- **Applies to:** every documentation page
- **Checked by:** review
- **Bad:** `project` on one page and `workstream` on the next
- **Good:** `project` on every page
- **Why:** Two names for one thing send readers looking for two things.

### DOC-10 — Each non-obvious abbreviation is expanded once per page, at its first use.
- **Applies to:** prose on documentation pages
- **Checked by:** review
- **Bad:** `Connect over SSH.`
- **Good:** `Connect over Secure Shell (SSH).`
- **Why:** An unexpanded abbreviation stops every reader who does not know it.

### DOC-11 — Customer pages state the action a user sees and keep internal queue, lease, process and storage mechanics out.
- **Applies to:** pages with `audience: user`
- **Checked by:** review
- **Bad:** `The worker claims a lease from the stream and acknowledges it.`
- **Good:** `Your run starts within 5 seconds.`
- **Why:** Customers act on outcomes, and internal mechanics change without notice.

### DOC-12 — Every pronoun names one clear subject, and the noun repeats wherever the referent could be ambiguous.
- **Applies to:** prose on documentation pages
- **Checked by:** review
- **Bad:** `The CLI calls the daemon, and it retries.`
- **Good:** `The CLI calls the daemon, and the CLI retries.`
- **Why:** An ambiguous pronoun makes the reader guess which part acts.

### DOC-13 — Prose targets a grade 8 reading level, with code and tables exempt.
- **Applies to:** prose on documentation pages
- **Checked by:** review
- **Bad:** `Subsequent to initialization, credentials are ascertained from configuration.`
- **Good:** `After setup, the CLI reads credentials from the config file.`
- **Why:** Plain prose serves readers in a hurry and readers in a second language.

### DOC-14 — Every page in a repository uses one spelling dialect.
- **Applies to:** all documentation pages in a repository
- **Checked by:** review
- **Bad:** `colour` on one page and `color` on another
- **Good:** one dialect, such as `color`, on every page
- **Why:** Mixed spelling breaks search and reads as careless.

### DOC-14b — Prose carries none of the slop patterns or slop words listed below, and stays within the em-dash budget.
- **Applies to:** prose on documentation pages
- **Checked by:** gate: docs.language
- **Bad:** `This is not a cache. It's a ledger.`
- **Good:** `It is a ledger.`
- **Why:** Machine-sounding prose hides the point and costs the reader's trust.

The gate checks the words, the phrases and the dash budget; review checks the patterns.
Copy under 300 prose words has no em dash, and a longer page has at most two.
Standards, skills, agent definitions and the always-on operating model are rule pages, exempt from the dash budget only.

## Slop patterns and banned words

DOC-14b cuts these patterns on sight.

| Pattern | Example to cut | Write instead |
|---|---|---|
| Binary contrast | "This is not X. It's Y." | State Y. |
| Throat-clearing | "Here's the thing", "Let me be clear" | The point. |
| Faux-insight | "What most people get wrong" | The claim, standing alone. |
| Colon reveal | "The detail that makes it work: a second pass." | A plain sentence. |
| Superficial `-ing` tail | "adds search, highlighting our commitment" | "adds search, so you find old drafts" |
| Importance puffery | "marks a pivotal moment" | The fact; the reader judges. |
| Weasel attribution | "experts agree", "studies show" | Name the source or cut the claim. |
| Fake-profound kicker | a closing metaphor or mic-drop | End on the last concrete sentence. |
| Summary recap | "In conclusion", "Overall" | Stop. |

The gate rejects these words, with their common inflections, in changed prose.

| Rule | Words and phrases |
|---|---|
| DOC-05 | `utilise`, `utilize`, `facilitate`, `leverage`, `orchestrate`, `instantiate`, `terminate`, `provision`, `execute`, `persist`, `hydrate`, `artifact` |
| DOC-07 | `powerful`, `revolutionary`, `enterprise-grade`, `seamless`, `cutting-edge`, `robust`, `next-generation`, `world-class`, `intelligent` |
| DOC-14b | `delve`, `foster`, `elevate`, `supercharge`, `embark`, `ever-evolving`, `tapestry`, `realm`, `streamline`, `empower`, `journey`, `landscape`, `testament`, `pivotal`, `notably`, `moreover`, `furthermore`, `myriad`, `plethora`, `aforementioned`, `meticulous`, `boasts`, `cornerstone`, `best-in-class`, `industry-leading`, `state-of-the-art`, `unparalleled`, `effortless`, `turnkey`, `battle-tested`, `rock-solid`, `innovative` |
| DOC-14b | `paradigm shift`, `game changer`, `deep dive`, `blazing fast`, `it is important to note`, `it is worth noting`, `in conclusion`, `we're excited to`, `we are excited to`, `we are pleased to` |
| DOC-14b | `contract` and `phase`; the plan hierarchy is Prototype, Milestone, Workstream, Section, Dimension and Batch, lifecycle steps are `stages`, and what `.orly/AGENTS.md` enforces is `rules` |

A real commercial agreement keeps `contract` only where no clearer term exists. `harness` is allowed, as orly's own word for what it installs. A bold list label's separating em dash is free, and a word in quotation marks or backticks is named rather than used.

## Checks and exceptions

### DOC-33 — `orly gate verify` and `orly gate pr` reject banned words, long sentences, long paragraphs, extra em dashes and heading-order errors in changed Markdown and MDX prose.
- **Applies to:** changes from the merge base, or `HEAD` without one, to the working tree, including staged edits and untracked pages
- **Checked by:** gate: docs.language
- **Bad:** `Trusting a green gate to prove front matter and section order.`
- **Good:** `Front matter, section order, command output, option defaults and table procedures checked in review.`
- **Why:** Knowing exactly what runs keeps readers from trusting a check that does not exist.

A new page and an edited paragraph are checked in full.
Unchanged history, spec records, installed `.orly/` snapshots, dependency and build directories, code and tables are excluded.
A Mintlify page title counts as the level-one heading, and a fragment needs no title.

### DOC-34 — Review judges what the gate cannot, including the editing pass, one-idea sentences, pronouns, resource lifecycle, failure behavior, implementation leaks and meaning kept through simplification.
- **Applies to:** every documentation diff
- **Checked by:** review
- **Bad:** `A page ships because the language check passed.`
- **Good:** `The reviewer confirms the page answers its reader's question with accurate facts.`
- **Why:** A clean language check proves neither that a page is useful nor that it is accurate.

Review notes, kept outside the published page, record the reader goal, the wording removed and any detail kept for safety.

### DOC-35 — A page declares at most three rule exceptions, each written `lint-ignore: <DOC-NN> — <reason>` and naming one rule.
- **Applies to:** page front matter, as one value or a list, and an HTML comment above a single paragraph
- **Checked by:** gate: docs.language
- **Bad:** `lint-ignore: ["DOC-02"]` with no reason, or four exceptions on one page
- **Good:** `lint-ignore: ["DOC-05 — quotes a vendor field name"]`
- **Why:** A stated reason keeps every exception reviewable.

In front matter, the gate drops an exception without a reason, and all of them when a page lists more than three.
An exception that recurs across pages marks a defect in the rule and is not copied forward.

## Editing pass

### DOC-43 — Every new or edited documentation block passes the distillation pass after its claims are checked against source.
- **Applies to:** new or edited blocks on documentation pages, changelog entries included
- **Checked by:** review
- **Bad:** `This release includes a major refactor of the retry module, extensively tested.`
- **Good:** `Failed deliveries now retry three times, 30 seconds apart.`
- **Why:** A new reader, without the diff, must be able to say what changed, why it matters and what to do.

The pass leads with the answer, keeps one reader goal per page and one new fact per paragraph, and states each fact once.
It cuts announcements, process narration, internal identifiers, filler, repeated conclusions and nested lists a sentence can replace.
Prerequisites, upgrade steps, defaults, limits and failure recovery stay beside the action that needs them.
