# write_documentation.md — published-documentation dispatch

Read `.orly/docs/DOCUMENTATION_RULES.md` before
writing customer-facing documentation. This general rule runs before narrower
guides selected by domain-specific packs (changelog voice, when selected) and
before prose about the dispatch model.

## Trigger

- Editing a published Markdown JSX (MDX) page.
- Editing a reusable `snippets/*.mdx` fragment.
- Editing public OpenAPI summaries, descriptions, examples, options, or errors.
- Editing a customer-facing readme or reference page.
- Editing `.orly/docs/CHANGELOG_VOICE.md` or `docs/DISPATCH_ARCHITECTURE.md`.

`changelog.mdx` history remains excluded from the general page shape. New
changelog entries read this dispatch first, then `write_changelog.md`.

## Required order

1. Read `.orly/docs/DOCUMENTATION_RULES.md` and identify the page scope.
2. Read the repository glossary and content boundaries.
3. Verify claims against runtime code, OpenAPI, and command help.
4. Draft the smallest complete page, then apply the DOC-43 distillation pass.
5. Run `orly gate verify` and the repository's documentation checks.

## Enforcement

`orly gate verify` and `orly gate pr` reject mechanical language findings in
changed Markdown and Markdown JSX (MDX) prose. DOC-33 names the checked rules
and exclusions. Repository checks enforce their own page metadata and examples.

Review judges whether the draft answers the reader's question, keeps required
facts, and passes DOC-43. Record that judgment outside the published page.

Before committing, cite the applied Documentation (DOC) rule identifiers:

```text
📖 DOC READ: .orly/docs/DOCUMENTATION_RULES.md — scope <page|fragment|OpenAPI|changelog>; DOC-<rules applied>
```

## Family

- `.orly/docs/DOCUMENTATION_RULES.md` — general customer-documentation rules.
- Selected changelog and HTTP packs provide their narrower archive, voice, and
  API design rules.
