# Changelog voice (Mintlify-style)

> Parent: [`../AGENTS.orly.md`](../AGENTS.orly.md) §Owner & Style.
> Prerequisite: read [`DOCUMENTATION_RULES.md`](./DOCUMENTATION_RULES.md) first.
> Changelog history keeps its archive exception; this file adds the narrower voice.

Editing `~/Projects/docs/changelog.mdx` or any other Mintlify `<Update>` block.

- **One headline per entry, no marketing words.** Apply Documentation rule 07
  (`DOC-07`) from `DOCUMENTATION_RULES.md`. Also ban "magical", "we are pleased
  to", and "we're excited to" in changelog entries. The change speaks for itself.
- **Lead paragraph states the change, not the announcement.** A reader has 30 seconds; they should know what changed in the first sentence. ✅ "Pricing collapses to one number per surface." ❌ "Today we're shipping single-rate pricing."
- **Bullets follow `**Bold lead-noun** — consequence-first clause`.** One bullet, one fact. Three "and"s in a sentence → split it. Code names always in backticks (functions, paths, env vars, routes, error codes, tables).
- **Internal cleanup / refactor entries get the most aggressive trimming.** One lead paragraph + one bullet list. Skip "Test coverage" sections unless the test count is the headline. Indy's exact direction: *"Keep internal code cleanup, refactor to a minimal."*
- **Never drop load-bearing facts.** Error codes (`UZ-AUTH-003`), endpoint paths + method + body shape + status code, env var names + defaults, schema column / table names, CLI subcommand + flag names, migration steps, money amounts. Tighten prose, not meaning.
- **Historical entries are archives.** Brevity-pass them; never rewrite the past. A typo correction (e.g. `$0.001` → `$0.01` when it was never true) is allowed and must be called out in the commit message.
- **Rate constants stay pinned across the runtime surfaces, and a test says so.** `rustd/crates/afd_billing/tests/cross_runtime_rates.rs` reads `ui/packages/app/lib/types.ts` and `cli/src/constants/billing.ts` and fails when either drifts from `rustd/crates/afd_billing/src/nanos.rs` — a test beside the constant it defends, not a gate nothing invokes. Public docs are the one surface it cannot reach: when a rate changes, update `~/Projects/docs/snippets/rates.mdx` in a paired Pull Request (PR). In `*.mdx`, import its named values rather than hand-typing money amounts.
- **The Mintlify reference Indy pasted (May 1 / May 8 entries) is canonical voice.** Mirror its rhythm, not its product nouns.
