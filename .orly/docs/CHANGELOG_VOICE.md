# Changelog voice (Mintlify-style)

> Parent: [`../AGENTS.md`](../AGENTS.md) §Owner & Style.
> Prerequisite: read [`DOCUMENTATION_RULES.md`](./DOCUMENTATION_RULES.md) first.
> Changelog history keeps its archive exception; this file adds the narrower voice.

Use these voice rules in the repository's declared changelog.
Apply Mintlify block rules only where that format is already selected.

- **One headline per entry, no marketing words.** Apply Documentation rule 07
  (`DOC-07`) from `DOCUMENTATION_RULES.md`. Also ban "magical", "we are pleased
  to", and "we're excited to" in changelog entries. The change speaks for itself.
- **Lead paragraph states the change, not the announcement.** A reader has 30 seconds; they should know what changed in the first sentence. ✅ "Pricing collapses to one number per surface." ❌ "Today we're shipping single-rate pricing."
- **Bullets follow `**Bold lead-noun** — consequence-first clause`.** One bullet, one fact. Three "and"s in a sentence → split it. Code names always in backticks (functions, paths, env vars, routes, error codes, tables).
- **Internal cleanup / refactor entries get the most aggressive trimming.** One lead paragraph + one bullet list. Skip "Test coverage" sections unless the test count is the headline. Indy's exact direction: *"Keep internal code cleanup, refactor to a minimal."*
- **Never drop load-bearing facts.** Error codes (`UZ-AUTH-003`), endpoint paths + method + body shape + status code, env var names + defaults, schema column / table names, CLI subcommand + flag names, migration steps, money amounts. Tighten prose, not meaning.
- **Historical entries are archives.** Brevity-pass them; never rewrite the past. A typo correction (e.g. `$0.001` → `$0.01` when it was never true) is allowed and must be called out in the commit message.
- **Rate constants are declared once, so a money claim has one thing to match.**
  Check claims against the repository's canonical billing constants and update
  its published rate tables in the same change.
- For `agentsfleet`, the rate sources are `rustd/crates/afd_billing/src/nanos.rs`
  and the denominator in `afd_core::money`. Update
  `~/Projects/docs/snippets/rates.mdx` through its authorized branch flow;
  import its named values in Markdown JSX (MDX) instead of copying amounts.
- **The Mintlify reference Indy pasted (May 1 / May 8 entries) is canonical voice.** Mirror its rhythm, not its product nouns.
