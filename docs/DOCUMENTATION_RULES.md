# Documentation rules

> Parent: [`../AGENTS.md`](../AGENTS.md) §Documentation voice. Read this before
> any narrower guide such as `CHANGELOG_VOICE.md` or before editing documentation
> about the dispatch model.

These rules govern customer-facing `agentsfleet` documentation. They keep pages
short, accurate, and usable by a first-day reader. Compliance is binary. A
rejection cites the Documentation (DOC) rule identifier that failed.

## Scope and precedence

- **DOC-S1 — Published pages.** A standalone Markdown JSX (MDX) file listed in
  `docs.json` follows every applicable rule in this document.
- **DOC-S2 — Generated API pages.** OpenAPI source follows the reference-page
  intent through summaries, descriptions, parameters, examples, responses, and
  stable errors. Generated pages do not need MDX front matter.
- **DOC-S3 — Fragments.** `snippets/*.mdx` files are reusable fragments. They
  follow language and content rules, but not page front matter or skeletons.
- **DOC-S4 — Changelog archive.** `changelog.mdx` follows
  `CHANGELOG_VOICE.md`. Historical entries are not rewritten and do not follow
  the page skeletons below.
- **DOC-S5 — Repository terms win.** Write the product as `agentsfleet`. Use the
  repository glossary and content boundaries when they are stricter than this
  document.
- **DOC-S6 — Source wins.** Runtime code is the source of truth. OpenAPI and the
  command-line interface follow runtime behavior. Published pages follow all
  three.

## Required front matter

Every published page carries these fields:

```yaml
type: tutorial | how-to | reference | explanation | troubleshooting
audience: user | operator | contributor
verified: YYYY-MM-DD
product_version: x.y.z
executable: true | false
```

- **DOC-F1 — Valid values.** Use only the values shown above.
- **DOC-F2 — Freshness.** `verified` records the last source check, not the last
  prose edit. `product_version` matches the repository's configured docs
  version.
- **DOC-F3 — Executable pages.** Set `executable: true` only when the page
  contains runnable commands that the repository checker can validate.

## Required page skeletons

Use the headings in the listed order. The page may add a heading only when the
task cannot be completed clearly without it.

| Type | Required headings, in order |
|---|---|
| tutorial | What you will build → Before you begin → Steps → Verify it works → What you learned → Related pages |
| how-to | What this does → Before you begin → Steps → Verify it works → Common problems → Remove or undo → Related pages |
| reference | Synopsis → Example with output → Options → Errors → Related pages |
| explanation | What it is → Why it exists → How it behaves → Limits → Related pages |
| troubleshooting | Symptom → Why it happened → How to fix it → How to prevent it |

- **DOC-P1 — One purpose.** Pick the page type that matches the reader's goal.
  Do not mix a tutorial and full reference on one page.
- **DOC-P2 — Honest empty sections.** If a required section does not apply,
  state that in one sentence. Do not invent behavior to fill a heading.

## Language

1. **DOC-01 — First-day reader.** Assume no product knowledge.
2. **DOC-02 — Short sentences.** Use one idea per sentence. Prose sentences have
   at most 25 words.
3. **DOC-03 — Short paragraphs.** Use at most three sentences per paragraph.
4. **DOC-04 — Active voice.** You act, or `agentsfleet` acts. Do not write “the
   system”.
5. **DOC-05 — Plain words.** Do not use “utilise”, “facilitate”, “leverage”,
   “orchestrate”, “instantiate”, “terminate”, “provision”, “execute”, “persist”,
   “hydrate”, or “artifact”. Prefer “use”, “help”, “manage”, “create”, “stop”,
   “run”, “save”, “load”, or “file”.
6. **DOC-06 — Precise meaning.** Never simplify a sentence if the shorter form
   changes its meaning. Define a necessary technical term in the glossary.
7. **DOC-07 — No marketing words.** Do not use “powerful”, “revolutionary”,
   “enterprise-grade”, “seamless”, “cutting-edge”, “robust”, “next-generation”,
   “world-class”, or “intelligent”.
8. **DOC-08 — Explain automation.** “Automatically” and “as needed” must state
   the trigger, timing, and effect.
9. **DOC-09 — One name.** Use one canonical name for each concept. The project
   glossary wins.
10. **DOC-10 — Expand abbreviations.** Expand each non-obvious abbreviation once
    per page, such as Secure Shell (SSH).
11. **DOC-11 — Customer language.** State the user-visible action. Keep internal
    queue, lease, process, and storage mechanics out of customer pages.
12. **DOC-12 — Clear referents.** Every pronoun names one clear subject. Repeat
    the noun when the reference could be ambiguous.
13. **DOC-13 — Reading level.** Target grade 8 prose. Code and tables are exempt.
14. **DOC-14 — One dialect.** Use one spelling dialect across all pages.

## Content

15. **DOC-15 — Command output.** Every runnable command is followed by its
    expected output. Mark changing output as `<varies>`.
16. **DOC-16 — Placeholders.** Use `<UPPER_SNAKE>` placeholders. State where each
    value comes from in the sentence before or after the example.
17. **DOC-17 — Fake credentials.** Example credentials are clearly fake, such as
    `af_test_00000000`.
18. **DOC-18 — Destructive actions.** Warn before the command. State what is
    lost. Show `--dry-run` first when the command supports it.
19. **DOC-19 — Complete options.** Every option states its effect, default,
    unit, and valid range. Write “none” when no default or range exists.
20. **DOC-20 — Resource lifecycle.** Every resource states who creates, owns,
    and deletes it. State the limit and the behavior at that limit.
21. **DOC-21 — Failure behavior.** Every feature states what fails, what retries,
    the retry count and delay, what data is lost, and what survives.
22. **DOC-22 — Audience boundary.** User pages state guarantees, not internals.
    Operator pages state exact dependencies and versions.
23. **DOC-23 — Units and time.** Numbers carry units. Dates use ISO 8601. Times
    use Coordinated Universal Time (UTC).
24. **DOC-24 — Stable errors.** Every product error has a stable Identifier (ID)
    and an anchor that explains why it happened, how to fix it, and how to
    prevent it.

## Structure

25. **DOC-25 — Heading order.** Use one level-one (H1) heading. Do not skip heading levels.
    Page renames require redirects.
26. **DOC-26 — Procedures.** Use numbered lists. Each item carries one action.
    Each runnable block carries one command.
27. **DOC-27 — Tables.** Use tables for comparisons. Never put procedures in a
    table.
28. **DOC-28 — Fence intent.** Use `bash` for runnable commands, `yaml` or `json`
    for configuration, and `text` for output.
29. **DOC-29 — Self-contained task.** A page completes its stated task alone.
    Links add depth and never hide a prerequisite.
30. **DOC-30 — User interface paths.** Write paths as **Settings → API Keys →
    Create Key**. Screenshots never carry unique instructions.

## Freshness

31. **DOC-31 — Required version fields.** `verified` and `product_version` are
    mandatory. A page two releases stale is rejected until rechecked.
32. **DOC-32 — Runnable examples.** Repository checks run the fenced commands on
    every `executable: true` page for each release. A failing example blocks the
    release.

## Enforcement

33. **DOC-33 — Mechanical checks.** Pre-commit checks block banned words,
    undefined abbreviations, long sentences, long paragraphs, missing front
    matter, missing or unordered sections, command blocks without output,
    options without defaults, heading violations, and procedures in tables.
34. **DOC-34 — Review checks.** Review checks one-idea sentences, pronoun
    clarity, resource lifecycle, failure behavior, implementation leaks, and
    meaning preserved by simplification.
35. **DOC-35 — Exceptions.** A page may list at most three exceptions in front
    matter. Each uses `lint-ignore: DOC-NN — reason`. Review recurring exceptions
    as a rule defect instead of copying them forward.

## Closing standard

A page is complete when a human and a Large Language Model (LLM) can answer how,
what, and why without guessing. The source check, page check, and reviewer must
all pass.
