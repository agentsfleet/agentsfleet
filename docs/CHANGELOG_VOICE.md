# Changelog voice (Mintlify-style)

> Parent: [`../AGENTS.md`](../AGENTS.md) §Owner & Style.
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
- **Rate constants flow through three pinned files only:** `src/agentsfleetd/state/tenant_billing.zig`, `ui/packages/website/src/lib/rates.ts`, `~/Projects/docs/snippets/rates.mdx`. Bumping a rate requires a paired Pull Request (PR) across the docs repo — there is no automated guard. In `*.mdx`, use `import { STARTER_CREDIT, EVENT_RATE, STAGE_RATE } from "/snippets/rates.mdx"` instead of hand-typing `$5`.
- **The Mintlify reference Indy pasted (May 1 / May 8 entries) is canonical voice.** Mirror its rhythm, not its product nouns.
