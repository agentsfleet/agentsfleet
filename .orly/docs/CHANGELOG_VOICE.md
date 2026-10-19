# Changelog voice

> Read [`DOCUMENTATION_RULES.md`](./DOCUMENTATION_RULES.md) first.
> Apply DOC-43 before review. Keep review notes outside the published entry.

A changelog tells readers what changed and whether they need to act.
Apply Mintlify block rules only where the repository uses that format.

- **One headline per entry, no marketing words.** Apply Documentation rule 07
  (`DOC-07`) from `DOCUMENTATION_RULES.md`. Also ban "magical", "we are pleased
  to", and "we're excited to" in changelog entries. The change speaks for itself.
- **Lead with the change.** Use one descriptive headline and one or two lead
  sentences. Skip announcements.
- **Select the reader-visible changes.** Default to at most five bullets and
  120 prose words per entry. Group related fixes; omit test counts, planning
  identifiers, and implementation inventories.

  Exceed the budget only to keep necessary upgrade or safety instructions.
  Record the reason in review notes, outside the entry.
- **Bullets follow `**Bold lead-noun** — consequence-first clause`.** One bullet, one fact. Three "and"s in a sentence → split it. Code names always in backticks (functions, paths, env vars, routes, error codes, tables).
- **Internal-only cleanup gets no entry.** Explain a refactor only when its
  effect changes behavior a reader can observe.
- **Keep facts needed to act.** Breaking changes, migration steps, defaults,
  limits, and money amounts stay explicit. Preserve exact technical tokens
  when the reader needs them.

  Link to the canonical reference for complete endpoint shapes, option tables,
  and error lists instead of repeating them.
  A link must not hide a prerequisite or required upgrade step.
- **Leave history alone.** Do not restyle old entries during an unrelated
  release. An explicitly requested brevity edit preserves dates, behavior,
  technical tokens, and required actions. Call out factual corrections in the commit.
- **Rate constants are declared once, so a money claim has one thing to match.**
  Check claims against the repository's canonical billing constants and update
  its published rate tables in the same change.
- For `agentsfleet`, the rate sources are `rustd/crates/afd_billing/src/nanos.rs`
  and the denominator in `afd_core::money`. Update
  `~/Projects/docs/snippets/rates.mdx` through its authorized branch flow;
  import its named values in Markdown JSX (MDX) instead of copying amounts.

## Example

This entry gives the behavior and action without an implementation inventory:

```md
## Retry failed deliveries

You can retry a failed webhook delivery from the dashboard.

- **Retries** — Open the failed delivery and select **Retry**.
- **Duplicates** — A delivery already running cannot be retried.
```

Before review, apply DOC-43 and verify every claim against shipped behavior.
Record the word count, bullet count, and any budget exception in review notes.
