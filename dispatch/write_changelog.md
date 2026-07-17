# write_changelog.md — changelog-voice dispatch (LATENT façade)

This is the prose the AGENT reads after `write_documentation.md` and
`docs/DOCUMENTATION_RULES.md`, **before writing any `<Update>` entry in the
changelog** (`~/Projects/docs/changelog.mdx`). Like `verify` and
`name_architecture`, it has **no deterministic `.sh` half** — voice is a judgment
call no script can grade. It is a pure **🔵 judgment** dispatch: the agent reads
this, drafts the entry, and self-audits against the rules below. The deep
reference is `docs/CHANGELOG_VOICE.md` (Mintlify-style) — read it before writing.

**Signal legend:**

- 🔵 DECIDE — judgment-only; the agent writes to the voice below and self-audits.
  No script gates it — it blocks the *turn* (the changelog entry), not a commit.
- ⚪ delegated — the changelog itself lives in `~/Projects/docs` (its own repo +
  own-branch flow per AGENTS.md Operational defaults); dotfiles carries only the
  voice discipline.

## Trigger — read `docs/DOCUMENTATION_RULES.md`, then `docs/CHANGELOG_VOICE.md`

Fires before adding or editing an `<Update>` block in `changelog.mdx`, or any
user-visible release note. Internal-only changes get **no** changelog entry — the
Changelog Claim Challenge applies: *"Would this be true if the test file
vanished?"* Only test evidence (not a middleware/handler/CLI path) ≠ an earned
user-visible claim.

**Override:** none needed — judgment-only. A voice exception (e.g. quoting a
vendor's marketing term verbatim) is stated inline in the entry, never dressed as
a gate skip.

## The voice (summary — `docs/CHANGELOG_VOICE.md` is canonical)

- **One headline per entry.** The lead paragraph states *the change*, not the
  announcement ("X now does Y", not "We're excited to announce Y").
- **No marketing words** — `seamless` / `magical` / `powerful` / `robust` are banned.
- **Bullets** follow `**Bold lead-noun** — consequence-first clause`.
- **Never drop load-bearing facts** — error codes, endpoints, env vars, schema
  names, money amounts stay verbatim.
- **Internal cleanup** gets aggressive trimming, not a paragraph.
- **History is archived, not rewritten** — past entries are an immutable record.
- **Rate constants** stay pinned across the three files (`tenant_billing.zig`,
  `rates.ts`, `rates.mdx`) — a changelog money/rate claim must match all three.

## Required output (self-audit line before committing the changelog)

```
📝 CHANGELOG: <N> bullet(s) · no marketing words · load-bearing facts kept · history append-only
```

If the change is internal-only, emit instead:

```
📝 CHANGELOG: skipped — internal-only (no user-visible behaviour change)
```

## Family

- `docs/CHANGELOG_VOICE.md` — the canonical Mintlify-style voice rules (deep reference).
- `skills/release-template.md` — release template + version-bump matrix; re-source
  each release, never paraphrase.
- `verify` — runs before CHORE(close); a changelog claim must be backed by the
  verification that dispatch reports.
