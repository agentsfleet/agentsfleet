<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M92_004: full `zombie` → `agent` rename (entity + data surface) + `agentsfleet` brand cutover

**Prototype:** v2.0.0
**Milestone:** M92
**Workstream:** 004
**Date:** Jun 15, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the `zombie`/`usezombie` surfaces are the ones users read, type, and store: entity routes/fields, the persistence schema, error text, Command-Line Interface (CLI) / dashboard labels, env vars, request headers, live hosts, the installer domain
**Categories:** API, CLI, INFRA, OBS, UI
**Batch:** B4 — follows M92_003 (B3) merge; one mega-spec per Indy ("one spec, not three")
**Branch:** feat/m92-004-agent-entity-cutover
**Test Baseline:** unit=1946 integration=189
**Depends on:** M92_003 (binary/package names this spec's consumers ship under; its E9 npm gate is shared by §5), M92_002 Dimension 6.1 (agentsfleet.net Domain Name System (DNS) rows — §4's host flips extend the same registrar work)
**Provenance:** agent-generated (Indy's rename sessions Jun 12–15, 2026; sources: `/private/tmp/agentsfleet_naming_handoff.md`, the M92_003 amendments handoff, M92_003 spec Discovery, and the Jun 15 reversal decisions captured below)

---

## Settled design (Indy, Jun 15, 2026 — final, binding; reverses the earlier "keep zombie data surface")

This spec moved through three designs. (1) A dual-serve entity rename `zombie`→`agent` (expand then
retire). (2) A narrowing on Jun 14 to a *brand + prose cutover that kept the `zombie` data surface* (no migration).
(3) **Jun 15 reversal — the binding design below:** a **full clean-break rename `zombie`→`agent`,
data surface included.** Pre-launch, so the schema is edited directly (no migration, no `ALTER`) and
there are no live external consumers to break. The line:

| Layer | Decision | Examples |
|---|---|---|
| **Entity data surface — flips to `agent`** | Full clean-break rename. `schema/*.sql` edited directly (pre-launch; no migration, no `ALTER`). | `/zombies`→`/agents`; `zombie_id`→`agent_id`; `zombie_slug`→`agent_slug`; `core.zombies`→`core.agents`; `core.zombie_*`→`core.agent_*`; `UZ-ZMB-*`→`UZ-AGT-*` error codes; `zombie_paused`/`zombie_config_changed`→`agent_*` wire values; `Zombie*` types→`Agent*` |
| **Agent-keys concept — keep the name, rename only the clash** | Option B (Indy Jun 15). The pre-existing "agent keys" concept already owns `agent`, so the entity rename collides; resolved by freeing the one colliding identifier. | key's own id `core.agent_keys.agent_id`→`agent_key_id`; foreign key (FK) `zombie_id`→`agent_id` (now references `core.agents(id)`); routes `/agent-keys/{agent_id}`→`/agent-keys/{agent_key_id}`; CLI group `agent …`→`agent-key …`; **all three raw token prefixes flip symmetric `agt_<role>`** (Indy Jun 15) — agent-key `zmb_`→`agt_a`, tenant `zmb_t_`→`agt_t`, runner `zrn_`→`agt_r` (a=agent / t=tenant / r=runner; runner is `agt_r`, not `arn_`, to avoid the Amazon Resource Name (ARN) clash). Symmetric same-length prefixes → no containment, order-independent routing; `UZ-AGENT-*` codes + `AuthMode.agent_key` kept |
| **User-facing prose — flips to `agent`** | (largely done in earlier passes; the rename now extends it into the identifiers the prose already describes) | error titles/hints; CLI `.description()`/help/output; User Interface (UI) labels/headings; OpenAPI descriptions/summaries |
| **Brand/namespace — flips to `agentsfleet`** | the product namespace (NOT `agent` — `agent` is the entity) | `usezombie.com`→`agentsfleet.net`; `usezombie.sh`→`agentsfleet.dev`; `ZOMBIE_*` env→`AGENTSFLEET_*`; `x-usezombie*`→`x-agentsfleet*`; `zombie_runner_*`→`agentsfleet_runner_*` metrics; `zombiectl`/`zombied`→`agentsfleet`; hosts; npm; mail |
| **Frozen history + vault — untouched** | Indy verbatim, Jun 15 | `docs/v2/done`, `docs/architecture/archive`, `CHANGELOG.md` (retain `zombie` as historical record); `ZMB_*` 1Password vault names |

**Verbatim acks (Indy, Jun 15, 2026):**
- "I think to keep it simple drop the zombie across even the original decision on /zombie dropped to
  /agent that is a clean way" → reverses the keep-the-data-surface design.
- "since its not relevant to keep it midway by saying oh support /zombie or zombie_id, so its best for
  you to do all the change from zombie to agent" → full rename, data surface included.
- "no fix it in this spec and complete it in this PR" → one spec, one Pull Request (PR); the
  auth-adjacent agent-keys disambiguation is **not** split into its own spec/PR (overrides the
  `dispatch/write_spec.md` "security-boundary follow-ups get their own spec" guideline — deliberate).
- "donot touch these folders … docs/v2/done … docs/architecture/archive … CHANGELOG.md".
- Keys collision → **Option B** ("Keep agent keys, rename only the clash").
- Mail → flip everything to `agentsfleet` (brand mail `hello@`/`team@usezombie.com`→`@agentsfleet.net`
  **and** the `usezombie@agentmail.to` support address).
- "I want all to be fixed in the PR. Indy accepts that the product is down after the fix." → new
  API/app hosts do not resolve until Indy stands up DNS + Clerk JSON Web Token (JWT) `aud` post-merge
  (accepted; pre-launch, no live external consumers).

**Why a clean break is coherent now:** pre-launch, nothing stores `zombie_id` and no client reads
`/zombies`, so the migration/broken-wire risk that motivated the Jun 14 keep is gone. Renaming the
data surface too makes every layer — schema, wire, code, prose, brand — speak one vocabulary, instead
of leaving `zombie_id` permanently mismatched against the "Agent" words around it.

**The agent-keys collision (the one sharp edge):** `core.agent_keys` already holds **both** an
`agent_id` column (the key's own id, `= uid::text`) **and** a `zombie_id` FK to the entity it is bound
to. A naïve global `zombie_id`→`agent_id` would leave that table with two `agent_id` columns. Option B
resolves it: first rename the key's own id `agent_id`→`agent_key_id` (keys scope only), **then** run
the entity rename, which turns the FK `zombie_id` into `agent_id` cleanly. Sequencing is load-bearing
— see §0 and Failure Modes.

---

## Implementing agent — read these first

1. The **Settled design** table above — the binding keep/flip line; it governs every edit.
2. §0 **sequencing** below — keys-disambiguation runs *before* the entity rename, or the two
   `agent_id`s collide.
3. `docs/REST_API_DESIGN_GUIDELINES.md` + the `/zombies` handlers (→ `/agents`). `docs/AUTH.md` — the
   agent-keys edits touch token-minting/authorization surfaces (auth-flow read fires) and §4 moves
   Clerk JWT `aud` claims.
4. `dispatch/write_zig.md` — daemon edits (ZIG/PUB/LIFECYCLE gates; cross-compile both linux targets);
   `dispatch/write_ts_adhere_bun.md` — CLI/dashboard edits; `dispatch/write_sql.md` — direct
   `schema/*.sql` edits (SCHEMA GUARD fires now that the data surface moves).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m92): full zombie→agent rename (entity + data surface) + agentsfleet brand cutover`
- **Intent (one sentence):** every surface — schema table, wire field, route, error code, error
  message, dashboard label, CLI description, env var, request header, live hostname, install command —
  speaks `agent`/`agentsfleet`; the pre-existing "agent keys" concept keeps its name but yields the one
  identifier (`agent_id` → `agent_key_id`) that the entity now claims.
- **Handshake (filled at PLAN):** intent restated above. `ASSUMPTIONS I'M MAKING:` (1) pre-launch — no
  stored `zombie_id` and no live client reads `/zombies`, so direct schema edits are safe; (2) the only
  `agent`-name collision is `core.agent_keys` (verified: `agent_id` appears in 22 files, all
  agent-keys/route/openapi/CLI scope); (3) frozen-history dirs + `ZMB_*` vault stay byte-stable;
  (4) `usezombie`/`zombiectl`/`zombied` flip to `agentsfleet` (brand/binary), never to `agent`. A `[?]`
  blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a workspace owner runs `agentsfleet agent list`, sees their **agents**,
   reads "Agent not found" on a bad id, calls `GET /agents` and reads `agent_id` back, mints an
   **agent key** with `agentsfleet agent-key add`, and installs from `agentsfleet.dev` — one vocabulary
   end to end.
2. **Preserved user behaviour** — install/login/run/steer/logs flows are behaviourally identical; only
   the names change. (Wire shape changes — pre-launch, accepted.)
3. **Optimal-way check** — a clean-break rename (no migration, no dual-serve, no compatibility shim) is
   the direct path pre-launch; it removes the permanent `zombie_id`-vs-"Agent" mismatch the Jun 14
   design would have frozen in.
4. **Rebuild-vs-iterate** — iterate; mechanical rename + direct schema edit, no behaviour change.
5. **What we build** — the full `zombie`→`agent` entity rename (routes, wire fields, schema, error
   codes, types), the agent-keys disambiguation (Option B), user-facing prose → `agent`, the
   `agentsfleet` brand/namespace cutover (env, headers, metrics, hosts, npm, installer domain, mail),
   four gated platform cutovers, `samples/` decommission, and two pre-approved Zig refactors.
6. **What we do NOT build** — a data migration or backfill (pre-launch clean break); compatibility
   shims / dual-read; vault renames (`ZMB_*` keeps, Indy verbatim); history rewrites (`docs/v1`,
   `docs/v2/done`, `docs/architecture/archive`, `CHANGELOG.md`); marketing copy (M92_001).
7. **Fit with existing features** — completes M92: identity (002), operational names (003), entity +
   brand + user-facing (this). The agent-keys concept stays distinct (`agent-key`), bound to an
   `agent`.
8. **Surface order** — keys-disambiguation first (collision-safe), then the entity rename, then the
   brand/platform/residue tail.
9. **Dashboard restraint** — UI re-words labels and updates the route paths it calls; no new controls.
10. **Confused-user next step** — old env vars (`ZOMBIE_*`) error loudly with the renamed-var hint;
    `usezombie.com`/`usezombie.sh` redirect to the agentsfleet domains (post-merge, Indy console).

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE NLR (touched files shed stale `zombie`/`usezombie`
  prose/comments), RULE NLG (no legacy shims / no dual-read), RULE ORP (orphan sweep per dropped
  `samples/platform-ops` symbol), RULE TST-NAM (tests milestone-free), RULE UFS (header/env/metric
  names as named constants).
- **`dispatch/write_sql.md`** — direct `schema/*.sql` edits for the 6 entity tables + `agent_keys`;
  SCHEMA GUARD fires; `schema/embed.zig` `@embedFile` consts + filenames kept in lockstep.
- **`dispatch/write_zig.md`** — daemon rename + metric rename + the two struct refactors (ZIG/PUB/
  LIFECYCLE gates; cross-compile both linux targets).
- **`dispatch/write_ts_adhere_bun.md`** — CLI + dashboard rename/prose.
- **`dispatch/write_http.md`** + **`docs/REST_API_DESIGN_GUIDELINES.md`** — `/zombies`→`/agents` route
  + OpenAPI edits; `/agent-keys` path-param rename.
- **`docs/AUTH.md`** — agent-keys disambiguation touches authorization; JWT `aud` changes in §4.
- **`docs/LOGGING_STANDARD.md`** — log scopes flip with the entity (`zombie`→`agent`).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | **yes** — direct edits to 6 entity tables + `agent_keys` | pre-v2 clean-break path (Indy grant); `schema/embed.zig` + filenames in lockstep; single-concern per file |
| ZIG GATE | yes — daemon rename, metric rename, two refactors | read façade; cross-compile both linux targets |
| PUB / Struct-Shape | yes — `@This()`/decl-literal refactor + renamed pub types | shape verdicts per touched file; no surface growth (rename only) |
| File & Function Length | yes — refactors + large renamed files | no file crosses 350 |
| UFS | yes — env/header/metric/route literals recur | named constants at module scope |
| ERROR REGISTRY | yes — `UZ-ZMB-*`→`UZ-AGT-*` codes + message prose; `UZ-AGENT-*` (keys) kept | codes + titles/hints flip; negative test per renamed row; pins updated |
| UI Substitution / DESIGN TOKEN | no — label re-wording only, no new markup | — |
| LOGGING | low — entity log scopes flip with the rename | per `docs/LOGGING_STANDARD.md` |
| CI/CD edit guard | yes — workflow host/env/installer strings | enumerate per workflow in PR body; Indy grant per session |

---

## Overview

**Goal (testable):** after this PR, `git grep -nE "[Zz]ombie|UZ-ZMB|/zombies|core\.zombie"` over the
active tree (excluding `docs/v2/done`, `docs/architecture/archive`, `CHANGELOG.md`, and the `ZMB_*`
vault refs) returns **zero**; `git grep usezombie` over source returns only flagged keeps;
`core.agent_keys` has exactly one `agent_id` (the FK to `core.agents`) plus `agent_key_id` (its own id);
the daemon builds and `make test` + cross-compile (both linux targets) are green.

**Problem:** the entity is named `zombie` at every layer; users read/type/store `zombie`, and after the
Jun 14 keep the persistence + wire identifiers would have stayed `zombie` permanently, mismatched
against the "Agent" prose around them.
**Solution summary:** rename the entity `zombie`→`agent` end to end (schema included, direct edit, no
migration); free the colliding `agent_id` in the agent-keys concept first; flip the brand to
`agentsfleet`; cut four platform identities behind their own external gates.

---

## Prior-Art / Reference Implementations

- **Rename pattern** → M92_003: blast-radius ledger → flip → eval-pin both directions.
- **Prose-vs-brand discrimination** → `zombie`→`agent` for the entity, but `usezombie`→`agentsfleet`
  and `zombiectl`/`zombied`→`agentsfleet` for brand/binary (protected from the entity sed); applied as
  ordered, scoped substitutions, not one blind global sed.
- **Pre-v2 direct-schema teardown** → M66_001 / the `core.agent_keys` file (`Pre-v2.0 teardown: full
  file replace`): full-file replace, no `ALTER`, no migration array growth.
- **Gated-surface pattern** → M92_002 Dimension 6.1 / M92_003 §4: a parked external gate parks only its
  surface.

---

## Files Changed (blast radius)

Counts are the active tree (frozen-history dirs excluded). Identifiers, not file lists, where a class
spans hundreds of files.

| Area | Action | Why |
|------|--------|-----|
| `schema/007_core_zombies.sql`+5 sibling `*zombie*.sql`, `schema/011_core_agent_keys.sql`, `schema/embed.zig` | EDIT/RENAME | `core.zombies`→`core.agents`, `core.zombie_*`→`core.agent_*` (direct, no migration); `agent_keys.agent_id`→`agent_key_id`; file renames + `@embedFile` consts in lockstep |
| `src/agentsfleetd/http/handlers/api_keys/agent.zig`, `http/route_matchers*.zig`, `route_table*.zig`, `routes.zig` | EDIT | keys disambiguation: `agent_id`→`agent_key_id`, `/agent-keys/{agent_id}`→`{agent_key_id}` |
| `src/agentsfleetd/**` (daemon: ~bulk) | EDIT | entity rename `zombie`→`agent` (handlers, fleet, errors, types, log scopes); `UZ-ZMB-*`→`UZ-AGT-*` |
| `agentsfleet/src/**` (CLI), `ui/packages/app/**` (dashboard) | EDIT | entity rename in identifiers + display; CLI group `agent`→`agent-key`; `--zombie`→`--agent` flag |
| `public/openapi/{root.yaml,paths/zombies.yaml→agents.yaml,paths/agent-keys.yaml,components/schemas.yaml}` + regen `public/openapi.json` | EDIT/RENAME | routes `/zombies`→`/agents`; `zombie_id`→`agent_id`; agent-keys path-param rename; `x_usezombie`→`x_agentsfleet` |
| `src/agentsfleetd/observability/metrics_runner.zig` (+`_test.zig`) + `deploy/grafana/runner_fleet.json` | EDIT | `zombie_runner_*`→`agentsfleet_runner_*` (lockstep) |
| install script, `README*`, website install snippet, config | EDIT | `usezombie.sh`→`agentsfleet.dev`; `usezombie.com`→`agentsfleet.net` |
| `agentsfleet/src/lib/contact.ts`, `ui/packages/{app,website}/.../contact.ts`, pins (5 mirrored) | EDIT | mail flip everything → `agentsfleet` |
| `deploy/fly/**`, cloudflared, `.github/workflows/**` (hosts/env strings) | EDIT | fly app cutover + API host split (gated; Indy CI grant) |
| `samples/platform-ops/` (delete), `samples/fixtures/`→test dirs | DELETE/EDIT | decommission in-repo samples; repoint 5 consumers |
| `src/agentsfleetd/http/routes.zig` + struct files across `src/agentsfleetd/**` | EDIT | two pre-approved Zig refactors (own commits) |

**Protected from the `zombie`→`agent` sed (flip to `agentsfleet`, or keep):** `usezombie` (brand),
`zombiectl`/`zombied` (binary), `ZMB_*` (vault, keep). **Untouched entirely:** `docs/v2/done`,
`docs/architecture/archive`, `CHANGELOG.md`.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, sequenced keys-first → entity → brand/tail; deterministic scripted
  rename with explicit protect-list + frozen-history exclusion, then adversarial verification.
- **Alternatives considered:** keep the `zombie` data surface (the Jun 14 design — rejected by Indy
  Jun 15, "do all the change"); rename the agent-keys concept to "API keys" instead of Option B
  (rejected — Indy chose "keep agent keys, rename only the clash").
- **Patch-vs-refactor verdict:** **patch** (mechanical rename + direct schema edit, no behaviour
  change), plus two isolated pre-approved Zig refactors in their own bisectable commits.

---

## Sections (implementation slices)

### §0 — Keys disambiguation (runs FIRST; blocks the entity rename)

Scoped to agent-keys files only. The key's own id `core.agent_keys.agent_id`→`agent_key_id` (+ the
constraints `ck_agent_keys_agent_id_uuidv7`→`…agent_key_id…`, `ck_agent_keys_uid_matches_agent_id`→
`…agent_key_id`); the `zombie_id` FK is left for §1 to rename. Route path-param
`/agent-keys/{agent_id}`→`{agent_key_id}` + matchers; OpenAPI `agent-keys.yaml`; CLI group
`agent`→`agent-key`; agent-key raw prefix `zmb_`→`agt_a`. `UZ-AGENT-*` codes + `AuthMode.agent_key`
kept. (The sibling token prefixes — tenant `zmb_t_`→`agt_t`, runner `zrn_`→`agt_r` — flip in §2;
all three settled together as symmetric `agt_<role>`, Indy Jun 15. See Discovery.)

- **Dimension 0.1** — `core.agent_keys` has `agent_key_id` (own id) + `zombie_id` (FK, still) → Test `test_agent_key_id_renamed`
- **Dimension 0.2** — `/agent-keys/{agent_key_id}` matches; CLI `agent-key add/list/delete` works → Test `test_agent_key_cli` + matcher test

### §1 — Entity rename `zombie` → `agent` (data surface + code)

Deterministic scripted rename over the non-frozen tree: `zombie_id`→`agent_id`, `zombie_slug`→
`agent_slug`, `/zombies`→`/agents`, `core.zombies`→`core.agents`, `core.zombie_*`→`core.agent_*`,
`Zombie*`→`Agent*`, lowercase `zombie`→`agent`, `UZ-ZMB-`→`UZ-AGT-`, `zombie_paused`/
`zombie_config_changed`→`agent_*`. Schema files edited directly + renamed; `schema/embed.zig` lockstep.
The `core.agent_keys.zombie_id` FK becomes `agent_id` here (collision-free because §0 freed it).

- **Dimension 1.1** — schema: `core.agents` + `core.agent_*` tables; `agent_keys.agent_id` FK →
  `core.agents(id)`; `embed.zig` consts/filenames aligned → Test `test_schema_agents` + SCHEMA GUARD
- **Dimension 1.2** — daemon builds; cross-compile both linux targets; full suite green → Eval `E2`
- **Dimension 1.3** — `/agents` + `agent_id` served; error codes read `UZ-AGT-*` → Test `test_agent_routes` + `test_error_codes_agt`
- **Dimension 1.4** — `git grep` for entity `zombie` tokens over active tree == 0 → Eval `E1`

### §2 — Brand cutover (`agentsfleet`)

`usezombie.com`→`agentsfleet.net`; `usezombie.sh`→`agentsfleet.dev`; `ZOMBIE_*` env→`AGENTSFLEET_*`
(hard cutover, loud diagnostic on legacy); `x-usezombie*`→`x-agentsfleet*`; `zombiectl`/`zombied`
residue→`agentsfleet`; `zombie_runner_*`→`agentsfleet_runner_*` with grafana in the same commit.
Raw token prefixes (brand abbreviations) flip with the brand, symmetric `agt_<role>`: agent key
`zmb_`→`agt_a`, tenant API key `zmb_t_`→`agt_t`, runner token `zrn_`→`agt_r` (`agt_r`, not `arn_` —
Amazon Resource Name (ARN) clash). Each prefix is single-sourced to one `pub const` (RULE UFS) —
`api_key.KEY_PREFIX` / `tenant_api_key.TENANT_KEY_PREFIX` / `protocol.RUNNER_TOKEN_PREFIX` — every
mint/validate site references the const and one pin test per prefix guards the literal, so a future
flip is one line. Pre-launch clean break: token validation is prefix-relative (`startsWith`), no
stored tokens; the three same-length prefixes differ at char 4 (`a`/`t`/`r`) → no containment,
order-independent routing in `bearer_or_api_key`. `ZMB_*` 1Password vault names stay (Indy keep).
`agentsfleetd` deploy/CI strings (`.github/workflows/deploy-dev.yml`, `deploy/baremetal/*` `agt_r`
placeholder guard) flip under the CI/CD grant in §3/§5.

- **Dimension 2.1** — `git grep usezombie` over source == flagged keeps only → Eval `E4`
- **Dimension 2.2** — env flipped; no `ZOMBIE_*` read remains → Eval `E4` + negative grep
- **Dimension 2.3** — metrics renamed; `/metrics` exposes `agentsfleet_runner_*`; grafana valid → Test `test_metrics_renamed`
- **Dimension 2.4** — installer serves at `agentsfleet.dev`; `usezombie.sh` live refs == 0 → Eval `E10`

### §3 — Platform identities (four independent external gates; each parks only its surface)

Fly apps `zombied-{dev,prod}`→new names; API hosts split prod `api.agentsfleet.net` / dev
`api-dev.agentsfleet.net` (DNS + Clerk JWT `aud` + `NEXT_PUBLIC_API_URL` + fixtures + cloudflared +
workflow URLs + OpenAPI `servers` in one gated edit per host); Vercel projects `usezombie-{app,website}`
renamed; Postgres creds rotated via the vault (values only; `ZMB_*` names keep).

- **Dimension 3.1** — fly cutover → Eval `E6` (Indy row)
- **Dimension 3.2** — API host split; JWT `aud` validated on new hosts → Eval `E7` (Indy row)
- **Dimension 3.3** — Vercel projects renamed → Eval `E8` (Indy row)
- **Dimension 3.4** — db creds rotated; `make test-integration` green → Eval `E9` (Indy row)

### §4 — npm deprecation + mail + skills cadence

`npm deprecate @usezombie/zombiectl`→`@agentsfleet/cli` (gated on M92_003 E9); mail flip everything →
`agentsfleet` (brand `hello@`/`team@usezombie.com`→`@agentsfleet.net`; support
`usezombie@agentmail.to`→`agentsfleet@agentmail.to`; 5 mirrored files + pin tests);
`INSTALL_SKILL_SLASH`→`/agentsfleet-install-platform-ops` with `agentsfleet/skills#4`.

- **Dimension 4.1** — old npm listing shows the deprecation pointer → Eval `E10`
- **Dimension 4.2** — mail flipped across 5 mirrored files + pins → grep + pin tests
- **Dimension 4.3** — slash-command constant + pins flip with skills#4 → Test `test_install_skill_slash_pin`

### §5 — Residue sweep, hygiene + `samples/` decommission + Zig refactors

Brand residue (Dockerfile labels, systemd `Description=`, compose headers, `github.com/usezombie`,
`docs.usezombie.com`). Orphan sweep per RULE ORP. **`samples/platform-ops/` decommission:** delete +
repoint its 5 consumers (`postinstall.mjs` copier, `error_entries.zig` example pointer,
`test-unit-bundle` lane, frontmatter/seed fixture readers). `samples/fixtures/` is parser test data —
relocate into test dirs, never delete. **Two pre-approved Zig refactors** (route-enum/matcher;
`@This()`/decl-literal struct passes) land here in their own bisectable commits, after the rename is
green.

- **Dimension 5.1** — residue grep matches only frozen-history keeps → Eval `E1` final
- **Dimension 5.2** — orphan sweep + dead-code table complete → Eval `E5`
- **Dimension 5.3** — `samples/platform-ops/` removed + consumers repointed; `test-unit-bundle` green → Test `test_samples_decommissioned`
- **Dimension 5.4** — two Zig refactors land; cross-compile + full suite green; no pub-surface growth → Eval `E2` + PUB-gate verdicts

---

## Interfaces

**New surfaces (locked `agent`/`agentsfleet`, byte-stable after this PR):** `/agents` +
`/v1/workspaces/{workspace_id}/agents/{agent_id}` routes; `agent_id`/`agent_slug` wire fields;
`core.agents` + `core.agent_*` tables/columns; `UZ-AGT-*` error codes; `agent_paused`/
`agent_config_changed` wire values; `AGENTSFLEET_*` env prefix; `x-agentsfleet*` headers;
`agentsfleet_runner_*` metric names; hosts `api.agentsfleet.net` / `api-dev.agentsfleet.net`; installer
`agentsfleet.dev`. **Agent-keys (kept distinct, Option B):** `core.agent_keys` with `agent_key_id`
(own id) + `agent_id` (FK to `core.agents`); routes `/agent-keys/{agent_key_id}`; CLI `agent-key …`;
`UZ-AGENT-*` codes. **Raw token prefixes (locked, symmetric `agt_<role>`, each single-sourced to one
`pub const`):** agent key `agt_a` (`api_key.KEY_PREFIX`), tenant API key `agt_t`
(`tenant_api_key.TENANT_KEY_PREFIX`), runner token `agt_r` (`protocol.RUNNER_TOKEN_PREFIX`). `ZMB_*`
vault names are out of scope of every sweep (Indy keep).

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Two `agent_id` columns in `core.agent_keys` | entity rename run before §0 frees the key's own id | §0 runs first (own id → `agent_key_id`); SCHEMA GUARD + build fail loud if violated |
| Brand corrupted to `useagent`/`agentctl` | `zombie`→`agent` sed hits `usezombie`/`zombiectl`/`zombied` | protect-list excludes them; they flip to `agentsfleet`; Eval `E4` + negative grep `useagent`/`agentctl` |
| Frozen history mutated | sed not scoped | path excludes for `docs/v2/done`, `docs/architecture/archive`, `CHANGELOG.md`; diff review |
| Old env var silently ignored | hard `ZOMBIE_*`→`AGENTSFLEET_*` cutover | §2.2 negative grep + loud diagnostic on legacy presence |
| Metric/grafana drift | metric renamed without grafana | §2.3 flips emit sites + `deploy/grafana/runner_fleet.json` in one commit |
| JWT rejections post host flip | `aud` claim mismatch | §3.2 gated edit changes Clerk + backend validation together; e2e login on new host first |
| Vault refs broken | `ZMB_*` swept by mistake | `ZMB_*` on the keep-list; Eval `E3` keep token == byte-stable |

---

## Invariants

1. **Zero `zombie` in the active tree.** `git grep -nE "[Zz]ombie|UZ-ZMB|/zombies|core\.zombie"`
   excluding `docs/v2/done`, `docs/architecture/archive`, `CHANGELOG.md`, and `ZMB_*`/`zmb`-vault
   lines == 0 (Eval `E1`). (Inverts the Jun 14 "byte-stable zombie surface" invariant.)
2. **`core.agent_keys` has exactly one `agent_id`** — the FK to `core.agents`; the key's own id is
   `agent_key_id` (Eval `E3`).
3. **No migration ships** — `schema/*.sql` is edited in place (full-file replace), no `ALTER`, no
   migration-array growth; `schema/embed.zig` consts match the on-disk filenames.
4. **Brand never degrades to `agent`** — `git grep -nE "useagent|agentctl\b|agentd\b"` == 0;
   `usezombie`/`zombiectl`/`zombied` flip to `agentsfleet` (Eval `E4`).
5. **`ZMB_*` vault names appear in zero diffs** — Eval `E3` keep token.
6. fly/host/Vercel/cred identifiers change only inside their gated Dimension — Eval `E6`–`E9`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 0.1 | unit/schema | `test_agent_key_id_renamed` | `core.agent_keys` columns = {`uid`,`agent_key_id`,`workspace_id`,`zombie_id`(pre-§1)/`agent_id`(post),…}; constraints renamed |
| 0.2 | e2e/unit | `test_agent_key_cli` + matcher | `agentsfleet agent-key add` mints a key; `/agent-keys/{agent_key_id}` matches; `/agent-keys/{x}/` rejects |
| 1.1 | schema | `test_schema_agents` + SCHEMA GUARD | `core.agents`/`core.agent_*` exist; `agent_keys.agent_id` FK → `core.agents(id)`; `embed.zig` aligned |
| 1.2 | integration | full suite + cross-compile | both linux targets exit 0; counts vs baseline |
| 1.3 | unit/e2e | `test_agent_routes` + `test_error_codes_agt` | `GET /agents` returns `agent_id`; 404 carries `UZ-AGT-009` (was `UZ-ZMB-009`) |
| 1.4 | eval | `E1` | entity `zombie` tokens over active tree == 0 |
| 2.1–2.2 | unit | `test_env_prefix_flipped` | config loader reads `AGENTSFLEET_*`; legacy `ZOMBIE_*` → loud diagnostic |
| 2.3 | integration | `test_metrics_renamed` | `/metrics` exposes `agentsfleet_runner_*`; no `zombie_runner_*`; grafana valid |
| 2.4 | eval | `E10` | `usezombie.sh` live refs == 0; install command names `agentsfleet.dev` |
| 3.1–3.4 | e2e (manual) | `E6`–`E9` | each external resolver answers on new identity; evidence in PR body |
| 4.1/4.3 | e2e/unit | `E10` / `test_install_skill_slash_pin` | npm pointer; slash-command constant + pin flip together |
| 4.2 | unit | mail pin tests | `SUPPORT_EMAIL` == `agentsfleet@agentmail.to` across 5 mirrors; no `usezombie` mail literal |
| 5.1–5.2 | eval | `E1` final + `E5` | residue and orphans zero outside frozen keeps |
| 5.3 | integration | `test_samples_decommissioned` | suites green with `samples/platform-ops/` gone, fixtures test-local |
| 5.4 | eval | `E2` + PUB verdicts | two refactors land; full suite + cross-compile green; no pub-surface growth |

**Regression:** `make test`, `make test-integration`, app/website suites, installer `install_test.sh` —
green. **Renamed-surface guard:** the `/zombies`+`zombie_id` tests are rewritten to `/agents`+`agent_id`
and pass; no test still asserts a `zombie` token outside frozen history.

---

## Acceptance Criteria

- [ ] Entity renamed end to end — verify: Eval `E1` (zero active `zombie`), `test_schema_agents`,
      `test_agent_routes`, `test_error_codes_agt`
- [ ] Agent-keys disambiguated (Option B) — verify: `test_agent_key_id_renamed`, `test_agent_key_cli`;
      Invariant 2 (one `agent_id` in `core.agent_keys`)
- [ ] No migration; schema edited directly — verify: Invariant 3; SCHEMA GUARD clean
- [ ] Brand reads `agentsfleet`, never degraded to `agent` — verify: Eval `E4`, `E10`; Invariant 4
- [ ] Metrics + grafana flipped together — verify: `test_metrics_renamed`
- [ ] Mail flipped across 5 mirrors — verify: mail pin tests
- [ ] Two Zig refactors land green — verify: Eval `E2` + PUB verdicts
- [ ] Each §3 gate either verified (evidence in PR body) or parked-with-surface — verify: `E6`–`E9`
- [ ] `make lint && make test` green; cross-compile both linux targets; `gitleaks detect` clean; no
      non-md file over 350 lines; frozen-history dirs byte-stable

---

## Eval Commands (post-implementation)

```bash
# E1: entity zombie residue (expect 0 over active tree)
git grep -nE "[Zz]ombie|UZ-ZMB|/zombies|core\.zombie" -- . \
  ':(exclude)docs/v2/done' ':(exclude)docs/architecture/archive' ':(exclude)CHANGELOG.md' \
  | grep -viE "ZMB_|zmb_vault" | grep -c . || true
# E2: build + suite — zig build && make test 2>&1 | tail -3 ; cross-compile both linux targets
# E3: keep tokens byte-stable — git grep -c "ZMB_" vs origin/main ; agent_keys has one agent_id
# E4: brand — git grep -nE "usezombie|useagent|agentctl\b|agentd\b" -- src/ agentsfleet/src ui/packages
#     (expect: usezombie→flagged keeps only; useagent/agentctl/agentd == 0)
# E5: orphan sweep — grep -rn "samples/platform-ops" src/ (expect 0)
# E10: installer — git grep -nE "usezombie\.sh" (live refs; expect 0)
# E6/E7/E8/E9: fly/host/Vercel/creds — Indy console rows; paste evidence in PR body
```

---

## Dead Code Sweep

| File to delete | Verify |
|----------------|--------|
| `samples/platform-ops/` | `test ! -d samples/platform-ops` + `test-unit-bundle` green |

| Renamed/removed symbol | Grep | Expected |
|-----------------------|------|----------|
| `core.zombies`/`core.zombie_*`/`zombie_id`/`/zombies` (active tree) | Eval `E1` | 0 |
| `zombie_runner_*` metrics + old grafana uid | `grep -rn zombie_runner_ src/ deploy/grafana/` | 0 |
| `core.agent_keys.agent_id` as the key's own id | `grep -n "agent_id" schema/011_core_agent_keys.sql` | only the FK to `core.agents` |
| `samples/platform-ops` consumers | Eval `E5` | 0 |

---

## Discovery (consult log)

- *Jun 15, 2026 (early):* JWT test-fixture blocker root-caused — RSA (Rivest–Shamir–Adleman) exponent
  `e` was clobbered to the modulus value across 4 auth files; fix is `e`→`AQAB` (verified). No fixture
  consolidation needed.
- *Jun 15, 2026 (this session):* Indy **reversed** the Jun 14 keep-the-data-surface design → full
  clean-break `zombie`→`agent` rename, schema included (verbatim acks in **Settled design**).
- *Jun 15, 2026:* agent-keys collision surfaced (`core.agent_keys` holds both `agent_id` and
  `zombie_id`); Indy chose **Option B** (keep "agent keys", rename only the clash). Resolution is
  sequencing-sensitive — §0 before §1.
- *Jun 15, 2026:* Indy: keep the auth-adjacent keys disambiguation **in this spec/PR**, not a split-out
  follow-up (deliberate override of `dispatch/write_spec.md` security-boundary guideline).
- *Jun 15, 2026:* frozen-history dirs (`docs/v2/done`, `docs/architecture/archive`, `CHANGELOG.md`)
  locked untouched (Indy verbatim).
- *Jun 15, 2026 (resume):* raw token prefixes resolved. A prior handoff had parked all three
  (`zmb_`/`zmb_t_`/`zrn_`) as "Indy hasn't ruled" with no ack-quote, contradicting §0's explicit
  agent-key flip mandate; surfaced as a single decision. **Indy verbatim:** *"zmb_ -> agt_, zmb_t to
  agt_t, zrn_ to agt_r (since arn has a clash with AWS arn)"* — flip all three: agent-key
  `zmb_`→`agt_`, tenant `zmb_t_`→`agt_t_`, runner `zrn_`→`agt_r_` (`agt_r_`, not `arn_`, to dodge the
  Amazon Resource Name (ARN) clash). Flip is prefix-relative (`startsWith`) and length-safe; all three
  stay mutually disjoint. The 3 CI/CD+deploy files holding `zrn_` are gated for an explicit CI grant.
- *Jun 15, 2026 (resume):* `zmb:` memory `instance_id` prefix investigated — it is **comment/doc-only**
  (zero live `"zmb:"` string literal in `src/`); `agent_memory.zig` states the legacy NullClaw
  `zmb:` form "is gone with the in-child Postgres path." `protocol.zig` / `runner_fleet.md` still
  describe it as current — **doc drift**, cleaned in the §5.1 residue sweep, not a live namespace
  (no Indy decision required).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification | Clean; outcome in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, Failure Modes, Invariants, the agent-keys collision + brand-protect-list | Clean or dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR | Addressed before human review/merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Suite + cross-compile | `make test` + both linux targets | | |
| Entity residue zero | Eval `E1` | | |
| Keys disambiguated | Invariant 2 | | |
| Brand/residue | Eval `E4`, `E10` | | |
| Metrics | `test_metrics_renamed` | | |
| External gates | Evals `E6`–`E10` | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- 1Password vault renames — `ZMB_*` keeps (Indy verbatim, Jun 13, 2026: "i dont wanna rename the vault now").
- A data migration or backfill — pre-launch clean break (direct schema edit, no `ALTER`).
- History rewrites: `docs/v1`, `docs/v2/done`, `docs/architecture/archive`, `CHANGELOG.md` — untouched
  (Indy verbatim, Jun 15).
- GitHub org/repo rename — done upstream (Jun 12, 2026, redirects); ghcr namespace — flipped in M92_003.
- Marketing copy (M92_001); dotfiles operating-model prose (companion dotfiles commit at cutover).
