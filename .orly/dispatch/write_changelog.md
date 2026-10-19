# write_changelog.md — changelog-voice dispatch (LATENT façade)

This is the prose the AGENT reads after `write_documentation.md` and
`.orly/docs/DOCUMENTATION_RULES.md`, **before writing any `<Update>` entry in the
repository's declared changelog**. Like `verify` and
`name_architecture`, it has **no deterministic `.sh` half** — voice is a judgment
call no script can grade. It is a pure **🤔 judgment** dispatch: the agent reads
this, drafts the entry, and self-audits against the rules below. The deep
reference is `.orly/docs/CHANGELOG_VOICE.md` (Mintlify-style) — read it before writing.

**Signal legend:**

- 🤔 DECIDE — judgment-only; the agent writes to the voice below and self-audits.
  Voice remains a review judgment. `docs.language` rejects mechanical language
  findings in changed prose; DOC-33 defines its scope.
- 🟣 delegated — edit the declared changelog in its owning repository with
  that repository's branch rules and authorization.

## Trigger — read `.orly/docs/DOCUMENTATION_RULES.md`, then `.orly/docs/CHANGELOG_VOICE.md`

Fires before adding or editing an `<Update>` block in `changelog.mdx`, or any
user-visible release note. Internal-only changes get **no** changelog entry — the
Changelog Claim Challenge applies: *"Would this be true if the test file
vanished?"* Only test evidence (not a middleware/handler/CLI path) ≠ an earned
user-visible claim.

**Override:** none needed — judgment-only. A voice exception (e.g. quoting a
vendor's marketing term verbatim) is stated inline in the entry, never dressed as
a gate skip.

## The voice (summary — `.orly/docs/CHANGELOG_VOICE.md` is canonical)

- **One headline per entry.** The lead paragraph states *the change*, not the
  announcement ("X now does Y", not "We're excited to announce Y").
- **No marketing words** — use the canonical banned-word lists in
  `.orly/docs/CHANGELOG_VOICE.md` and `.orly/docs/DOCUMENTATION_RULES.md`; do not duplicate
  either list here.
- **Bullets** follow `**Bold lead-noun** — consequence-first clause`.
- **Distill** with DOC-43 and the budget in `.orly/docs/CHANGELOG_VOICE.md`.
  Select user-visible consequences instead of listing implementation changes.
- **Keep facts needed to act.** Preserve exact technical tokens when needed;
  link to the full reference. Required upgrade and safety steps stay inline.
- **Internal-only cleanup** gets no release entry.
- **History stays unchanged** unless the user explicitly requests an edit.
- **Rate claims** match the repository's canonical billing constants and
  published rate tables, with both updated when the behavior changes.
- **Rate constants** are declared once, in `afd_billing`'s `nanos.rs`, with the
  nanos denominator in `afd_core::money`. A changelog money or rate claim must
  match those — and `rates.mdx` in the docs repository, which no test reaches.

## Required output (self-audit line before committing the changelog)

```
📝 CHANGELOG: <N> words · <N> bullets · reader action clear · required facts kept · DOC-43 applied
```

If the change is internal-only, emit instead:

```
📝 CHANGELOG: skipped — internal-only (no user-visible behaviour change)
```

## Family

- `.orly/docs/CHANGELOG_VOICE.md` — the canonical Mintlify-style voice rules (deep reference).
- `.orly/docs/RELEASE_TEMPLATE.md` — release template + version-bump matrix; re-source
  each release, never paraphrase.
- `verify` — runs before CHORE(close); a changelog claim must be backed by the
  verification that dispatch reports.
