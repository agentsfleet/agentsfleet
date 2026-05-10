# Handoff — M65 marketing rephrase + docs brevity pass

**Date:** May 10, 2026
**Captain:** Kishore
**Author:** Claude (Opus 4.7, this session)
**Status:** in-progress, two PRs open

---

## What's done

### usezombie/usezombie PR #310 (`feat/m65-001-pricing-rewrite`) — **MERGED**
Single-rate pricing, BYOK rephrase on user-facing surfaces, paired Zig+TS rate-pin tests, dashboard copy fixes, docs/architecture vocabulary preamble.

### usezombie/docs PR #47 (`chore/m65-byok-marketing-rephrase`) — **OPEN**
Companion docs PR. Branch lives in `~/Projects/docs/`. Five commits at `c2e0aac`:

1. **`9fee9df`** — drop BYOK from user-facing copy in `index.mdx` / `concepts.mdx`; tighten Early Access banner; fix `$0.001` → `$0.01` typo + `$30.10` → `$31.00` math in M65 changelog entry.
2. **`7d61649`** — extract rate strings to `snippets/rates.mdx` (`STARTER_CREDIT` / `EVENT_RATE` / `STAGE_RATE`); wire into `index.mdx`, `concepts.mdx`, `quickstart.mdx` via MDX `import { STARTER_CREDIT } from "/snippets/rates.mdx"`. Reduces the docs-side drift surface from 4 hand-typed strings to 1.
3. **`90eae0f`** — brevity pass on the four most-recent changelog entries (May 9 × 2, May 8 × 2). Fixed two more `$0.001` typos in the M65 entry header + body.
4. **`b157e9c`** — brevity pass on May 7 × 2, May 6, May 4 × 2, May 3 × 2 entries.
5. **`e2daa4c`** — brevity pass on May 1, Apr 30, Apr 29 × 3, Apr 28, Apr 27, Apr 26, Apr 24, Apr 22 × 4 entries.
6. **`c2e0aac`** — brevity pass on Apr 21 × 2, Apr 11 × 2, Apr 6 entries.

**Net delta on changelog.mdx so far:** ~600 lines reduced (from 1038 → ~440). Voice tightened, `$0.001` typos eliminated in 3 files, BYOK acronym removed from user-facing copy.

### What's left in the docs PR

The following changelog entries received a brevity pass: **May 9 × 2, May 8 × 2, May 7 × 2, May 6, May 4 × 2, May 3 × 2, May 1, Apr 30, Apr 29 × 3, Apr 28, Apr 27, Apr 26, Apr 24, Apr 22 × 4, Apr 21 × 2, Apr 11 × 2, Apr 6** (28 entries).

**Untouched (already short, low-traffic):** Apr 19 × 3, Apr 18 × 3, Apr 16 × 3, Apr 15, Apr 12 × 6, Mar 30, Mar 28, Mar 25 (20 entries). These are 1–3 paragraphs each and mostly already in Mintlify-style brevity. Decision: leave as-is unless you want a final sweep.

---

## The voice rules I followed (durable for next session)

These should land in `~/Projects/dotfiles/AGENTS.md` (or `~/Projects/usezombie/CLAUDE.md`) so the next agent doesn't have to re-derive them. Suggested location: a new `## Changelog voice` section, or extend the existing `# Tone and style` block.

### 1. Mintlify-style entry shape

Every `<Update>` follows this skeleton:

```mdx
<Update label="Mon DD, YYYY" tags={["Category", ...]}>
  ## Headline — one phrase, no fluff

  One short lead paragraph. State what changed and why someone reading the changelog cares.

  ## Sub-section (Upgrading / What's new / API / CLI / Bug fixes / Breaking)

  - **Bold lead-noun** — short consequence-first explanation.
  - Skip the "we are pleased to" / "is now" / "has been" preamble.
  - Show the diff (`old` → `new`) when describing renames, never re-state the old then re-state the new in two sentences.

  ## Code blocks for canonical request/response shapes

  Trim to the smallest example that proves the contract.
</Update>
```

### 2. Lead paragraph rules

- **Lead with the change, not the announcement.** ✅ "Pricing collapses to one number per surface." ❌ "Today we're shipping single-rate pricing across all of usezombie."
- **One paragraph max for the lead.** If the change has multiple substantive parts, list them as section headings, not as a wall of prose.
- **No vendor-flavoured marketing words.** Ban: "seamless", "magical", "powerful", "robust", "we're excited to". Use: nothing — the change speaks for itself.
- **Write like the reader has 30 seconds.** A peer agent or operator scanning at 11pm should know what changed before they finish the lead.

### 3. Bullet rules

- **Pattern:** `**Bold name** — short clause stating consequence`. The bold word(s) is what the reader greps for. The dash explains why they care. Period.
- **Drop justification prose** that isn't load-bearing. ❌ "We chose this because it's more standard REST." ✅ delete the sentence.
- **One bullet, one fact.** If a bullet has three "and"s, split it.
- **Code names always in backticks.** Function names, file paths, env vars, route paths, error codes, table names.

### 4. Internal cleanup / refactor entries

These get the **most** aggressive trimming:
- One paragraph lead naming what changed and why a reader should care (often: "no user-visible change").
- One bulleted list of the moving parts.
- Skip "## Test coverage" sections unless the test count is genuinely the headline.

Captain's exact direction: *"Keep internal code cleanup, refactor to the a minimal."*

### 5. Load-bearing facts to **never** drop

When tightening, preserve these even if it costs words:
- **Error codes** (`UZ-AUTH-003` style) and the message they return.
- **Endpoint paths** (URLs, methods, body shape, status code) for any breaking or new API surface.
- **Env var names + their default values** for any operator-facing change.
- **Schema column / table names** for any persistence change.
- **CLI subcommand names + flag names** for any CLI change.
- **Migration steps** under `## Upgrading` — if the upgrade requires an action, the action stays even if the prose is curt.
- **Money amounts** (`$5`, `$0.01`, `$0.10`) — these now flow through `snippets/rates.mdx` for non-historical entries.

### 6. Rates: source of truth

- **Zig:** `src/state/tenant_billing.zig` — `EVENT_PLATFORM_CENTS`, `EVENT_BYOK_CENTS`, `STAGE_CENTS`, `STARTER_CREDIT_CENTS`.
- **TS:** `ui/packages/website/src/lib/rates.ts` — `RATES_CENTS.{eventPlatform, eventByok, stage, starterCredit}` + `RATES_DISPLAY` mirror.
- **Docs site:** `~/Projects/docs/snippets/rates.mdx` — `STARTER_CREDIT`, `EVENT_RATE`, `STAGE_RATE`.

Paired pin tests on both Zig and TS sides (`tenant_billing_test.zig` "rates pinned" + `rates.test.ts`) fail if the values drift between Zig and TS. **The docs snippet has no automated guard** — bumping a rate in the usezombie repo means a paired one-line PR to `~/Projects/docs/snippets/rates.mdx`. Captain explicitly chose to keep this manual rather than build cross-repo CI.

Historical changelog entries quoting `$5` / `$10` / `$0.001` are **not** rewired through the snippet — they describe what shipped at the time, including obsolete values.

### 7. BYOK vocabulary split

- **User-facing surfaces** (marketing site, dashboard CTAs, docs prose, FAQ, Privacy): use plain English.
  - "Bring your own model" / "you pick the model and pay your provider directly" / "your model, your bill" / "model-agnostic".
- **Internal / technical** (Zig posture enum `Mode.byok`, `mode: "byok"` API value, `posture` DB column, `byok_credential_*` log scopes, `RATES_CENTS.eventByok`, architecture docs, historical changelog): keep `BYOK`. It's the persisted identifier; renaming is a breaking schema/API change.

The split is documented in `~/Projects/usezombie/docs/architecture/billing_and_byok.md` §0 (Vocabulary preamble).

### 8. Historical entries

**Never rewrite the past.** A changelog entry from M48 describing "Bring your own key (BYOK) + credit-pool billing" stays as the M48 archive even if today's user-facing copy says something different. The brevity pass tightens prose; it does not retcon what the system was on the day the entry was written.

If the rewrite is required (e.g., the M65 entry had a `$0.001` typo that was never true), fix the typo and note in the commit message that this is a typo correction, not a rewrite.

---

## Suggested AGENTS.md / CLAUDE.md additions

### Add to `~/Projects/dotfiles/AGENTS.md` (Captain's global rules)

Under **`# Tone and style`**, add a new sub-section:

```markdown
## Changelog voice (Mintlify-style)

When editing `~/Projects/docs/changelog.mdx` or any other `Update` block:

- **One headline per entry, no marketing words** ("seamless", "magical",
  "powerful", "we are pleased to" all banned).
- **Lead paragraph states the change, not the announcement.** A reader
  should know what changed in the first sentence.
- **Bullets follow `**Bold lead** — consequence-first clause` shape.**
  No three-clause sentences with two "and"s.
- **Internal cleanup / refactor entries get the most trimming.** One
  lead paragraph + one bullet list. Skip "Test coverage" sections
  unless the test count is the headline.
- **Never drop load-bearing facts** — error codes, endpoint paths +
  shapes, env var names + defaults, schema names, CLI flags, money
  amounts, migration steps. Tighten the prose, not the contract.
- **Historical entries are archives.** Brevity-pass them; never rewrite
  the past. A bug fix to a typo (e.g., $0.001 → $0.01) is allowed and
  must be called out in the commit message.
- **Rate constants** flow through three pinned files only:
  `src/state/tenant_billing.zig`,
  `ui/packages/website/src/lib/rates.ts`,
  `~/Projects/docs/snippets/rates.mdx`. Bumping a rate requires a paired
  PR across the docs repo — there is no automated guard.

The Mintlify reference Captain pasted (May 1 / May 8 entries) is the
canonical voice; mirror its rhythm, not its product nouns.
```

### Add to `~/Projects/usezombie/CLAUDE.md` (project-specific)

Under the **`### EXECUTE`** doc-reads table, add a row:

```markdown
| Edit on `~/Projects/docs/*.mdx`              | Read `snippets/rates.mdx` first; use `{STARTER_CREDIT}` etc. instead of hand-typing `$5`. Mintlify changelog voice rules in AGENTS.md `## Changelog voice` apply. |
```

Optional: add a one-liner cross-reference under **`## BYOK vocabulary`** (new sub-section) pointing at `docs/architecture/billing_and_byok.md` §0 for the user-facing-vs-internal split.

---

## Before / after summary

### Files changed in `~/Projects/docs/`

| File | Before | After (head: c2e0aac) | Notes |
|---|---|---|---|
| `index.mdx` | 88 lines, BYOK-flavoured pillars + verbose Early Access warning | 90 lines, "Bring your own model" pillar, terse Early Access with design-partner contacts, `STARTER_CREDIT` snippet import | +2 lines but tighter copy + 1 less hand-typed `$5` |
| `concepts.mdx` | 113 lines, "BYOK" in 4 places + `$5` in 2 places | 114 lines, BYOK only in tenant-tree example output, `STARTER_CREDIT` snippet | One snippet import row; copy clearer |
| `quickstart.mdx` | (1 `$5` mention) | (uses `STARTER_CREDIT`) | Snippet wired |
| `changelog.mdx` | 1038 lines | ~440 lines (28 entries tightened) | ~600-line reduction so far. M65 `$0.001` typo + `$30.10` math fixed in 3 places. |
| `snippets/rates.mdx` | did not exist | 17 lines | New single-source-of-truth file |
| `.gitignore` | (no `.gstack/`) | (+`.gstack/`) | Carried in from main per Captain's ask |

### Files changed in `~/Projects/usezombie/` (PR #310, already merged)

Already documented in the merged PR. Notable for this handoff:

- `docs/architecture/billing_and_byok.md` §0 added — vocabulary preamble for the BYOK split.
- `src/state/tenant_billing.zig` — constants renamed: `RECEIVE_PLATFORM_CENTS` → `EVENT_PLATFORM_CENTS`, `RECEIVE_BYOK_CENTS` → `EVENT_BYOK_CENTS`, `STAGE_OVERHEAD_CENTS` → `STAGE_CENTS`, `STARTER_GRANT_CENTS` → `STARTER_CREDIT_CENTS`.
- Paired pin tests on both Zig and TS sides for the four rate constants.

---

## How to resume

```bash
# 1. Land on the docs branch
cd ~/Projects/docs
git checkout chore/m65-byok-marketing-rephrase
git pull
git status                # should be clean

# 2. Pick up where I left off — remaining short entries
grep -nE "^<Update label" changelog.mdx | head
# Apr 19 ×3, Apr 18 ×3, Apr 16 ×3, Apr 15, Apr 12 ×6, March ×3 are untouched.
# Most are 1–3 paragraphs already; light touch only.

# 3. When the brevity pass is done, request review on PR #47
gh pr view 47 --web

# 4. After merge, the only outstanding work in usezombie is whatever
# Captain queues next (this M65 cycle is otherwise complete).
```

## Open questions for Captain (defer if you want to ship the docs PR as-is)

1. **Final brevity sweep on Apr 19 → March entries (20 entries)?** They're already short. My read: skip unless you spot something. Lower-traffic, lower-value.
2. **Add the changelog voice rules to `~/Projects/dotfiles/AGENTS.md` now?** I drafted them in §"Suggested AGENTS.md / CLAUDE.md additions" above.
3. **Cross-repo rate-drift CI guard?** Captain explicitly said no when last asked. Surfaced again in case the snippet-only approach feels too manual after a real rate change.

---

🤖 Authored by Claude Opus 4.7 (1M context). Hand off to next agent or future-Kishore.
