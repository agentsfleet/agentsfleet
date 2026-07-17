# write_documentation.md — published-documentation dispatch

Read [`docs/DOCUMENTATION_RULES.md`](../docs/DOCUMENTATION_RULES.md) before
writing customer-facing documentation. This general rule runs before narrower
guides such as `CHANGELOG_VOICE.md` and before prose about the dispatch model.

## Trigger

- Editing a published Markdown JSX (MDX) page.
- Editing a reusable `snippets/*.mdx` fragment.
- Editing public OpenAPI summaries, descriptions, examples, options, or errors.
- Editing a customer-facing readme or reference page.
- Editing `docs/CHANGELOG_VOICE.md` or `docs/DISPATCH_ARCHITECTURE.md`.

`changelog.mdx` history remains excluded from the general page shape. New
changelog entries read this dispatch first, then `write_changelog.md`.

## Required order

1. Read `docs/DOCUMENTATION_RULES.md` and identify the page scope.
2. Read the repository glossary and content boundaries.
3. Verify claims against runtime code, OpenAPI, and command help.
4. Write the smallest complete page.
5. Run the repository-owned pre-commit checker.

## Enforcement

⚪ **Delegated.** Each repository owns its checker because a fresh clone must not
depend on files in a developer's home directory. Dotfiles owns the rule and the
read trigger. Repository hooks own mechanical enforcement.

Before committing, cite the applied Documentation (DOC) rule identifiers:

```text
📖 DOC READ: docs/DOCUMENTATION_RULES.md — scope <page|fragment|OpenAPI|changelog>; DOC-<rules applied>
```

## Family

- `docs/DOCUMENTATION_RULES.md` — general customer-documentation rules.
- Selected changelog and HTTP packs provide their narrower archive, voice, and
  API design rules.
