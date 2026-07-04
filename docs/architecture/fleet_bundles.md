# Fleet Bundles — source, storage, and the bundle/fleet split

> Parent: [`README.md`](./README.md) · Sibling: [`data_flow.md`](./data_flow.md) (the INSTALL sequence this storage backs).
>
> Scope: where a Fleet's `SKILL.md`, `TRIGGER.md`, and support files come from, how they are stored across Cloudflare R2 + Postgres, what is mutable, and what the runner reads at run time. Read this when you touch import, the bundle store, or the fleet-update path.

## Two layers: the immutable Bundle vs the live Fleet

A Fleet's definition lives in two distinct objects with different mutability and different runtime roles. Conflating them is the usual source of confusion.

| | **Fleet library entry** | **Fleet** |
|---|---|---|
| What | the onboarded snapshot of `SKILL.md` + `TRIGGER.md` + support files | the live runtime instance installed from a template |
| Table | `core.fleet_library` (platform) · `core.tenant_fleet_library` (tenant) | `core.fleets` |
| Mutability | **immutable** content — content-addressed; a re-onboard mints a new snapshot | **live** — `SKILL.md`/`TRIGGER.md` editable via `PATCH` |
| Object store | the canonical tar in R2 | — (records the content identity as `bundle_content_hash`) |
| Runtime role | source of **support files** (untarred into the sandbox) | source of **`SKILL.md`/`TRIGGER.md`** (ride every lease as `instructions`/`policy`) |

A fleet's `source_markdown`/`trigger_markdown` start as copies of the template's, but diverge the moment the user PATCHes them. The runner always executes the **fleet's** copy, not the template's onboard-time copy.

## Onboard: fetch, validate, re-pack (agentsfleet builds its own tar)

A template onboarded from a GitHub source is **not** a passthrough of GitHub's archive. The daemon fetches, validates, and **re-packs a fresh canonical tar**:

```
GitHub repo (author's source of truth)
   SKILL.md · TRIGGER.md · review.sh · lib/…
        │  POST /v1/admin/fleet-libraries  ·  POST /v1/workspaces/{ws}/fleet-libraries
        ▼
agentsfleetd
   1. fetch     GET api.github.com/repos/{owner}/{repo}/tarball/{ref}   (GitHub tarball API)
   2. validate  strip GitHub's wrapper dir; reject symlinks / ".." / absolute / dotfiles;
                cap 16 MiB decompressed, 4096 entries
   3. RE-PACK   canonicalTar(): a NEW deterministic tar — root-level, no wrapper,
                no symlinks — SKILL.md, optional TRIGGER.md, then each support file
   4. hash      content_hash = sha256(skill + trigger + support files)
   5. store     R2 + Postgres (below)
```

The re-pack is deliberate: the runner untars the result **without re-validating**, so the stored tar must be safe by construction. The author's repo framing is normalized away; only validated file *contents* survive. GitHub is the source for this one-time import and is **never** a runtime dependency.

## Two-tier Fleet library catalog (M103)

Templates onboard into one of two catalog tiers, each its own table:

- **Platform tier — `core.fleet_library`** (slug id, e.g. `github-pr-reviewer`). The global shop-window; a platform operator holding the `platform-library:write` scope onboards via `POST /v1/admin/fleet-libraries`. Migration-seeded rows bootstrap the catalog and become installable once onboarded (which populates their content hash + snapshot).
- **Tenant tier — `core.tenant_fleet_library`** (UUIDv7 id + `workspace_id` FK CASCADE). A workspace's own templates; a tenant admin holding `library:write` onboards via `POST /v1/workspaces/{ws}/fleet-libraries`, deduped on `(workspace_id, content_hash)`.

The workspace gallery `GET /v1/workspaces/{ws}/fleet-libraries` returns the union of all platform rows and that workspace's tenant rows, and nothing from another workspace.

## Storage map: R2 is the only content store (M103)

| Where | Key / columns | Holds |
|---|---|---|
| **R2** | `fleet-bundles/sha256/{content_hash}.tar` | the canonical tar (SKILL.md + TRIGGER.md + support files) — the **sole** support-file content store. Written only when support files are present. |
| **Postgres Fleet library tables** | `skill_markdown`, `trigger_markdown` | the SKILL.md / TRIGGER.md text |
| | `support_files_json` (JSONB) | a **manifest only** — `[{path, size_bytes, sha256}]`, never file bytes |
| | `content_hash`, `requirements_json` | the content address + parsed requirements (the R2 key is derived from `content_hash`, never stored as a public field) |
| **Postgres `core.fleets`** | `source_markdown`, `trigger_markdown`, `bundle_content_hash`, `bundle_snapshot_key` | the live, editable fleet copy + the content identity the runner materializes from |

Caps (`importer.zig`): **32 support files · 64 KiB per file · 256 KiB total.**

**No dual-write.** Support-file bytes live in R2 only; Postgres holds a path/size/hash manifest and the content hash. The legacy per-workspace `core.fleet_bundles` table — which stored full support-file content inline and was installed from by `bundle_id` — was removed; install now resolves from a Fleet library tier (below).

## Cardinality + dedup

- **R2: one object per unique content, globally.** Identical bytes from any source dedup to one `fleet-bundles/sha256/{hash}.tar`.
- **Tenant templates: one row per `(workspace_id, content_hash)`.** Re-onboarding identical bytes into a workspace converges on one row, one R2 object.
- **Platform templates: one row per slug id.** Re-onboarding refreshes the snapshot in place.
- **`core.fleets`: one row per fleet.** Many fleets may install from one template; the fleet stores the content hash, not a template reference.

## Runtime read path

At lease time (see [`data_flow.md` §C](./data_flow.md)):

- **SKILL.md / TRIGGER.md** → from the **lease** (`instructions`/`policy`, resolved from `core.fleets` fresh per lease, so they reflect any PATCH). The runner **ignores** the SKILL.md/TRIGGER.md copies inside the tar.
- **Support files** → the runner downloads the tar via `GET /v1/runners/me/bundles/{content_hash}` (daemon proxies `r2.get`; cached at `.bundle-cache/{hash}.tar`), and untars the support files into the per-lease sandbox workspace **before** the child forks. `SKILL.md` can then reference them (`review.sh`, playbooks, …).

## Update + sync

- **SKILL.md / TRIGGER.md are editable** via `PATCH /v1/workspaces/{ws}/fleets/{id}` (`source_markdown` / `trigger_markdown`). The edit is **in place on `core.fleets`** (reparse, validate the name still matches, bump revision). It does **not** mint a new template and does **not** change the fleet's `bundle_content_hash`.
- **Templates are immutable content** — a re-onboard with changed bytes mints a new snapshot (new `content_hash`); existing fleets are unaffected.
- **Install is from a template only** — `POST /v1/workspaces/{ws}/fleets` accepts exactly `{platform_library_id}` or `{tenant_library_id}`. Raw-SKILL paste and the legacy `bundle_id` install were removed (M103 §4).
- **Support files are NOT editable in place.** They live only in the immutable R2 snapshot, so changing one requires re-onboarding the template (new `content_hash`) and re-installing. No fleet-level support-file override today. 🟡 gap.
- **No GitHub → template sync.** Onboard is one-time; pushing a new commit to the source repo does nothing. Re-sync is a manual re-onboard. 🟡 gap.

## Notable invariants

- **The stored tar is agentsfleet's canonical re-pack, never GitHub's archive.** Safe-by-construction so the runner untars without re-validating.
- **The runner's behaviour comes from the fleet's live SKILL.md, not the template's.** A PATCH takes effect on the next lease; the tar's onboard-time copies are inert.
- **Secrets never enter R2 or the snapshot.** Credentials are vault refs (`fleet:<source>`), resolved at lease and delivered inline; the tar carries only author-authored files.
- **Content-addressing makes onboarding idempotent.** Re-onboarding identical bytes reuses the same R2 object and (per workspace, for the tenant tier) the same template row.
