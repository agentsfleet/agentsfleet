# Fleet Bundles — source, storage, and the bundle/fleet split

> Parent: [`README.md`](./README.md) · Sibling: [`data_flow.md`](./data_flow.md) (the INSTALL sequence this storage backs).
>
> Scope: where a Fleet's `SKILL.md`, `TRIGGER.md`, and support files come from, how they are stored across Cloudflare R2 + Postgres, what is mutable, and what the runner reads at run time. Read this when you touch import, the bundle store, or the fleet-update path.

## Two layers: the immutable Bundle vs the live Fleet

A Fleet's definition lives in two distinct objects with different mutability and different runtime roles. Conflating them is the usual source of confusion.

| | **Bundle** | **Fleet** |
|---|---|---|
| What | the import-time snapshot of `SKILL.md` + `TRIGGER.md` + support files | the live runtime instance created from a bundle (or from direct markdown) |
| Table | `core.fleet_bundles` | `core.fleets` |
| Mutability | **immutable** — content-addressed; never edited in place | **live** — `SKILL.md`/`TRIGGER.md` editable via `PATCH` |
| Object store | the canonical tar in R2 | — (references the bundle by `content_hash`) |
| Runtime role | source of **support files** (untarred into the sandbox) | source of **`SKILL.md`/`TRIGGER.md`** (ride every lease as `instructions`/`policy`) |

A fleet's `source_markdown`/`trigger_markdown` start as copies of the bundle's, but diverge the moment the user PATCHes them. The runner always executes the **fleet's** copy, not the bundle's import-time copy.

## Import: fetch, validate, re-pack (agentsfleet builds its own tar)

A bundle created from a GitHub source is **not** a passthrough of GitHub's archive. The daemon fetches, validates, and **re-packs a fresh canonical tar**:

```
GitHub repo (author's source of truth)
   SKILL.md · TRIGGER.md · review.sh · lib/…
        │  POST /v1/workspaces/{ws}/fleets/bundles/snapshots
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

## Storage map: R2 + Postgres (and the current redundancy)

| Where | Key / columns | Holds |
|---|---|---|
| **R2** | `fleet-bundles/sha256/{content_hash}.tar` | the canonical tar (SKILL.md + TRIGGER.md + support files). Written only when support files are present. |
| **Postgres `core.fleet_bundles`** | `skill_markdown`, `trigger_markdown` | the SKILL.md / TRIGGER.md text |
| | `support_files_json` (JSONB) | **full** support-file content (`[{path, content}]`) |
| | `content_hash`, `snapshot_key`, `requirements_json` | the address + parsed requirements |
| **Postgres `core.fleets`** | `source_markdown`, `trigger_markdown`, `bundle_id`, `bundle_content_hash` | the live, editable fleet copy + the bundle reference |

Caps (`importer.zig`): **32 support files · 64 KiB per file · 256 KiB total.**

**Known redundancy (open decision).** Support-file *content* is stored twice: inline in Postgres `support_files_json` **and** in the R2 tar. At the current 256 KiB cap either store alone could serve. R2's payoff is future-facing — large artifacts out of the transactional database, global content-dedup, edge delivery. The open call is whether to **(A)** make R2 the single content store and shrink Postgres to metadata-only, or **(B)** drop R2 until bundles outgrow Postgres. Either resolves the dual-write; the status quo (both) is the waste. Tracked, not yet decided.

## Cardinality + dedup

- **R2: one object per unique content, globally.** Identical bytes from any source dedup to one `fleet-bundles/sha256/{hash}.tar`.
- **Postgres `core.fleet_bundles`: one row per `(workspace_id, content_hash)`.** Same content in two workspaces = two rows, one R2 object.
- **`core.fleets`: one row per fleet.** Many fleets may reference one bundle.

## Runtime read path

At lease time (see [`data_flow.md` §C](./data_flow.md)):

- **SKILL.md / TRIGGER.md** → from the **lease** (`instructions`/`policy`, resolved from `core.fleets` fresh per lease, so they reflect any PATCH). The runner **ignores** the SKILL.md/TRIGGER.md copies inside the tar.
- **Support files** → the runner downloads the tar via `GET /v1/runners/me/bundles/{content_hash}` (daemon proxies `r2.get`; cached at `.bundle-cache/{hash}.tar`), and untars the support files into the per-lease sandbox workspace **before** the child forks. `SKILL.md` can then reference them (`review.sh`, playbooks, …).

## Update + sync

- **SKILL.md / TRIGGER.md are editable** via `PATCH /v1/workspaces/{ws}/fleets/{id}` (`source_markdown` / `trigger_markdown`). The edit is **in place on `core.fleets`** (reparse, validate the name still matches, bump revision). It does **not** mint a new bundle and does **not** change `bundle_id`.
- **Bundles are immutable** — no PATCH/PUT, only import (POST) + read (GET).
- **Support files are NOT editable in place.** They live only in the immutable bundle, so changing one requires a **re-import** (new `content_hash` → new bundle → re-point the fleet). No fleet-level support-file override today. 🟡 gap.
- **No GitHub → bundle sync.** Import is one-time; pushing a new commit to the source repo does nothing. Re-sync is a manual re-import. 🟡 gap.

## Notable invariants

- **The stored tar is agentsfleet's canonical re-pack, never GitHub's archive.** Safe-by-construction so the runner untars without re-validating.
- **The runner's behaviour comes from the fleet's live SKILL.md, not the bundle's.** A PATCH takes effect on the next lease; the tar's import-time copies are inert.
- **Secrets never enter R2 or the bundle.** Credentials are vault refs (`fleet:<source>`), resolved at lease and delivered inline; the tar carries only author-authored files.
- **Content-addressing makes import idempotent.** Re-importing identical bytes reuses the same R2 object and (per workspace) the same bundle row.
