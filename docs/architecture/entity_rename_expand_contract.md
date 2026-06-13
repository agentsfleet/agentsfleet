# Entity rename `zombie` → `agent`: naming decision + expand-contract cutover design

**Status:** canonical for M92_004 (the entity + platform cutover). Authored Jun 13, 2026
from Indy's naming directives (`/private/tmp/agentsfleet_naming_handoff.md`, the Jun 12–13
rebrand sessions) and the M92_003 rename-principle precedent.

## Naming decision (Indy, binding)

- **`agentsfleet` is the product namespace** — binaries, packages, env-var prefix, image
  names, domains. One word; never `agents_fleet`.
- **`agent` is the row-level entity** — tables, wire fields, routes, module folders.
- **AVOID:** `agentsfleet` as an entity name (`agents_fleet_id`, `/agentsfleets`,
  `core.agents_fleet` are all wrong by construction).

| Surface | Old | New |
|---|---|---|
| schema | `core.zombies`, `core.zombie_{events,sessions,approval_gates,approval_gates_append_only,execution_telemetry}` | `core.agents`, `core.agent_*` |
| wire fields | `zombie_id`, `zombie_slug` | `agent_id`, `agent_slug` |
| routes | `/zombies`, `/v1/workspaces/{ws}/zombies/{id}` | `/agents`, `/v1/workspaces/{ws}/agents/{id}` |
| modules | `src/agentsfleetd/zombie/` | `src/agentsfleetd/agent/` (singular — repo convention is singular domain folders) |
| CLI verb | `agentsfleet zombie …` | `agentsfleet agent …` (`agent install` is what the skills repo documents) |
| env prefix | `ZOMBIE_*` | `AGENTSFLEET_*` (product namespace, per Indy: "the ZOMBIE_ is AGENTSFLEET_") |
| headers | `x-usezombie*`, `x-usezombie.triggers[]` | `x-agentsfleet*`, `x-agentsfleet.triggers[]` |
| metrics | `zombie_runner_*` | `agentsfleet_runner_*` (runner is a product component, not the entity) |

## Why expand-contract (and not a one-shot rename)

`schema/*.sql` migrations are frozen (append-only parity); the fly apps serve live traffic
during the cutover; the dashboard, CLI, and runner deploy on different cadences. A one-shot
rename forces a synchronized deploy of every consumer — one missed surface is an outage.
Expand-contract lets each consumer flip independently while both names serve.

## The three stages

**Stage 1 — EXPAND (one migration, additive only).**
New migration creates `core.agents` (+ the five `core.agent_*` satellites) as the canonical
tables. Existing data moves once (`INSERT … SELECT`); `core.zombies` and satellites become
updatable views (or synonyms where the engine allows) over the new tables so every old
query keeps answering. API: `/agents` routes register as canonical; `/zombies` routes stay
as thin aliases onto the same handlers. Wire: responses carry **both** `agent_id` and
`zombie_id` (same value); requests accept either. Headers: emit `x-agentsfleet*`, accept
both. Nothing breaks at this stage boundary; old clients are fully served.

**Stage 2 — FLIP consumers.**
Dashboard, CLI (`agentsfleet agent …`), runner, fixtures, OpenAPI document, docs, skills
repo (`agentsfleet/skills#4` merges here — LAST, per the merge-order constraint) move to
the new names. Env vars flip `ZOMBIE_*` → `AGENTSFLEET_*` (hard cutover, RULE NLG — no
dual-read shim; pre-launch blast radius, Indy's `.zshrc` already renamed, dotfiles
`20f7300`). Each consumer ships and verifies independently against the dual-serving API.

**Stage 3 — CONTRACT (one migration + route removal).**
After every consumer is verified on new names: drop the compatibility views, drop the
`/zombies` aliases, stop emitting `zombie_id`/`zombie_slug`, reject `x-usezombie*`.
Contract criteria are observable, not calendar-based: zero `/zombies` hits and zero
`zombie_id` request-field occurrences in API logs over a full deploy cycle, plus the
fleet's runners and CLI installs at versions that speak `agent_*`.

## Constraints carried from the rename principle

*Names operators type and see get renamed; identifiers external systems resolve keep
resolving until their own cutover.* The platform identities (fly `zombied-{dev,prod}`
apps, `api(-dev).usezombie.com` hosts, Vercel `usezombie-*` projects, Postgres
`usezombie`/`usezombiedb`/`usezombie-admin` creds) each cut over inside M92_004 with their
own external-resolver step (DNS row, fly app create, Vercel rename, cred rotation) — never
implicitly via a string sweep.

**API host split (Indy verbatim, Jun 13, 2026):** prod `api.usezombie.com` →
`api.agentsfleet.net`; dev `api-dev.usezombie.com` → `api-dev.agentsfleet.dev` — the
deliberate prod-`.net` / dev-`.dev` split. (Supersedes the earlier naming-handoff line
that sent dev to `.net`.) Blast radius rides with the host flip: Clerk JSON Web Token (JWT)
`aud` claims + backend audience validation, `NEXT_PUBLIC_API_URL` defaults + fixtures,
fly/cloudflared host config, workflow health URLs, OpenAPI `servers`.

**Explicit keeps (Indy, Jun 13, 2026):** 1Password vault names stay `ZMB_*` ("i dont wanna
rename the vault now") — `op://` refs, `vars.VAULT_*`, and the AGENTS.md vault list are
out of every sweep. `docs/v1`, `docs/v2/done`, `docs/architecture/archive`, `CHANGELOG.md`,
and frozen `schema/*.sql` migration files stay byte-stable; the expand and contract
migrations are NEW files appended to the migration array.

## Already done upstream (do not re-plan)

- GitHub org + repo renamed to `agentsfleet/agentsfleet` (Jun 12, 2026; redirects serve
  old URLs). The ghcr namespace flip (`ghcr.io/agentsfleet/`) landed in M92_003 because
  ghcr serves no redirects.
- Binary/target/package renames (`agentsfleet`, `agentsfleetd`, `agentsfleet-runner`,
  `@agentsfleet/design-system`) — M92_003.
