<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M92_004: Entity rename `zombie` → `agent` + platform-identity cutover (expand-contract)

**Prototype:** v2.0.0
**Milestone:** M92
**Workstream:** 004
**Date:** Jun 13, 2026
**Status:** PENDING
**Priority:** P1 — the last zombie surfaces are the ones users' code and data touch: wire fields, routes, schema, env vars, live hosts
**Categories:** API, CLI, INFRA, OBS, UI
**Batch:** B4 — follows M92_003 (B3) merge; one mega-spec per Indy ("one spec, not three")
**Branch:** {feat/m92-004-agent-entity-cutover — added when work begins}
**Depends on:** M92_003 (binary/package names this spec's consumers ship under; its E9 npm gate is shared by §5), M92_002 Dimension 6.1 (agentsfleet.net DNS rows — §4's host flips extend the same registrar work)
**Provenance:** agent-generated (Indy's rename sessions Jun 12–13, 2026; sources: `/private/tmp/agentsfleet_naming_handoff.md`, the M92_003 amendments handoff, M92_003 spec Discovery)

**Canonical architecture:** `docs/architecture/entity_rename_expand_contract.md` — the naming decision (product namespace vs entity), the three-stage expand-contract design, the prod-`.net`/dev-`.dev` API host split, and the binding keep list. Consult before every flow-name edit (`dispatch/name_architecture.md`, no override).

---

## Implementing agent — read these first

1. `docs/architecture/entity_rename_expand_contract.md` — the design this spec executes; the stage boundaries and contract criteria live there, not here.
2. `docs/v2/active/M92_003_P1_API_CLI_INFRA_OBS_AGENTSFLEET_BINARY_TARGET_RENAME.md` (or `done/`) — the §1 ledger + keep-pin eval pattern (E1/E3 count compares) this spec reuses, and the Discovery entries documenting what is already renamed.
3. `docs/SCHEMA_CONVENTIONS.md` + the newest migration under `schema/` — migration shape, `schema/embed.zig`, the migration array; the expand and contract migrations are NEW appended files, never edits to frozen ones.
4. `docs/REST_API_DESIGN_GUIDELINES.md` + the nearest `/zombies` handler — route registration the `/agents` aliases mirror; `docs/AUTH.md` — §4 moves Clerk JSON Web Token (JWT) `aud` claims (auth-flow read fires regardless).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m92): rename entity zombie -> agent + platform cutover`
- **Intent (one sentence):** a user inspecting any surface — API response, dashboard URL, CLI verb, env var, database table, request header, or live hostname — sees `agent`/`agentsfleet` names, with every old identifier served through an expand stage until the observable contract criteria let it drop.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm against the live world: (a) fresh blast-radius grep matches the §1 ledger, (b) M92_003 merged and its gated rows' state is known (E9 npm org, E10 installer domain), (c) each §4 external resolver has an Indy console row sequenced. A `[?]` blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a workspace owner calls `GET /v1/workspaces/{ws}/agents`, gets `agent_id` fields back, opens the dashboard at a `/agents` URL, and runs `agentsfleet agent install` — the zombie vocabulary is gone from everything they touch.
2. **Preserved user behaviour** — during the expand stage every old surface keeps answering: `/zombies` routes, `zombie_id` fields (dual-emitted), `x-usezombie*` headers (accepted), old hosts (DNS aliases until cutover). Install/login/run flows never break mid-rename.
3. **Optimal-way check** — expand-contract is the direct path given frozen migrations and independently-deployed consumers; the one-shot rename is rejected (one missed consumer is an outage).
4. **Rebuild-vs-iterate** — iterate; zero behaviour change; eval pins both directions per stage.
5. **What we build** — one expand migration + view layer, dual-serving routes/fields/headers, consumer flips (modules, CLI verb, env prefix, dashboard, metrics, fixtures), four gated platform cutovers (fly apps, API hosts, Vercel projects, Postgres creds), npm deprecation pointer, mail flip, residue sweep, one contract migration + alias removal.
6. **What we do NOT build** — compatibility shims beyond the expand stage (RULE NLG); vault renames (`ZMB_*` keeps, Indy verbatim); history rewrites (`docs/v1`, `docs/v2/done`, archive, `CHANGELOG.md`, frozen migrations); marketing copy (M92_001).
7. **Fit with existing features** — completes M92: identity (002), operational names (003), entity + platform (this). Must not destabilize the install path or live fly traffic; every resolver flip is gated on its external step verifying.
8. **Surface order** — API-first (schema + dual-serve), then CLI/UI consumers, then platform identities, then contract. The CLI verb flip rides the consumer stage.
9. **Dashboard restraint** — UI only re-points names/routes; no new controls or claims.
10. **Confused-user next step** — during expand the old names answer; after contract, `/zombies` 404s with the registry code and `agentsfleet zombie` prints the renamed-verb hint via the structured-error `suggestion` field.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE NDC (no dead alias code beyond the contract stage), RULE NLR (touched files shed stale zombie comments), RULE NLG (no legacy shims outside the designed expand stage), RULE ORP (orphan sweep per dropped view/alias/symbol), RULE TST-NAM (tests milestone-free), RULE UFS (header/field names as named constants).
- **`dispatch/write_zig.md`** — daemon module renames, handler edits (ZIG GATE; cross-compile both linux targets).
- **`docs/SCHEMA_CONVENTIONS.md`** + **`dispatch/write_sql.md`** — both NEW migrations; Schema Table Removal Guard fires at the contract migration.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — `/agents` route registration + alias handlers.
- **`dispatch/write_ts_adhere_bun.md`** — CLI verb + dashboard + fixture flips.
- **`docs/AUTH.md`** — JWT audience changes in §4.
- **`docs/LOGGING_STANDARD.md`** — log-scope renames (`.zombie_*` scopes → `.agent_*`).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — daemon modules, handlers, metrics | read façade; cross-compile both linux targets |
| SCHEMA GUARD | yes — expand + contract migrations touch `schema/embed.zig` + array | new appended files only; removal guard consult at contract |
| PUB / Struct-Shape | yes — renamed pub symbols in moved modules | shape verdicts per moved file; no surface growth |
| File & Function Length | yes — moves can concatenate | keep file splits; no file crosses 350 |
| UFS | yes — `agent_id`/header literals recur | named constants at module scope; cross-runtime ids shared verbatim |
| UI Substitution / DESIGN TOKEN | no — renames only, no new markup | — |
| LOGGING | yes — scope renames | per `docs/LOGGING_STANDARD.md`; grafana queries flip in the same Section |
| ERROR REGISTRY | yes — `ZOMBIE_NOT_FOUND`-class codes rename | registry update + negative tests per row |
| CI/CD edit guard | yes — workflow host/env strings | enumerate per workflow in PR body; strings-only; Indy grant required per session |

---

## Overview

**Goal (testable):** with the expand stage live, `GET /v1/workspaces/{ws}/agents` and `GET …/zombies` return identical bodies carrying both `agent_id` and `zombie_id`; after every consumer flips and the contract criteria hold (zero old-name API hits over a full deploy cycle), the contract stage drops views/aliases/old fields and a repo-wide entity grep matches only the frozen-history keep ledger.

**Problem:** users' code and data still speak zombie — wire fields, routes, tables, env vars, headers, hosts, creds — after M92_002/003 renamed what they read and run.
**Solution summary:** execute `docs/architecture/entity_rename_expand_contract.md`: one additive migration + dual-serve layer; flip consumers independently; cut four platform identities behind their own external gates; contract when the criteria are observable in logs.

---

## Prior-Art / Reference Implementations

- **Rename pattern** → M92_003: ledger → flip → eval-pin both directions; keep-pin count compares reused verbatim.
- **Schema / API** → nearest migration + `docs/SCHEMA_CONVENTIONS.md` (views-over-new-tables is greenfield — shape in the architecture doc); existing `/zombies` handlers re-register under `/agents` per the REST guide.
- **CLI** → 7 Pillars unchanged; the verb rename keeps command → handler → errors structure. **Gated-surface pattern** → M92_002 Dimension 6.1 / M92_003 §4: a parked external gate parks only its surface.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/0NN_agents_expand.sql` (next free) + `schema/embed.zig` + migration array | CREATE/EDIT | expand: `core.agents` + 5 satellites, data move, compat views |
| `schema/0NN_agents_contract.sql` (later, gated) | CREATE | contract: drop views; Removal Guard consult |
| `src/agentsfleetd/zombie/` → `src/agentsfleetd/agent/` (~72 entity-named files repo-wide) | RENAME/EDIT | module + symbol renames; zon fingerprint refresh |
| `src/http/handlers/**` + `public/openapi/**` | EDIT | `/agents` canonical + `/zombies` aliases; dual fields; OpenAPI servers + paths |
| `agentsfleet/src/**` + `agentsfleet/test/**` | EDIT | CLI verb `zombie`→`agent`, `ZOMBIE_*`→`AGENTSFLEET_*`, fixtures incl. `zombie-*` named test files |
| `ui/packages/app/**` (incl. `app/(dashboard)/zombies/`, backend routes) | EDIT/RENAME | dashboard routes/components/types to `agents` |
| `src/agentsfleetd/observability/metrics_runner.zig` + `deploy/grafana/*.json` | EDIT | `zombie_runner_*`→`agentsfleet_runner_*` metrics + queries together; drop `zombie-postgres` datasource + old `zombie-runner-fleet` uid |
| `deploy/fly/**`, cloudflared config, `.github/workflows/**` (hosts/env strings) | EDIT | fly app cutover refs + API host split (gated) |
| `ui/packages/website/src/config.ts` + pins | EDIT (gated) | `INSTALL_SKILL_SLASH` flips with the skills-repo cadence |
| docs (`docs/architecture/*.md`, `docs/development.md`, playbooks prose) | EDIT | entity prose + residue sweep (archive untouched) |
| `samples/platform-ops/` (delete), `samples/fixtures/`→test dirs, `zombiectl/scripts/postinstall.mjs`, `error_entries.zig` pointer | DELETE/EDIT | decommission in-repo samples — skill migrated to `agentsfleet/skills`; repoint the 5 consumers |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, sections sliced by stage and resolver — the expand/flip/contract ordering is the safety mechanism; splitting specs would scatter the gate state.
- **Alternatives considered:** three specs (entity / platform / residue) — rejected by Indy ("one mega-spec, not three"); one-shot rename — rejected (synchronized-deploy outage risk).
- **Patch-vs-refactor verdict:** **refactor** (schema + module shape moves), executed as staged renames with eval pins; no behaviour redesign.

---

## Sections (implementation slices)

### §1 — Blast-radius ledger + keep-pins (blocks every flip)

Fresh repo-root grep per entity token (`zombie_id`, `zombie_slug`, `/zombies`, `core.zombie`, `ZOMBIE_`, `x-usezombie`, file names `*zombie*`); every hit dispositioned flip/keep/stage. Separate REAL env vars from test-fixture constants and error codes. Keep-pin baseline captured (frozen paths + `ZMB_*` vaults).

- **Dimension 1.1** — ledger complete, classes dispositioned → Eval `E1` definition matches ledger
- **Dimension 1.2** — keep-pin baseline recorded → Eval `E3`

### §2 — Expand: schema + dual-serve API

One additive migration (`core.agents` + satellites, data move, updatable compat views); `/agents` canonical routes + `/zombies` aliases on the same handlers; responses dual-emit `agent_id`+`zombie_id` (and `agent_slug`+`zombie_slug`); requests accept either; emit `x-agentsfleet*`, accept both header families; error registry rows renamed with old codes mapped.

- **Dimension 2.1** — expand migration applies on a fresh and an existing database; old queries answer through views → Test `test_expand_migration_dual_read`
- **Dimension 2.2** — `/agents` and `/zombies` return identical bodies with both id fields → Test `test_routes_dual_serve_identical`
- **Dimension 2.3** — both header families accepted; new family emitted → Test `test_header_dual_accept`
- **Dimension 2.4** — OpenAPI document lists `/agents` canonical, marks `/zombies` deprecated → Test `test_openapi_agents_canonical`

### §3 — Consumer flips (modules, CLI, UI, env, metrics)

`src/agentsfleetd/zombie/`→`agent/` + symbol/log-scope renames; CLI verb `agent` (old verb answers with the structured-error rename hint); dashboard routes/components; `ZOMBIE_*`→`AGENTSFLEET_*` hard cutover; `zombie_runner_*`→`agentsfleet_runner_*` metrics with grafana queries in the same commit; fixtures + the 72 entity-named files.

- **Dimension 3.1** — daemon modules renamed; cross-compile + full suite green → Eval `E2`
- **Dimension 3.2** — `agentsfleet agent install` works; `agentsfleet zombie install` errors with the suggestion hint → Test `test_cli_verb_rename_hint` (e2e, subprocess)
- **Dimension 3.3** — env prefix flipped; no `ZOMBIE_*` read remains in source → Eval `E4` + negative grep test
- **Dimension 3.4** — metrics + dashboards flip together; runner fleet dashboard renders on new metric names → Test `test_metrics_renamed` + grafana provisioning check
- **Dimension 3.5** — dashboard `/agents` routes serve; old `/zombies` dashboard path redirects → Test `test_ui_agents_routes` (e2e)

### §4 — Platform identities (four independent external gates; each parks only its surface)

Fly apps `zombied-{dev,prod}` → new app names (create, deploy, traffic verify, retire); API hosts split prod `api.agentsfleet.net` / dev `api-dev.agentsfleet.dev` (DNS + Clerk JWT `aud` + `NEXT_PUBLIC_API_URL` + fixtures + cloudflared + workflow URLs + OpenAPI servers in one gated edit per host); Vercel projects `usezombie-{app,website}` renamed (then the kept `usezombie-app.vercel.app` URLs flip); Postgres creds `usezombie`/`usezombiedb`/`usezombie-admin` rotated to agentsfleet names via the vault (values only; `ZMB_*` vault names keep).

- **Dimension 4.1** — fly cutover: new apps serve, old apps drained → Eval `E6` (Indy row: fly app create + DNS)
- **Dimension 4.2** — API host split live; JWT `aud` validated on new hosts; old hosts alias until contract → Eval `E7` (Indy row: DNS + Clerk config)
- **Dimension 4.3** — Vercel projects renamed; workflow/fixture URLs flip in the same edit → Eval `E8` (Indy row: Vercel console)
- **Dimension 4.4** — db creds rotated; `make test-integration` green against rotated creds → Eval `E9` (Indy row: cred rotation playbook)

### §5 — npm deprecation + mail + skills cadence

`npm deprecate @usezombie/zombiectl` with pointer to `@agentsfleet/cli` (gated on M92_003 E9 first publish being stable); mail `hello@` + `team@usezombie.com` → `@agentsfleet.net` (Indy row: mailbox/alias); `INSTALL_SKILL_SLASH` → `/agentsfleet-install-platform-ops` in the same cadence as `agentsfleet/skills#4` (which merges LAST, after the CLI ships and the API cutover serves).

- **Dimension 5.1** — old npm listing shows the deprecation pointer → Eval `E10`
- **Dimension 5.2** — mail flip verified send+receive; repo refs updated → Indy row + grep
- **Dimension 5.3** — slash-command constant + pins flip with skills#4; hero terminal teaches a live command → Test `test_install_skill_slash_pin`

### §6 — Contract (gated on observable criteria)

Contract migration drops views; `/zombies` aliases removed; `zombie_id`/`zombie_slug` emission stops; `x-usezombie*` rejected; old-host aliases retired. Criteria (from the architecture doc): zero old-name API hits over a full deploy cycle + consumer versions verified.

- **Dimension 6.1** — criteria evidence captured (log queries pasted) → Discovery entry
- **Dimension 6.2** — contract migration + alias removal; negative tests: `/zombies` 404s with registry code, `zombie_id` absent from responses → Test `test_contract_old_names_gone`

### §7 — Residue sweep, hygiene + `samples/` decommission

Brand residue: Dockerfile labels, systemd `Description=`, compose headers, `github.com/usezombie` URL refs (redirects serve them — flip is janitorial), `docs.usezombie.com` doc URLs, playbook prose flagged in M92_003's PR (reconcile with Indy's one-place naming system — get the pointer). UFS rows for surviving literals. Orphan sweep per RULE ORP. **`samples/` decommission:** the agent skill `samples/platform-ops/` moved to `agentsfleet/skills` (`agentsfleet-install-platform-ops`) — delete it here and repoint its 5 consumers (the `postinstall.mjs` copier, the `error_entries.zig` example pointer, the `test-unit-bundle` lane, and the frontmatter/substitution/seed fixture readers). `samples/fixtures/` is parser **test data**, not a skill — relocate it into the test dirs, never delete. Gated after skills#4 is the live distribution channel.

- **Dimension 7.1** — residue grep matches only frozen-history keeps → Eval `E1` final
- **Dimension 7.2** — orphan sweep + dead-code table complete → Eval `E5`
- **Dimension 7.3** — `samples/platform-ops/` removed + consumers repointed; `samples/fixtures/` relocated test-local; `test-unit-bundle` + frontmatter suites green → Test `test_samples_decommissioned`

---

## Interfaces

Locked surface — changes require amending this spec: `agent_id`/`agent_slug` field names; `/agents` + `/v1/workspaces/{ws}/agents/{id}` routes; `core.agents` + `core.agent_*` table names; `AGENTSFLEET_*` env prefix; `x-agentsfleet*` headers; `agentsfleet_runner_*` metric names; hosts `api.agentsfleet.net` / `api-dev.agentsfleet.dev`. During expand both name families serve; after contract only these remain. `ZMB_*` vault names are out of scope of every sweep (Indy keep).

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Old client breaks during expand | alias/view/dual-field gap | dual-serve tests (2.1–2.3) are merge-blocking; alias parity asserted per route |
| Premature contract | criteria not actually met | §6 blocked on pasted log evidence; Removal Guard consult; Indy sign-off row |
| Synchronized-deploy trap | a §4 flip bundled with a code flip | each resolver flip is its own gated Dimension; parked gate parks only its surface |
| JWT rejections post host flip | `aud` claim mismatch | 4.2 gated edit changes Clerk + backend validation together; e2e login test on new host before DNS cutover completes |
| Old env var silently ignored | hard cutover surprise | 3.3 negative grep + CLI errors loudly on legacy `ZOMBIE_*` presence (one-release diagnostic, removed at contract) |
| Migration breaks fresh bootstrap | expand assumes existing data | 2.1 applies on both fresh and seeded databases in integration tests |

---

## Invariants

1. During expand, `/zombies` and `/agents` bodies are byte-identical — enforced by `test_routes_dual_serve_identical` in Continuous Integration (CI).
2. Frozen migrations stay byte-stable — Eval `E3` count compare; expand/contract are appended files only.
3. `ZMB_*` vault names appear in zero diffs — Eval `E3` keep token.
4. No `AGENTSFLEET_*`/`agent_id` surface ships without its test — `/write-unit-test` audit + Test Specification rows.
5. fly/host/Vercel/cred identifiers change only inside their gated Dimension — Eval `E6`–`E9` byte-stability checks until each gate passes.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1–1.2 | eval | `E1`, `E3` baseline | ledger matches grep reality; keep counts recorded |
| 2.1 | integration | `test_expand_migration_dual_read` | fresh + seeded db: old-name queries answer via views; data identical |
| 2.2 | integration | `test_routes_dual_serve_identical` | same workspace: `/agents` body == `/zombies` body; both id fields present |
| 2.3 | integration | `test_header_dual_accept` | request with either header family → 200; response carries `x-agentsfleet*` |
| 2.4 | unit | `test_openapi_agents_canonical` | OpenAPI doc: `/agents` present, `/zombies` deprecated:true |
| 3.1 | integration | full suite + cross-compile | both linux targets exit 0; counts vs baseline |
| 3.2 | e2e | `test_cli_verb_rename_hint` | subprocess: `agent install` works; `zombie install` → structured error with suggestion |
| 3.3 | unit | `test_env_prefix_flipped` | config loader reads `AGENTSFLEET_*`; legacy var presence → loud diagnostic |
| 3.4 | integration | `test_metrics_renamed` | `/metrics` exposes `agentsfleet_runner_*`; no `zombie_runner_*` |
| 3.5 | e2e | `test_ui_agents_routes` | dashboard `/agents` renders list; `/zombies` redirects |
| 4.1–4.4 | e2e (manual-verified) | Evals `E6`–`E9` | each external resolver answers on new identity; evidence in PR body |
| 5.1 | e2e (manual-verified) | Eval `E10` | `npm view` shows deprecation message on old listing |
| 5.3 | unit | `test_install_skill_slash_pin` | constant + pin flip together; gated until skills#4 |
| 6.2 | integration | `test_contract_old_names_gone` | `/zombies` → 404 registry code; responses carry no `zombie_id` |
| 7.1–7.2 | eval | `E1` final + `E5` | residue and orphans zero outside frozen keeps |
| 7.3 | integration | `test_samples_decommissioned` | postinstall + frontmatter/substitution suites green with `samples/platform-ops/` gone, fixtures test-local; no `samples/platform-ops` ref remains |

**Regression:** `make test`, `make test-integration`, app/website suites, installer `install_test.sh` — green at every stage boundary. **Idempotency/replay:** expand migration re-runnable check per migration-array conventions.

---

## Acceptance Criteria

- [ ] Expand stage live: dual-serve proven — verify: `make test-integration` (2.1–2.3 rows green)
- [ ] Consumer flips complete — verify: Evals `E2`, `E4`; CLI e2e suite green
- [ ] Each §4 gate either verified (evidence in PR body) or parked-with-surface — verify: Evals `E6`–`E9`
- [ ] Contract executed only with criteria evidence in Discovery — verify: `test_contract_old_names_gone`
- [ ] Entity grep matches only frozen keeps (Eval `E1` empty); `make lint && make test && make test-integration` green; cross-compile both linux targets; `gitleaks detect` clean; no non-md file over 350 lines

---

## Eval Commands (post-implementation)

```bash
# E1: Entity flip completeness (expect empty outside frozen keeps)
git grep -rnwE "zombie_id|zombie_slug|core\.zombie|ZOMBIE_[A-Z_]+|x-usezombie" -- ':!docs/v1' ':!docs/v2/done' ':!docs/architecture/archive' ':!CHANGELOG.md' ':!schema' | head
# E2: Build + suite — zig build && make test 2>&1 | tail -3
# E3: Keep-pins — HEAD-vs-tree count compare per keep token (ZMB_, frozen paths) — M92_003 E3 loop shape
# E4: Env prefix — git grep -rn "ZOMBIE_" -- src/ agentsfleet/src ui/packages/*/src | head  (expect empty)
# E5: Orphan sweep — grep -rn "core\.zombies\|/zombies" src/ | head  (expect empty post-contract)
# E6/E8: fly + Vercel — flyctl status --app <new-app> ; curl -fsSI https://<renamed-project>.vercel.app (Indy rows; paste)
# E7: hosts — curl -fsSI https://api.agentsfleet.net/healthz && curl -fsSI https://api-dev.agentsfleet.dev/healthz
# E9: creds — make test-integration against rotated creds (paste tail)
# E10: npm — npm view @usezombie/zombiectl deprecated
```

---

## Dead Code Sweep

| File to delete | Verify |
|----------------|--------|
| compat views (contract migration) | `psql: \dv core.*` shows none |
| `/zombies` aliases + `ui/.../zombies/` route dirs | Eval `E5`; `test ! -d ui/packages/app/app/(dashboard)/zombies` |
| `samples/platform-ops/` (migrated to `agentsfleet/skills`) | `test ! -d samples/platform-ops` + `test-unit-bundle` green |

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `zombie_id` emission | Eval `E1` | 0 outside frozen keeps |
| `zombie_runner_*` metrics + old grafana uid/datasource | `grep -rn zombie_runner_ src/ deploy/grafana/` | 0 |

---

## Discovery (consult log)

- *(empty at creation — consults, skill-chain outcomes, and Indy-acked deferral quotes append here)*

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification | Clean; outcome in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, the architecture doc, REST guide, Failure Modes, Invariants | Clean or dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR | Addressed before human review/merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Dual-serve | `make test-integration` | | |
| Suite + cross-compile | `make test` + both linux targets | | |
| Entity grep | Eval `E1` | | |
| Keep-pins | Eval `E3` | | |
| External gates | Evals `E6`–`E10` | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- 1Password vault renames — `ZMB_*` keeps (Indy verbatim, Jun 13, 2026: "i dont wanna rename the vault now").
- History rewrites: `docs/v1`, `docs/v2/done`, `docs/architecture/archive`, `CHANGELOG.md`, frozen `schema/*.sql` — append-only.
- GitHub org/repo rename — already done upstream (Jun 12, 2026, redirects); ghcr namespace — already flipped in M92_003.
- Marketing copy (M92_001); dotfiles operating-model prose (companion dotfiles commit at cutover).
