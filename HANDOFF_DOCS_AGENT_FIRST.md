# HANDOFF — agent-first re-architecture of docs/architecture (on PR #569)

> Ephemeral. Briefs the next session; **delete this file before the PR merges**
> (CHORE-close rule: `HANDOFF*.md` never ships).

## Scope / status

Indy's directive (verbatim intent): rebuild `docs/architecture/` **agent-first,
human-secondary**, in THIS PR — no new spec. Setup/run stays parked in the repo
root `README.md`; the architecture set carries none of it today and must stay
that way.

Done on this branch already (do not redo):

- ✅ M143_003 observability workstream, closed. Spec in `docs/v2/done/`.
- ✅ `observability.md` fully rewritten: 53-family metric census with
  golden-signal categories, label registry verified member-for-member against
  the Zig enums (3/10/9/5/4), two decision artifacts attached inline.
- ✅ `README.md`: decision-records index (8 Claude-artifact links) + routing
  rule (one file, grep inside).
- ✅ Directory-wide sentence surgery: **0 sentences >45 words** remain
  (was 82). Detector below; keep it at 0.
- 🔄 The structural re-architecture (this handoff's plan): **not started**.

## Working tree / branch / PR

- Branch `feat/m143-library-performance-evidence`, clean, synced with origin
  at `2fac6d293`.
- PR **#569** (GitHub, `gh`), ready-for-review, 21 commits.
- CI at last check: 36 pass / 1 pending / 2 skipping. Greptile: one review;
  its P1 ("capture target never captures") answered by removal in `d99a48a3d`.
- Worktree: `/Users/kishore/Projects/agentsfleet-m143-performance-evidence`.
  Stay inside it.

## The plan (execute in order; one commit per step or per file group)

### Step 0 — build the frozen-headings inventory (safety net, ~one command)

Specs cite these docs **by §-heading text in prose**, 141 times — not by URL
anchor. A cited heading's TEXT is frozen; renaming it silently breaks the spec
corpus. Build the list first and keep it beside you:

```bash
git grep -ohE '[a-z_]+\.md §[^)`.,;]+' -- docs/v2 ':!docs/architecture' | sort -u
```

Rules derived from it: never rename a cited heading; new sections are
ADDITIVE (insert, don't rename); uncited headings may be renamed freely.
`bash scripts/check_architecture_doc.sh` must stay green after every file.

### Step 1 — per-file skeleton (the core work)

Restructure each file to front-load facts, additively:

```
# topic — one-line claim
## Facts      ← table: invariant | number+unit | mechanism | code anchor
## Traps      ← the "do not X because Y" list — KEEP VERBATIM, only relocate
## Topology   ← existing ASCII/mermaid, byte-identical
## Decisions  ← table: decision | reason | artifact link
## Detail     ← remaining prose under EXISTING (cited) headings
```

The Facts table is extraction, not authorship: pull every load-bearing number
and invariant already in the prose up into the table, and leave a pointer
where it lived. No fact is deleted, ever — moved or pointed.

Order (by citation count = value): `runner_fleet.md` (82 citations),
`data_flow.md` (75), `billing_and_provider_keys.md` (73), `user_flow.md` (48),
`scaling.md` (22), `observability.md` (18 — already close; needs only the
Facts front-block), then `concurrency.md`, `capabilities.md`,
`fleet_bundles.md`, `connectors.md`, `memory.md`, `web_app.md`,
`high_level.md`, `direction.md`, `roadmap.md`, `testing.md`,
`product_analytics.md`. `scenarios/` files: touch only after checking the
inventory — they are contributor-canonical and cited in acceptance criteria.

### Step 2 — dedup ledger (one fact, one home)

Known duplicates, canonical home decided; siblings become one-line pointers:

| Fact | Canonical home | Currently also in |
|---|---|---|
| free-trial gating mechanics | `billing_and_provider_keys.md` §2.3 | `runner_fleet.md` §Money gates |
| config reload (pull-per-lease) | `runner_fleet.md` §Config | `data_flow.md` §Config reload |
| readiness index (`fleet:ready`) | `runner_fleet.md` §Redis topology | `data_flow.md`, `scaling.md` |
| Redis surface table | `data_flow.md` §Two streams | `runner_fleet.md` §Redis topology (keep only the before/after delta there) |
| Postgres pool guarantees | `data_flow.md` §The Postgres pool | `observability.md` census row (pointer is fine) |

Find more with: pick distinctive tokens (`LEASE_TTL_MS`, `fencing_token`,
`XREADGROUP`, `FREE_TRIAL_END_MS`, `fleet:ready`) and check which files
*explain* vs *point*. Two explanations = one becomes a pointer.
Memory continuity is already correctly aspect-split (transport in
`runner_fleet`, scope in `memory.md`, tools in `capabilities.md`) — add cross
pointers, do not merge.

### Step 3 — question→anchor index in README (~40 rows)

`| question | file §heading |`. Source the questions from real usage: the
"Implementing agent — read these first" blocks across `docs/v2/*/*.md`, plus
the obvious operator questions per file. This is the biggest tokens-to-answer
win; it turns two greps into one jump.

### Step 4 — measure (the acceptance criterion)

Ten real questions (take them from spec read-first blocks). For each, record
the sections an agent must load to answer, before vs after, in characters ÷ 4
≈ tokens. Target: **median reduction ≥ 40%**. Put the table in the commit
message or PR Session Notes. No latency-style gate — just the honest table.

### Step 5 — close out

- `bash scripts/check_architecture_doc.sh` green.
- Long-sentence detector still 0:
  ```bash
  python3 - <<'PY'
  import re, pathlib
  n=0
  for p in pathlib.Path("docs/architecture").rglob("*.md"):
      fence=False
      for l in p.read_text().split("\n"):
          if l.strip().startswith("```"): fence=not fence; continue
          if fence or l.strip().startswith(("|","#",">")): continue
          n+=sum(1 for s in re.split(r'(?<=[.!?])\s+',l) if len(s.split())>45)
  print(n)
  PY
  ```
- Frozen-headings inventory re-checked (grep each cited heading still exists).
- Resume `kishore-babysit-prs` for PR #569 — greptile re-reviews on push, and
  CI had 1 pending job at handoff time.
- **Delete this HANDOFF file** in the final commit.

## Tests / checks already green (don't re-run unless you touch code)

R1/R3(withdrawn)/R4/S1/S2 all recorded in the CHORE-close commit `d15a75b83`.
This remaining work is docs-only; the only gates that fire are
`check_architecture_doc.sh`, gitleaks (pre-commit), and the spec-template gate
if you touch `docs/v2/`.

## Risks / gotchas

- **Cited heading text is load-bearing** (Step 0). The one non-obvious
  failure mode of this whole job.
- `docs/LOGGING_STANDARD.md` and `docs/REST_API_DESIGN_GUIDELINES.md` are
  dotfiles symlinks, unresolved on this machine — `make lint-all` fails at
  `check-route-registration-doc` on `main` too. Environment issue, not yours.
- Push over SSH intermittently times out; just retry.
- Greptile suppression history lives at
  `~/.gstack/projects/agentsfleet-m143-performance-evidence/greptile-history.md`.
- Em dashes: apply the no-chain spirit, not a purge — appositive dashes in
  dense reference prose are fine (DOC-14b's ≤2 rule is scoped to published
  pages).
- Fable/xhigh is the session default now; the docs work reads better with it.
