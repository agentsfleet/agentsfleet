<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M92_004: `agentsfleet` brand cutover + user-facing prose → `agent` (zombie data surface retained)

**Prototype:** v2.0.0
**Milestone:** M92
**Workstream:** 004
**Date:** Jun 15, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the last `zombie`/`usezombie` surfaces are the ones users read and type: error text, CLI/dashboard labels, env vars, request headers, live hosts, the installer domain
**Categories:** API, CLI, INFRA, OBS, UI
**Batch:** B4 — follows M92_003 (B3) merge; one mega-spec per Indy ("one spec, not three")
**Branch:** feat/m92-004-agent-entity-cutover
**Test Baseline:** unit=1946 integration=189
**Depends on:** M92_003 (binary/package names this spec's consumers ship under; its E9 npm gate is shared by §5), M92_002 Dimension 6.1 (agentsfleet.net DNS rows — §4's host flips extend the same registrar work)
**Provenance:** agent-generated (Indy's rename sessions Jun 12–15, 2026; sources: `/private/tmp/agentsfleet_naming_handoff.md`, the M92_003 amendments handoff, M92_003 spec Discovery, and the Jun 14–15 settled-design decisions captured below)

---

## Settled design (Indy, Jun 14–15, 2026, binding)

This spec was first drafted as an expand/contract entity rename `zombie`→`agent`. Indy's Jun 14–15
decisions narrowed it to a **brand cutover with the `zombie` data surface retained**. The line:

| Layer | Decision | Examples |
|---|---|---|
| **Data surface — stays `zombie`** | Untouched. No migration, no `ALTER`. | `/zombies` routes; `zombie_id`/`zombie_slug` wire fields; `core.zombies`/`core.zombie_*` tables + columns; `UZ-ZMB-*` error *codes*; `zombie_paused`/`zombie_config_changed` wire values |
| **Internals already flipped — kept** | Don't revert what's done. | `ERR_AGENTSFLEET_*`/`MSG_AGENTSFLEET_*` const *names*; `agent:` Redis key prefixes; `AGENTSFLEET_*` test-fixture constants |
| **User-facing prose — flips to `agent`** | "Zombie not found" → "Agent not found", across CLI, UI, API. | error titles/hints; CLI `.description()`/help/output; UI labels/headings; OpenAPI descriptions/summaries |
| **Brand/namespace — flips to `agentsfleet`** | The product namespace. | `usezombie.com`→`agentsfleet.net`; **`usezombie.sh`→`agentsfleet.dev`**; `ZOMBIE_*` env→`AGENTSFLEET_*`; `x-usezombie*`→`x-agentsfleet*`; `zombie_runner_*` metrics→`agentsfleet_runner_*`; hosts; npm |

**Verbatim acks (Indy):**
- "if we keep zombies url then the zombie_id can remain." → data surface stays `zombie`; no migration.
- "if changes have been done, then lets keep it." → already-flipped internals are kept, not reverted.
- "the errors like Zombie not found … user facing can be updated to `Agent not found` … CLI, UI, API."
- "the usezombie.sh must be agentsfleet.dev (rename)." (Jun 15 — reverses the earlier M92_002 keep.)
- "I want all to be fixed in the PR. Indy accepts that the product is down after the fix." → one PR;
  new API/app hosts do not resolve until Indy stands up Domain Name System (DNS) + Clerk JSON Web
  Token (JWT) `aud` post-merge (accepted; pre-launch, no live external consumers).

**Schema latitude (Indy, Jun 15):** pre-launch (until v2), `schema/*.sql` may be edited directly — no
append-only migration, no `ALTER`. Unused here because the data surface stays `zombie`.

**Why this is coherent:** the already-flipped internal symbols (`ERR_AGENTSFLEET_NOT_FOUND`) now *match*
the user-facing "Agent" message they carry; only the persistence + wire identifiers (`zombie_id`,
`/zombies`, `core.zombies`, `UZ-ZMB-*`) stay `zombie`, because changing those is the expensive/risky
part Indy chose to keep.

**Pre-existing `agent` concept is distinct (do not touch):** `core.agent_keys`, `agent_id` (the
agent-key id), `/agent-keys` routes, `UZ-AGENT-*` codes, `AuthMode.agent_key` already exist (keys
bound to a `zombie`). They are NOT the `zombie` entity and are out of scope.

---

## Implementing agent — read these first

1. The **Settled design** table above — the binding keep/flip line; it governs every edit.
2. `docs/v2/active/M92_003_P1_API_CLI_INFRA_OBS_AGENTSFLEET_BINARY_TARGET_RENAME.md` (or `done/`) — the
   §1 ledger + keep-pin eval pattern (E1/E3 count compares) this spec reuses; the Discovery entries
   documenting what is already renamed.
3. `docs/REST_API_DESIGN_GUIDELINES.md` + the `/zombies` handlers — routes stay `zombie`; only
   description prose flips. `docs/AUTH.md` — §4 moves Clerk JWT `aud` claims (auth-flow read fires).
4. `dispatch/write_zig.md` — daemon edits (ZIG/PUB/LIFECYCLE gates; cross-compile both linux targets);
   `dispatch/write_ts_adhere_bun.md` — CLI/dashboard edits.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m92): agentsfleet brand cutover + user-facing agent (zombie data surface kept)`
- **Intent (one sentence):** a user reading any surface — an error message, a dashboard label, a CLI
  description, the install command, an env var, a request header, a live hostname — sees
  `agent`/`agentsfleet` words, while their code keeps reading `zombie_id` from `/zombies` (the data
  surface is deliberately retained, so nothing in a client breaks and no migration runs).
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`.
  Confirm against the live world: (a) fresh blast-radius grep matches the §1 ledger; (b) M92_003 merged
  and its gated rows' state is known (E9 npm org, E10 installer domain); (c) each §4 external resolver
  has an Indy console row sequenced. A `[?]` blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a workspace owner reads "Agent not found" instead of "Zombie not found",
   opens the dashboard and sees "Agents", runs `agentsfleet agent install`, and installs from
   `agentsfleet.dev` — every word they read says agent/agentsfleet, and their existing `GET /zombies`
   call with `zombie_id` still works unchanged.
2. **Preserved user behaviour** — the data surface is byte-stable: `/zombies` routes, `zombie_id`/
   `zombie_slug` fields, `core.zombies` schema, and `UZ-ZMB-*` error codes are unchanged, so no client
   and no stored data breaks. Install/login/run flows never break.
3. **Optimal-way check** — keeping the data surface is the direct path: it removes the migration and the
   broken-wire risk entirely while still delivering the agent/agentsfleet words users actually read.
4. **Rebuild-vs-iterate** — iterate; zero data-surface change; brand + prose only.
5. **What we build** — the `agentsfleet` brand/namespace cutover (env, headers, metrics, hosts, npm,
   installer domain), user-facing prose → `agent` (error catalog, CLI, UI, OpenAPI), four gated platform
   cutovers (fly apps, API hosts, Vercel projects, Postgres creds), npm deprecation pointer, mail flip,
   residue sweep, `samples/` decommission. Plus two pre-approved Zig refactors (route matcher;
   `@This()`/decl-literal passes).
6. **What we do NOT build** — a schema migration; a `zombie`→`agent` data-surface rename; compatibility
   shims; vault renames (`ZMB_*` keeps, Indy verbatim); history rewrites (`docs/v1`, `docs/v2/done`,
   archive, `CHANGELOG.md`, frozen migrations); marketing copy (M92_001).
7. **Fit with existing features** — completes M92: identity (002), operational names (003), brand +
   user-facing (this). Must not destabilize the install path or live fly traffic; every resolver flip is
   gated on its external step verifying.
8. **Surface order** — user-facing prose + brand first (the bulk, already largely done), then platform
   identities (gated), then residue/decommission.
9. **Dashboard restraint** — UI only re-words labels; no new controls or claims; routes stay `/zombies`.
10. **Confused-user next step** — old env vars (`ZOMBIE_*`) error loudly with the renamed-var hint;
    `usezombie.com`/`usezombie.sh` redirect to the agentsfleet domains.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE NLR (touched files shed stale `usezombie`/`zombie`-prose
  comments where they are genuinely user-facing), RULE NLG (no legacy shims), RULE ORP (orphan sweep per
  dropped `samples/platform-ops` symbol), RULE TST-NAM (tests milestone-free), RULE UFS (header/env names
  as named constants).
- **`dispatch/write_zig.md`** — daemon metric rename + the two struct refactors (ZIG/PUB/LIFECYCLE gates;
  cross-compile both linux targets).
- **`dispatch/write_ts_adhere_bun.md`** — CLI + dashboard prose flips.
- **`dispatch/write_http.md`** + **`docs/REST_API_DESIGN_GUIDELINES.md`** — OpenAPI description edits;
  routes unchanged.
- **`docs/AUTH.md`** — JWT audience changes in §4.
- **`docs/LOGGING_STANDARD.md`** — only where a log scope is genuinely user-facing prose; entity log
  scopes stay.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — metric rename, two refactors | read façade; cross-compile both linux targets |
| SCHEMA GUARD | no — no migration; data surface retained | — |
| PUB / Struct-Shape | yes — `@This()`/decl-literal refactor | shape verdicts per touched file; no surface growth |
| File & Function Length | yes — refactors touch large files | keep file splits; no file crosses 350 |
| UFS | yes — env/header/metric literals recur | named constants at module scope |
| UI Substitution / DESIGN TOKEN | no — label re-wording only, no new markup | — |
| LOGGING | low — only user-facing scope prose | per `docs/LOGGING_STANDARD.md` |
| ERROR REGISTRY | yes — message *prose* flips; codes `UZ-ZMB-*` stable | titles/hints → "Agent"; codes unchanged; pins updated |
| CI/CD edit guard | yes — workflow host/env/installer strings | enumerate per workflow in PR body; strings-only; Indy grant per session |

---

## Overview

**Goal (testable):** after this PR, the daemon error catalog, CLI, dashboard, and OpenAPI document read
`agent`/`agentsfleet` in every human-facing string, the `agentsfleet` brand replaces `usezombie`
(including the `agentsfleet.dev` installer domain) across code, and a repo-wide `usezombie` grep over
source matches only flagged machine/parity keeps — while `git grep` proves the `zombie` data surface
(`zombie_id`, `/zombies`, `core.zombies`, `UZ-ZMB-*`) is byte-stable and `make test` is green.

**Problem:** users still read "Zombie" in errors/labels and type `usezombie` install/host strings after
M92_002/003 renamed the binaries and identity.
**Solution summary:** flip user-facing prose to `agent` and the brand to `agentsfleet`; retain the
`zombie` data surface (no migration); cut four platform identities behind their own external gates.

---

## Prior-Art / Reference Implementations

- **Rename pattern** → M92_003: ledger → flip → eval-pin both directions; keep-pin count compares reused.
- **Prose-vs-identifier discrimination** → the word-boundary rule (`\b[Zz]ombie\b` flips standalone prose
  but never `zombie_id`/`/zombies`/`usezombie`/`ZombieStatus`); applied with judgment, not blind sed.
- **Gated-surface pattern** → M92_002 Dimension 6.1 / M92_003 §4: a parked external gate parks only its
  surface.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/architecture/entity_rename_expand_contract.md` | DELETE | superseded design doc; decisions live in this spec (Indy) |
| `src/agentsfleetd/errors/error_entries*.zig`, `error_registry.zig`, `error_registry_test.zig` | EDIT | error titles/hints prose → "Agent"; `UZ-ZMB-*` codes + pins kept/updated |
| `src/agentsfleetd/auth/{jwks_test,oidc,middleware/bearer_or_api_key}.zig`, `http/runner_enrollment_integration_test.zig` | EDIT | JWT test fixtures: RSA exponent `e` → `AQAB` (un-clobber) |
| `agentsfleet/src/**` (CLI, ~11 files) | EDIT | display strings (descriptions/help/output) → "agent"; identifiers kept |
| `ui/packages/app/**` (~5 files) | EDIT | dashboard display text → "agent"; routes/types/fields kept |
| `public/openapi/{root.yaml,paths/zombies.yaml}` + regen `public/openapi.json` | EDIT | description/summary prose → "agent"; field names/paths kept; `x_usezombie`→`x_agentsfleet` |
| `src/agentsfleetd/observability/metrics_runner.zig` (+ `_test.zig`) + `deploy/grafana/runner_fleet.json` | EDIT | `zombie_runner_*` → `agentsfleet_runner_*` metrics + grafana queries (lockstep) |
| install script, `README*`, website install snippet, config | EDIT | installer domain `usezombie.sh` → `agentsfleet.dev`; `usezombie.com` → `agentsfleet.net` |
| `~128 already-flipped files` (brand + kept internals) | EDIT | committed as-is per "keep what's done" |
| `src/agentsfleetd/http/routes.zig` + struct files across `src/agentsfleetd/**` | EDIT | the two pre-approved Zig refactors |
| `deploy/fly/**`, cloudflared, `.github/workflows/**` (hosts/env strings) | EDIT | fly app cutover + API host split (gated; Indy CI grant) |
| `samples/platform-ops/` (delete), `samples/fixtures/`→test dirs, `error_entries.zig` pointer | DELETE/EDIT | decommission in-repo samples — skill migrated to `agentsfleet/skills` |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream; brand + prose are the bulk, platform identities are gated tails.
- **Alternatives considered:** full `zombie`→`agent` data-surface rename (rejected by Indy — keep
  `zombie_id`, no migration); reverting the already-done internal over-flips (rejected — "keep it").
- **Patch-vs-refactor verdict:** **patch** (string/prose/brand edits, no behaviour or schema change),
  plus two isolated pre-approved Zig refactors carried in their own bisectable commits.

---

## Sections (implementation slices)

### §1 — Blast-radius ledger + keep-pins (blocks every flip)

Fresh repo-root grep per token (`usezombie`, `usezombie.sh`, `ZOMBIE_`, `x-usezombie`, `zombie_runner_`,
the prose word `zombie`, plus the data-surface keeps `zombie_id`/`/zombies`/`core.zombie`/`UZ-ZMB`).
Every hit dispositioned brand-FLIP / prose-FLIP / data-surface-KEEP / internal-kept / machine-parity-keep.

- **Dimension 1.1** — ledger complete, classes dispositioned → Eval `E1`
- **Dimension 1.2** — keep-pin baseline recorded (`ZMB_*` vaults, data surface, frozen history) → Eval `E3`

### §2 — Brand cutover + retained `zombie` data surface

`usezombie.com`→`agentsfleet.net`; `usezombie.sh`→`agentsfleet.dev` (installer domain, Indy Jun 15);
`ZOMBIE_*` env→`AGENTSFLEET_*` (hard cutover, no dual-read); `x-usezombie*`→`x-agentsfleet*` headers;
`zombie_runner_*`→`agentsfleet_runner_*` metrics with grafana queries in the same commit. The `zombie`
data surface is NOT touched: routes, wire fields, schema, error codes stay byte-stable.

- **Dimension 2.1** — `git grep usezombie` over source == flagged keeps only → Eval `E4`
- **Dimension 2.2** — env prefix flipped; no `ZOMBIE_*` read remains in source → Eval `E4` + negative grep
- **Dimension 2.3** — metrics renamed; `/metrics` exposes `agentsfleet_runner_*`; grafana renders → Test `test_metrics_renamed` + provisioning check
- **Dimension 2.4** — installer domain serves at `agentsfleet.dev`; `usezombie.sh` live refs == 0 → Eval `E10`

### §3 — Consumer prose flips (CLI, UI) + env/CLI verb

CLI display strings (`agentsfleet/src/**`) and dashboard text (`ui/packages/app/**`) → "agent"; the
`zombie` module directory stays (no move); CLI `agent` is the documented verb (`zombie` kept as alias).

- **Dimension 3.1** — daemon builds; cross-compile both linux targets; full suite green → Eval `E2`
- **Dimension 3.2** — CLI help/output reads "agent"; `--zombie` flag + `zombie_id` arg names unchanged → Test `test_cli_prose` (subprocess)
- **Dimension 3.3** — dashboard labels read "Agents"; `/zombies` routes + `zombie_id` access unchanged → Test `test_ui_prose` (e2e)

### §4 — Platform identities (four independent external gates; each parks only its surface)

Fly apps `zombied-{dev,prod}` → new app names; API hosts split prod `api.agentsfleet.net` / dev
`api-dev.agentsfleet.net` (DNS + Clerk JWT `aud` + `NEXT_PUBLIC_API_URL` + fixtures + cloudflared +
workflow URLs + OpenAPI servers in one gated edit per host); Vercel projects `usezombie-{app,website}`
renamed; Postgres creds rotated via the vault (values only; `ZMB_*` vault names keep).

- **Dimension 4.1** — fly cutover → Eval `E6` (Indy row)
- **Dimension 4.2** — API host split; JWT `aud` validated on new hosts → Eval `E7` (Indy row)
- **Dimension 4.3** — Vercel projects renamed → Eval `E8` (Indy row)
- **Dimension 4.4** — db creds rotated; `make test-integration` green → Eval `E9` (Indy row)

### §5 — npm deprecation + mail + skills cadence

`npm deprecate @usezombie/zombiectl` → `@agentsfleet/cli` (gated on M92_003 E9); mail `hello@`/`team@
usezombie.com` → `@agentsfleet.net` (Indy row; the `usezombie@agentmail.to` support local-part is a
cross-tier parity identifier — confirm with Indy before editing); `INSTALL_SKILL_SLASH` →
`/agentsfleet-install-platform-ops` with `agentsfleet/skills#4`.

- **Dimension 5.1** — old npm listing shows the deprecation pointer → Eval `E10`
- **Dimension 5.2** — mail flip verified; repo refs updated → Indy row + grep
- **Dimension 5.3** — slash-command constant + pins flip with skills#4 → Test `test_install_skill_slash_pin`

### §6 — User-facing prose → `agent` (error catalog, CLI, UI, OpenAPI)

Daemon error catalog titles/hints "Zombie…" → "Agent…" with `UZ-ZMB-*` codes + message *values* stable
where they are wire values; CLI/UI display text; OpenAPI descriptions/summaries. Identifiers
(`zombie_id`, `/zombies`, `core.zombies`, type names) untouched. Article grammar (`a zombie`→`an agent`).

- **Dimension 6.1** — error catalog titles/hints read "Agent"; `error_registry_test.zig` pins updated; codes stable → Test `test_error_prose_agent`
- **Dimension 6.2** — OpenAPI descriptions read "agent"; `zombie_id`/`/zombies`/operationIds intact → Test `test_openapi_prose_agent`

### §7 — Residue sweep, hygiene + `samples/` decommission

Brand residue: Dockerfile labels, systemd `Description=`, compose headers, `github.com/usezombie` refs,
`docs.usezombie.com` URLs. Orphan sweep per RULE ORP. **`samples/platform-ops/` decommission:** delete +
repoint its 5 consumers (`postinstall.mjs` copier, `error_entries.zig` example pointer, `test-unit-bundle`
lane, frontmatter/seed fixture readers). `samples/fixtures/` is parser test data — relocate into test
dirs, never delete. **Two pre-approved Zig refactors** (route-enum/matcher; `@This()`/decl-literal struct
passes) land here in their own bisectable commits, after the rename is green.

- **Dimension 7.1** — residue grep matches only frozen-history keeps → Eval `E1` final
- **Dimension 7.2** — orphan sweep + dead-code table complete → Eval `E5`
- **Dimension 7.3** — `samples/platform-ops/` removed + consumers repointed; `test-unit-bundle` green → Test `test_samples_decommissioned`
- **Dimension 7.4** — two Zig refactors land; cross-compile + full suite green; no pub-surface growth → Eval `E2` + PUB-gate verdicts

---

## Interfaces

**Retained (locked `zombie`, byte-stable — changing any requires amending this spec):** `/zombies` +
`/v1/workspaces/{ws}/zombies/{id}` routes; `zombie_id`/`zombie_slug` wire fields; `core.zombies` +
`core.zombie_*` tables/columns; `UZ-ZMB-*` error codes; `zombie_paused`/`zombie_config_changed` wire
values. **New surfaces (locked `agentsfleet`/`agent`):** `AGENTSFLEET_*` env prefix; `x-agentsfleet*`
headers; `agentsfleet_runner_*` metric names; hosts `api.agentsfleet.net` / `api-dev.agentsfleet.net`;
installer `agentsfleet.dev`. `ZMB_*` vault names are out of scope of every sweep (Indy keep).

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| A prose flip corrupts an identifier | over-broad sed hits `zombie_id`/`/zombies`/a type name | word-boundary rule + judgment + `git grep` data-surface invariant is merge-blocking; build fails loud |
| Old env var silently ignored | hard `ZOMBIE_*`→`AGENTSFLEET_*` cutover | §2.2 negative grep + loud diagnostic on legacy `ZOMBIE_*` presence |
| Metric/grafana drift | metric renamed without grafana | 2.3 flips emit sites + `deploy/grafana/runner_fleet.json` queries in one commit |
| JWT rejections post host flip | `aud` claim mismatch | 4.2 gated edit changes Clerk + backend validation together; e2e login test on new host first |
| Installer points at dead domain | `usezombie.sh` flipped before `agentsfleet.dev` serves | 2.4 gated on the domain answering; live refs grep == 0 only after |
| Mail/parity identifier broken | unilateral one-file edit of a cross-tier mirrored value | §5.2 confirm-with-Indy gate before editing parity files |

---

## Invariants

1. The `zombie` data surface is byte-stable — `git grep` for `zombie_id`/`zombie_slug`/`/zombies`/
   `core\.zombie`/`UZ-ZMB-` is unchanged from `origin/main` for those tokens (Eval `E3`).
2. No schema migration ships and `schema/*.sql` is untouched — Schema Removal Guard finds nothing.
3. `ZMB_*` vault names appear in zero diffs — Eval `E3` keep token.
4. Every user-facing surface reads `agent`/`agentsfleet`; `git grep usezombie` over source == flagged
   keeps only — Eval `E4`.
5. fly/host/Vercel/cred identifiers change only inside their gated Dimension — Eval `E6`–`E9`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1–1.2 | eval | `E1`, `E3` baseline | ledger matches grep reality; data-surface + keep counts byte-stable |
| 2.1–2.2 | unit | `test_env_prefix_flipped` | config loader reads `AGENTSFLEET_*`; legacy `ZOMBIE_*` presence → loud diagnostic |
| 2.3 | integration | `test_metrics_renamed` | `/metrics` exposes `agentsfleet_runner_*`; no `zombie_runner_*`; grafana provisioning valid |
| 2.4 | eval | `E10` | `usezombie.sh` live refs == 0; install command names `agentsfleet.dev` |
| 3.1 | integration | full suite + cross-compile | both linux targets exit 0; counts vs baseline |
| 3.2 | e2e | `test_cli_prose` | subprocess: help/output reads "agent"; `--zombie`/`zombie_id` names unchanged |
| 3.3 | e2e | `test_ui_prose` | dashboard reads "Agents"; `/zombies` routes + `zombie_id` access unchanged |
| 4.1–4.4 | e2e (manual) | `E6`–`E9` | each external resolver answers on new identity; evidence in PR body |
| 5.1/5.3 | e2e/unit | `E10` / `test_install_skill_slash_pin` | npm pointer; slash-command constant + pin flip together |
| 6.1 | unit | `test_error_prose_agent` | error catalog titles/hints read "Agent"; `UZ-ZMB-009` code + 404 stable; pins updated |
| 6.2 | unit | `test_openapi_prose_agent` | OpenAPI descriptions read "agent"; `zombie_id` fields + `/zombies` paths + operationIds intact |
| 7.1–7.2 | eval | `E1` final + `E5` | residue and orphans zero outside frozen keeps |
| 7.3 | integration | `test_samples_decommissioned` | suites green with `samples/platform-ops/` gone, fixtures test-local |
| 7.4 | eval | `E2` + PUB verdicts | two refactors land; full suite + cross-compile green; no pub-surface growth |

**Regression:** `make test`, `make test-integration`, app/website suites, installer `install_test.sh` —
green. **Data-surface guard:** the existing `/zombies` + `zombie_id` tests continue to pass unchanged.

---

## Acceptance Criteria

- [ ] User-facing prose reads `agent` — verify: `test_error_prose_agent`, `test_openapi_prose_agent`,
      CLI/UI e2e
- [ ] Brand reads `agentsfleet` incl. `agentsfleet.dev` installer — verify: Eval `E4`, `E10`
- [ ] `zombie` data surface byte-stable; no migration — verify: Eval `E3`; Schema Guard empty
- [ ] Metrics + grafana flipped together — verify: `test_metrics_renamed`
- [ ] Two Zig refactors land green — verify: Eval `E2` + PUB verdicts
- [ ] Each §4 gate either verified (evidence in PR body) or parked-with-surface — verify: `E6`–`E9`
- [ ] `make lint && make test` green; cross-compile both linux targets; `gitleaks detect` clean; no
      non-md file over 350 lines

---

## Eval Commands (post-implementation)

```bash
# E1: residue completeness (expect flagged keeps only outside frozen history)
git grep -nE "usezombie" -- src/ agentsfleet/src ui/packages | grep -vE "agentmail|usezombie\.dev/role"
# E2: build + suite — zig build && make test 2>&1 | tail -3 ; cross-compile both linux targets
# E3: data-surface byte-stability — git grep -cE "zombie_id|zombie_slug|/zombies|core\.zombie|UZ-ZMB-" vs origin/main
# E4: env/brand — git grep -nE "ZOMBIE_[A-Z_]+|x-usezombie" -- src/ agentsfleet/src (expect 0 real env/header)
# E5: orphan sweep — grep -rn "samples/platform-ops" src/ (expect 0)
# E10: installer — git grep -nE "usezombie\.sh" (live refs; expect 0)
# E6/E7/E8/E9: fly/host/Vercel/creds — Indy console rows; paste evidence in PR body
```

---

## Dead Code Sweep

| File to delete | Verify |
|----------------|--------|
| `docs/architecture/entity_rename_expand_contract.md` | `test ! -f docs/architecture/entity_rename_expand_contract.md` |
| `samples/platform-ops/` | `test ! -d samples/platform-ops` + `test-unit-bundle` green |

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `zombie_runner_*` metrics + old grafana uid | `grep -rn zombie_runner_ src/ deploy/grafana/` | 0 |
| `samples/platform-ops` consumers | Eval `E5` | 0 |

---

## Discovery (consult log)

- *Jun 15, 2026:* JWT test-fixture blocker root-caused — the RSA exponent `e` was clobbered to the
  modulus value across 4 auth files; fix is `e`→`AQAB` (verified: token verifies). No fixture
  consolidation needed (drops original "refactor #1").
- *Jun 15, 2026:* Indy verbatim acks captured in **Settled design** above; expand/contract design + its
  architecture doc retired.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification | Clean; outcome in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, Failure Modes, Invariants, the data-surface guard | Clean or dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR | Addressed before human review/merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Suite + cross-compile | `make test` + both linux targets | | |
| Data-surface byte-stable | Eval `E3` | | |
| Brand/residue | Eval `E1`, `E4` | | |
| Metrics | `test_metrics_renamed` | | |
| External gates | Evals `E6`–`E10` | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- 1Password vault renames — `ZMB_*` keeps (Indy verbatim, Jun 13, 2026: "i dont wanna rename the vault now").
- A `zombie`→`agent` data-surface rename or schema migration (Indy: keep `zombie_id`).
- History rewrites: `docs/v1`, `docs/v2/done`, `docs/architecture/archive`, `CHANGELOG.md`, frozen
  `schema/*.sql` — append-only / byte-stable.
- GitHub org/repo rename — done upstream (Jun 12, 2026, redirects); ghcr namespace — flipped in M92_003.
- Marketing copy (M92_001); dotfiles operating-model prose (companion dotfiles commit at cutover).
