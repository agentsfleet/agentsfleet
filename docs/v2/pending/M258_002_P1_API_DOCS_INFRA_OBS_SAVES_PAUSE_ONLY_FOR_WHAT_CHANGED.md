<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M258_002: A save pauses a fleet only for what changed, and a restore waits only for the bytes it reads

**Prototype:** v2.0.0

**Milestone:** M258

**Workstream:** 002

**Date:** Nov 26, 2026

**Status:** PENDING

**Priority:** P1, every save stops a fleet's tools while the runner reads, uploads and verifies its whole workspace

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B2, after M258_001 merges; M258_003 waits for it

**Depends on:** M258_001, for the `afr_engine` seam, files through the executor, the `Snapshots` interface and trials, and the Firecracker inside-out guest

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 26, 2026) from Indy's rulings and the research in `~/.local/state/agentsfleet-m254/` (`vm-engines/object-store-workspace.md`, `vm-engines/fly-sprites-design-post.md`, `vm-engines/design.md`, `vm-engines/sprites-cloud.md`, `space/trait-comparison.md`). Each repository citation was re-read at `632ce4efa`.

**Canonical architecture:** `docs/architecture/checkpoints.md` §How it behaves and §Limits; this spec rewrites its save, publication and restore flows

---

## Overview

**Goal (testable):** A save pauses only for its entries and changed bytes, and a lazy restore starts a run before its last chunk arrives.

**The compute layer.** M258 is the compute layer, ruled the first milestone so bubblewrap is sunset first. M258_001 builds the engine seam and our Firecracker engine, and M258_003 adds Sprites and the engine policy and deletes bubblewrap. This spec rebuilds checkpoint and snapshot between them; the three-Workstream packing is O1.

**Terms.**

- R2 is Cloudflare's object store, where checkpoint chunks live. A generation is one published checkpoint: manifest pages and a current head in Postgres, encrypted chunks in R2.
- The pause is the window in which tenant code is stopped for a save. The pin is the instant that fixes what the save reads.
- A userspace block device (ublk) is a Linux block device whose reads and writes a process of ours answers. Rust paths that start with a crate name are under `rustd/crates/<layer>/`, at `632ce4efa`.

**Problem.** Read from source.

- **The pause spans the whole save.** The boundary owner freezes the sandbox (`afr_supervisor/src/run_loop/recovery_capture.rs:75-79`) and thaws it only at `:89-97`. Between them it scans, hashes, registers, seals, uploads, reads back, writes pages and commits, as `recovery/turn/capture.rs:1` states.
- **Every save reads every byte.** The catalog reads and hashes each file in 8 MiB chunks on every save (`recovery/catalog.rs:168`; `recovery/files.rs:14`). It registers unchanged chunks too, 256 per request (`af_wire` `CHECKPOINT_BATCH`).
- **Turn manifests relist everything.** A turn save passes no base (`recovery/turn/capture.rs:210-215`), so its pages list every entry every time. A task increment lists every entry too, adding base paths only as `Remove` (`recovery/catalog.rs:122-128`).
- **A restore reads everything first.** Every chunk of every file is fetched, four per authorization (`recovery/session.rs:176`), before the first model call (`run_loop/turn_recovery.rs:26-62`).

```text
pause today ≈ walk(E) + read and SHA-256(B) + ⌈C/256⌉·RTT + N·(seal + PUT + daemon GET + open)/4 + P·RTT + commit
E entries · B bytes · C chunks · N new chunks · P manifest pages · RTT one runner-to-daemon round trip
```

**Solution summary.** Stage 1 splits a save at the pin. Under the pause the executor walks file identities and copies only changed bytes to host staging, then tenant code resumes. Registration, encryption, upload, read-back, delta pages and commit run after resume, one publication per owner.

Stages 2 and 3 put the workspace on a host ublk device whose origin is a block-level chunk map. Stage 2 restores lazily, and stage 3 uploads dirty chunks continuously so the host disk is only a cache. Each waits for its spike and Indy's rulings.

Our Firecracker engine serves M258_001's `Snapshots` on this chunk map. A restore swaps the workspace under the inner environment without rebooting the virtual machine (VM).

**Design flow:**

```text
boundary
  pause tenant code     bubblewrap: the tenant leaf alone · Firecracker: the inner environment (D22)
  quiesce               no live tenant process, or the save skips
  pin   stage 1         statx walk of E entries; changed bytes ΔB copied to root-only host staging
        stages 2-3      guest flush (FIFREEZE or syncfs); one generation switch in the block server
  resume                tenant code and the next model call run on
background, one publication per owner
  changes(pin) ─► register ΔC ─► seal, one daemon data key per object ─► PUT, If-None-Match: *
               ─► daemon read-back ─► delta pages, parent = last committed ─► commit head ─► release(pin)
restore
  stage 1               resolve the chain ─► fetch every chunk ─► import ─► inner environment up
  stage 2               map only ─► inner environment up ─► a read miss fetches one checked chunk
```

| | Still paused | After resume |
|---|---|---|
| Today | everything in the formula above | nothing |
| Stage 1 | stat walk of E entries, copy of ΔB | register, seal, PUT, read-back, pages, commit |
| Stages 2 and 3 | guest flush of its dirty page cache, one generation switch | the same, reading changed chunks from the pinned generation |

**Litestream and SQLite: not adopted.** Fly kept its map in SQLite made durable by Litestream, beside a checkpoint database, then replaced both. Fly gave its reason in the block-device post: "Each one was made durable separately, so each one could be right while the others were wrong."

Litestream signs in with a bucket key on its host, and the report found no signed-link mode. The map would also become a durable fact outside Postgres. Our map stays one ledger in Postgres, with a runner cache that a restore never trusts (D16).

**JuiceFS: rejected.** Wherever it runs, its client holds the bucket keys and the metadata database's credentials beside tenant code. On Postgres metadata, a 10,000-file checkout would spend at least 8.4 s in creates, by arithmetic on JuiceFS's published benchmark.

Fly retired that stack after a harness drove 400 test Sprites through about 5,000 cycles. In Fly's words, "A nonzero number of them permanently lost the ability to checkpoint."

**Do encryption and the stop-the-world pause go away?** Encryption stays, with a fresh daemon-held key per object, and runs after resume, off the pause. The pause shrinks to a walk and a copy in stage 1, and to a guest flush and a pin in stages 2 and 3.

The pause never reaches zero. A save needs tenant code stopped and the guest's dirty pages on disk, or a file the tenant believed written can vanish.

**What stays exactly as today, for safety.**

1. Chunks are sealed with AES-256-GCM (Advanced Encryption Standard, Galois/Counter Mode) under a fresh data key per object, which the daemon wraps (`docs/architecture/checkpoints.md:115-117`).
2. Objects are written once, because every upload link binds `If-None-Match: *` (`afd_fleet/src/durable_task/storage/transfer.rs:116-123`).
3. A generation becomes current only after every object verifies through the daemon's read-back (`checkpoints.md:80`; `afd_fleet/src/durable_task/checkpoint/transfer/verify.rs:76-81`).
4. No runner or sandbox holds a bucket token, a listing right or a delete right (`checkpoints.md:109`).
5. Manifest pages, object rows and the current generation live in Postgres (`afd_db/schema/20261122090200_runner_checkpoints.sql`, `…090300_runner_checkpoint_pages.sql`, `…090500_runner_checkpoint_objects.sql`).

**What is unverified.**

- The racy-clean guard's exact form, and whether a write through a shared mapping always moves a file's change time. The tests in §2 settle both.
- Whether ublk serves a Firecracker drive on the dev host, what `FIFREEZE` costs in our guest, and whether Debian 13 ships `dm-clone`. The spike in §4 settles them.
- Fly's figures, about 300 ms per checkpoint and restores "in well under a second", are vendor-stated.

**What changes for a fleet.** Each line is a behaviour Indy accepts by approving this spec.

1. A save's pause no longer grows with the workspace's size. A fleet's tools stop for a walk of its files and a copy of what changed.
2. The run continues before its save commits. A crash in between falls back to the previous checkpoint, as a skipped save does today.
3. After §4, a restored run on Firecracker starts before every chunk is fetched, and a read of a missing chunk waits for one fetch.
4. After §4, `/tmp` is a separate disk that no save holds, if Indy rules D19 as recommended.
5. After §5, a held workspace keeps nothing on its host.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): saves pause only for what changed; restores fetch only what a run reads
- **Intent (one sentence):** A save stops a fleet's tools only while changed bytes are copied, and a restored run starts before its workspace arrives.
- **Authoring handshake:** Indy wants the Sprites design applied to our checkpoint and restore "to make them faster", and asked about Litestream, SQLite and JuiceFS. He asked whether encryption and the stop-the-world pause go away, and ruled that the compute layer fixes checkpoint and snapshot first. ASSUMPTIONS I'M MAKING:
  1. O1: the compute layer packs into three Workstreams, and this one is checkpoint and snapshot. The packing is the coordinator's proposal, not Indy's ruling.
  2. M258_001 lands first. This spec builds on its executor `import` and `export`, its `Snapshots` interface and trials, and its inside-out guest. It changes none of them except as D22 asks.
  3. D16 to D21 hold as the report recommends until Indy rules. §1 to §3 need none of them; §4 and §5 wait for them.
  4. Nothing is backward compatible. The manifest format, `runner.checkpoints` and the runner protocol edit in place, with one `VERSION` move.
  5. One Pull Request, one commit per Section. §4 and §5 join it only after their spikes pass and their decisions are ruled.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/runner/afr_supervisor/src/run_loop/recovery_capture.rs` — the freeze that spans the whole save today; §1 ends it at the pin.
2. `rustd/crates/runner/afr_supervisor/src/recovery/catalog.rs` — the scan and hash that runs frozen; §2 replaces it with a walk of identities.
3. `rustd/crates/runner/afr_supervisor/src/recovery/restore_manifest.rs` — the overlay that applies `Remove` entries over a base; turns gain the same chain.
4. `docs/architecture/checkpoints.md` — the publication, encryption and transfer rules this spec keeps.
5. https://fly.io/blog/sprites-block-device.md — one ledger, a checkpoint as a volume version, and a restore that unmounts, rewinds and mounts.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M258_002_P1_API_DOCS_INFRA_OBS_SAVES_PAUSE_ONLY_FOR_WHAT_CHANGED.md` | CREATE | this spec; moves to `active/`, then `done/` |
| `docs/v2/reviews/m258-block-layer-spike.md`, `docs/v2/reviews/m258-writeback-spike.md` | CREATE | §4's and §5's spike records; helpers stay outside the repository |
| `rustd/crates/runner/afr_engine/**`, created by M258_001 | EDIT | the storage concern gains `WorkspaceVolume`, `Origin`, `ChunkSource`, `ChunkStore` and `Change`; two snapshot trials; the pin pause (D22) |
| `rustd/crates/runner/{afr_engine_bubblewrap,afr_cgroup}/**`, created by M258_001 | EDIT | §1: the pin pause freezes the tenant leaf alone (D22) |
| `rustd/crates/runner/afr_executor/**` | EDIT | §1: the inner environment's pin pause on Firecracker; §2: the identity walk, its cache and staging; §4: the guest flush |
| `rustd/crates/runner/afr_supervisor/src/recovery/**`, `afr_supervisor/src/run_loop/{recovery_capture,turn_recovery}.rs` | EDIT | §1: the pause ends at the pin, publication after resume, one per owner, staging swept at boot; §2: delta chains and the full-manifest rule |
| `rustd/crates/runner/afr_engine_firecracker/**`, created by M258_001 | EDIT | §3: `Snapshots` on the chunk map; §4: the ublk drive, a rewind in place, boot refusing a host without `ublk_drv` |
| `rustd/crates/runner/afr_volume/**` | CREATE | §4, §5: the ublk server on `libublk`, its chunk cache, fetch, dirty bitmap and writeback; the name is a proposal |
| `rustd/crates/protocol/af_wire/src/durable_task/checkpoint/{mod.rs,manifest.rs,plan.rs}`, `af_wire/src/run/turn_checkpoint/**`, `af_wire/src/protocol/mod.rs` | EDIT | a `parent` on both plans; a block manifest under the next format value; `VERSION` moves once |
| `rustd/crates/daemon/afd_db/schema/20261122090200_runner_checkpoints.sql` | EDIT | `parent_checkpoint_id`, edited in place before production |
| `rustd/crates/daemon/afd_fleet/src/run/turn_checkpoint/**`, `afd_fleet/src/durable_task/checkpoint/**` | EDIT | §2: commit records the parent, purge skips ancestors; §4: single-chunk authorization and pin renewal; §5: writeback registration |
| `rustd/crates/daemon/main/src/**` | EDIT | §5: the production caller of checkpoint maintenance, which nothing calls today (`checkpoints.md:165`) |
| `rustd/crates/daemon/afd_fleet/tests/{integration_turn_checkpoints,integration_durable_tasks}/**` | EDIT | chains, ancestors, pins and collection against live Postgres |
| `rustd/crates/e2e/af_e2e/tests/recovery_process/**`, `af_e2e/benches/recovery/**` | EDIT | §2's killed-runner chain restore; the bench's large-workspace, small-change shape |
| `rustd/crates/runner/main/examples/kernel_lane/**` | EDIT | the kernel trials of §1, §2, §3 and §4 |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | `libublk` for §4 |
| `public/openapi.json` | EDIT | regenerated with `--no-banner` for the task plan's `parent` |
| `deploy/baremetal/agentsfleet-runner.service`, `playbooks/lib/runner/prepare.sh` | EDIT | §4: `ublk_drv` loads beside `erofs` (`agentsfleet-runner.service:23`) |
| `docs/architecture/{checkpoints,runner_execution}.md` | EDIT | the pause, the chain, the block layer, and what never crosses engines |
| `~/Projects/docs` `changelog.mdx` | EDIT, on that repository's own branch after merge | saves pause only for changed bytes |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR and RS-SOURCE for `afr_volume`'s error through `error_shell!`; RS-UNSAFE for the ublk buffers; RS-RACE for one publication per owner.
- **`domains/sql.md`** — SQL-FILE, STS, SQL-FKNAME for `parent_checkpoint_id`, SQL-FKSTATE for the ancestor refusal, TXN, SQL-ONEFACT.
- **`tests.md`** — TEST-NEG, TEST-INJECT, TEST-PARTIAL for each ordered step of publication, TEST-RESIDUAL, TEST-KILL, TEST-REAL, TEST-LANE, TEST-SKIP, TEST-DELTA.
- **`code.md`** — NLG for the manifest edited in place, ORP, OWN for each pin, staging file and device, TIM for pin lifetimes, ECL, FLS.
- **`domains/logging.md`** — LOG-EVENT, LOG-SECRET. **`domains/http-surface.md`** — HTTP-OPENAPI. **`domains/architecture.md`** — ARCH-WINS, ARCH-LAND.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` with `cargo shear` | yes | `libublk` enters only `afr_volume`; moved code drops what it leaves behind |
| `make test-unit-all` | yes | `afr_volume` sits under `crates/runner/`, which both shard globs select |
| `make test-integration-rustd` | yes | chains, ancestors, pins and collection run against live Postgres |
| `af_e2e/tests/layers.rs` | yes | `afr_volume` is `afr_` in `runner/` and names no datastore client (`layers.rs:3`) |
| `make test-runner-kernel ENGINE=firecracker` on a host with `/dev/kvm` | not in a green Pull Request | runs before the Pull Request opens; counts go in the Session Notes |
| OpenAPI drift | yes | regenerate with `--no-banner` |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | `afr_volume` splits into device, cache, fetch, bitmap and writeback modules |

## Prior-Art / Reference Implementations

- **Reference:** today's task overlay (`recovery/restore_manifest.rs:115-134`) — turns gain the same parent chain, with `Remove` entries applied over a base.
- **Reference:** Fly's block device (https://fly.io/blog/sprites-block-device.md, L50-64) — one ledger, a checkpoint as a version, a rewind in place, and a check on every block read back. Divergence: the map lives in Postgres, and the server runs on the host, so the guest never reaches the bucket.
- **Reference:** `libublk` 0.4.8 (https://github.com/ublk-org/libublk-rs, MIT OR Apache-2.0), its `loop.rs` and `ramdisk.rs` examples — the shape of `afr_volume`; no block protocol is hand-written.
- **Reference:** git's index — the identity tuple and racy-clean rule behind §2's cache, which `recovery/files.rs:16-36` already computes.
- **CLI practices:** N/A; no command-line change.

## Sections (implementation slices)

Build order: stage 1 is §1 and §2, which need no new block layer. Snapshots in §3 build on them. Stages 2 and 3 are §4 and §5; each opens with a spike that commits only its record and waits for Indy's rulings.

### §1 — The pause ends at the pin, and publication runs after it

Today `BoundaryOwner::capture` holds the freeze through every step of the save (`recovery_capture.rs:75-97`). This Section splits a save at the pin, with today's full walk behind it, so §2 changes only what the walk reads.

**The volume.** M258_001's storage concern gains `WorkspaceVolume`: `pin(parent)`, `changes(pin)` and `release(pin, committed)`. The caller has paused tenant code before `pin`, which returns once no later write can change what it reads. The supervisor's pipeline consumes `changes` unchanged across all three stages.

**The pause.** A pin needs tenant code stopped while the executor still answers, ended without a reboot. No engine has one today, because bubblewrap's freezer stops "both leaves at once", the executor's included (`afr_sandbox/src/cgroup/mod.rs:208-211`). The pin pause freezes bubblewrap's tenant leaf alone and Firecracker's inner environment, which M258_001's seam does not name (D22).

**Publication.** After resume the pipeline registers, seals, uploads, reads back, writes pages and commits. The boundary's receipt arrives after resume, so the next model call does not wait. A boundary that finds its owner's publication running skips, as an optional save skips a busy slot (`checkpoints.md:36`).

- **State authority:** the owner's head, `thread.turns.head_checkpoint_id` or `fleet.durable_tasks.checkpoint_id`.
- **Guarded transition:** a commit advances the head only from the `expected_head` its plan named (`recovery/turn/capture.rs:234-237`).
- **Commit before release:** `release(pin, committed: true)` runs only after the commit answers, and the next pin diffs against that generation.
- **Failure assumption:** an uncertain commit is reconciled against its operation and generation, as today (`checkpoints.md:80`).

**Staging.** Changed bytes wait in one staging file per pin, in the runner's state directory, readable by the runner's user alone. It holds plaintext at the image's own trust, "every file the run wrote" (`afr_sandbox/src/workspace_disk/mod.rs:123`). Boot removes every staging file, and today's per-save upload bounds cap its size (`checkpoints.md:134`).

- **Dimension 1.1** — the pause ends before any daemon call → Test `a_save_resumes_before_it_registers_or_uploads` (unit, `afr_supervisor`, a fake daemon and a recording volume). The log reads pause, quiesce, pin, resume, then register, seal, PUT, read-back, pages and commit. A register sent before resume, even from a spawned task, fails it.
- **Dimension 1.2** — the run does not wait for publication → Test `the_next_model_call_does_not_wait_for_publication` (unit, the agent loop over a fake supervisor). The fake daemon holds every PUT. The next model request is sent while the upload is held, and the receipt arrives after it.
- **Dimension 1.3** — a failed publication keeps the previous generation → Test `a_publication_failing_after_resume_keeps_the_previous_generation` (integration, live Postgres). The store fails the third PUT; the head still names the previous generation, the staging file is gone, and `checkpoint_publication_failed` is logged. The next save's delta names that previous generation as its parent.
- **Dimension 1.4** — one publication per owner → Test `a_boundary_finding_a_publication_running_skips` (unit, contention). With the first save's upload held, 50 boundaries released from a barrier each skip as busy and none pauses. Exactly one generation commits.
- **Dimension 1.5** — a runner killed after resume loses nothing committed → Test `a_runner_killed_between_resume_and_commit_keeps_the_previous_generation` (integration, live Postgres). `SIGKILL` lands after resume and before commit, and the head names the previous generation. The next boot leaves no staging file, and a replacement restores that generation byte for byte.
- **Dimension 1.6** — the pause stops tenant code and keeps the executor → Test `the_pin_pause_stops_tenant_code_and_keeps_the_executor_answering` (integration, kernel lane, bubblewrap and Firecracker). A tenant loop's counter holds still through the pause while `list_dir` answers, and advances after resume. A live tenant process skips the save as `BusyProcesses` with no pin taken, and the VM monitor's process identifier is unchanged.

### §2 — Stage 1: a save reads only what changed

**Identity cache.** The executor keeps each path's identity at the last committed pin: device, inode, size, and nanosecond modification and change times (`recovery/files.rs:16-36`). The cache is a hint that a restore never trusts. A missing cache, on a new host or after a restart, costs one full hash.

**Under the pause.** The executor walks the tree with `statx` through directory descriptors, in place of an open and a stat per file. An unchanged identity is skipped unless its change time falls at or after the previous pin's clock tick, and that racy-clean guard re-reads it. Changed files are read, hashed with the Secure Hash Algorithm (SHA-256) and streamed to staging, over vsock on Firecracker.

**After resume.** Only changed chunks are registered. The manifest is a delta against the parent generation, holding changed entries and `Remove`, which the task overlay already applies (`recovery/restore_manifest.rs:115-134`). Turns gain the same chain.

**The full-manifest rule.** A save writes a full manifest, with no parent, once the chain's delta pages, summed, would outnumber a full manifest's. The threshold comes from the data, so it is no invented limit. A cacheless save also writes a full manifest, and its unchanged chunks register as `Reused` and upload nothing.

**Ancestors.** **Implementation default:** `runner.checkpoints` gains `parent_checkpoint_id`, a foreign key to `runner.checkpoints` with no cascade, edited in place before production. Postgres then refuses to delete an ancestor that a delta names, and purge reads that refusal with `afd_db::constraint::violates_foreign_key`. A chain a full manifest has superseded is purged newest first.

- **Dimension 2.1** — untouched files are neither read nor registered → Test `a_save_reads_and_registers_only_the_changed_files` (unit, `afr_executor` walker and `afr_supervisor` catalog, a counting reader). With 1,000 files and 3 rewritten, bytes read under the pause equal those 3 files' bytes, and registrations name only their chunks. A walker that reads everything but registers 3 fails it, and the restore equals the source tree.
- **Dimension 2.2** — the pause does work in entries and changed bytes only → Test `a_pause_does_work_in_entries_and_changed_bytes_only` (unit, counters, sparse files). Workspaces of 1,000 entries, with 100 MiB or 64 GiB of untouched bytes and the same 3 changes, do equal work under the pause. Each counts 1,000 stats, the changed bytes, zero daemon calls and zero seals; doubling changes doubles bytes, and doubling entries doubles stats.
- **Dimension 2.3** — a change the stat cannot see is still saved → Test `a_same_tick_same_size_rewrite_is_saved` (unit). A file is rewritten within the previous pin's clock tick at the same size, its modification time set back. The change-time guard re-reads it, the restore holds the new bytes, and the test fixes the guard's form.
- **Dimension 2.4** — a write through a mapping is saved → Test `a_shared_mapping_write_is_saved_by_the_next_save` (integration, kernel lane, both engines). A tenant writes through `MAP_SHARED` to a page it wrote before the previous pin, and the next restore holds the new bytes. Whether that write moves the change time is unverified `[?]`, and a red blocks §2 until Indy rules.
- **Dimension 2.5** — a lost cache costs one full hash → Test `a_save_without_its_cache_reads_everything_once` (unit). With no cache, every file is read once and a full manifest is written. It resolves to the tree a warm-cache chain resolves to, and its unchanged chunks upload nothing.
- **Dimension 2.6** — a save writes only what changed → Test `a_turn_save_writes_only_changed_and_removed_entries` (unit, `afr_supervisor`). After a full base of 1,000 entries, a save that changed 2 files and deleted 1 writes 2 file entries and 1 `Remove`. Its `parent` names the base, and resolving the chain equals a full scan's manifest.
- **Dimension 2.7** — a chain never outgrows a full manifest → Test `a_chain_whose_deltas_outgrow_a_full_manifest_writes_a_full_one` (unit). With a 3-page full manifest, a chain reaching 4 summed delta pages makes the next save full, with no parent. A chain at 3 pages stays a delta.
- **Dimension 2.8** — an ancestor outlives every delta that names it → Test `an_ancestor_of_a_ready_delta_is_never_purged` (integration, live Postgres). Deleting a parent that a ready delta names fails on `parent_checkpoint_id`'s constraint, and purge skips it. Once a full manifest supersedes the chain and it settles, purge removes the chain newest first.
- **Dimension 2.9** — a delta chain moves hosts → Test `a_killed_runner_restores_a_delta_chain_on_another_host` (e2e, `af_e2e`, mirroring `killed_runner_mid_turn_restarts_on_another_host`). A source runner saves a full manifest and three deltas through the real routes, frames and R2, then is killed. The replacement restores byte for byte, and its first model call follows the restore.
- **Dimension 2.10** — the measured pause stays flat as the workspace grows → Test `make bench-recovery` with the large-workspace, small-change shape (manual). Aiwa runs it for 100 MiB and 10 GiB of untouched files with one changed 1 MiB file. The larger's median pause falls within the spread of ten runs of the smaller, and the numbers go in the Session Notes.

### §3 — Snapshots on our engine, and what never crosses engines

**Our engine's snapshots.** `afr_engine_firecracker` declares `snapshots: chunk_map` and serves `Snapshots` on the volume and a `ChunkStore` the supervisor hands it at `prepare`. `take(id)` pauses, pins, resumes and publishes one generation; its `SnapshotRef` names the generation, its format and the sandbox's environment. `forget` drops the ref, so maintenance may collect what nothing else names.

**Restore inside the running VM.** The executor ends the inner environment, and its init's exit ends every process in it. The workspace is rebuilt from the generation, through `import` in stage 1 and by rewinding the block device in stage 2. The executor builds the inner environment again, the run's egress plan reads back in force, and the VM never reboots.

**The production caller.** On an engine declaring `chunk_map`, the supervisor's recovery save is `take` and its restore is `restore`. An engine declaring `vendor` snapshots keeps recovery saves on the stage-1 volume through `export`, because a Fly checkpoint never leaves its Sprite. Bubblewrap declares none and uses the stage-1 volume until M258_003 deletes it.

**One behaviour.** Our engine passes M258_001 §6's four snapshot trials. This spec adds two trials for every engine that declares snapshots, M258_003's Sprites included.

The first holds that a snapshot never crosses environments, and the second that a restore never brings back an older egress plan. Policies ride in Fly's checkpoints (https://fly.io/sprites/checkpoint-restore.md, L86-88), so a Sprite can fail the second.

**What never crosses engines.**

| Saved state | Restores on | Never on, and why |
|---|---|---|
| Our generation, file format | bubblewrap and Firecracker with the same toolbox digest (`checkpoints.md:90`) | a Sprite: its root is Fly's, so the digest check refuses |
| Our generation, block format | Firecracker with the same toolbox digest | bubblewrap, which would mount guest-written ext4 in the host kernel (`afr_sandbox/src/toolbox/mod.rs:9-12` refuses that for any unhashed image); a Sprite, which cannot attach our volume |
| A Fly checkpoint | its own Sprite | any other Sprite or engine: no public call restores across Sprites, and its bytes stay in Fly's storage, outside our Postgres and R2 |
| Memory | nowhere | no engine snapshots memory here; Firecracker restores memory only on identical host software and hardware |

- **Dimension 3.1** — our engine passes every snapshot trial → Test `firecracker_passes_every_snapshot_trial` (integration, `make test-runner-kernel ENGINE=firecracker`). M258_001's four trials and this spec's two pass with the in-memory store, and no restore changes the VM monitor's process identifier. Bubblewrap's six trials assert `Unsupported`, and none reads skipped.
- **Dimension 3.2** — a snapshot never crosses environments → Test `a_snapshot_never_crosses_environments` (unit, `afr_engine` trial over fakes and our engine). A ref naming `Vendor { ubuntu, 26.04 }`, another toolbox digest or another engine is refused naming `environment`, before any chunk is read. A block-format ref offered to an engine that mounts on the host is refused the same way.
- **Dimension 3.3** — a restore never brings back an older plan → Test `a_restore_never_brings_back_an_older_plan` (unit trial over fakes; on Firecracker in 3.1's lane). A snapshot taken under plan generation 3 is restored while 4 is in force, and `in_force()` reads 4 before the executor answers. A fake whose restore brings back 3 fails the trial by name.
- **Dimension 3.4** — no key or link reaches the guest → Test `no_key_or_link_reaches_the_guest_across_take_and_restore` (integration, kernel lane, Firecracker). After a take and a restore, M258_001's planted-token search covers the guest's files, environment and `/proc`. It finds no data key, nonce or signed URL, and finds a canary planted on purpose.

### §4 — Stage 2: lazy restore behind a host ublk device

Stage 2 makes a restore wait only for the chunks a run reads. **Gate:** Dimension 4.1 passes, and Indy rules D16 to D19.

**Mechanism.** `afr_volume` serves the workspace as a host device, `/dev/ublkbN`, attached as Firecracker's data drive, with a block-level chunk map as its origin. A read miss fetches one object through a daemon-signed link and checks its ciphertext digest, tag and plaintext identity. Writes land in a local sparse file.

**Block capture.** The server's dirty bitmap names the changed chunks. A pin is the executor's guest flush, `FIFREEZE` or `syncfs`, then one generation switch in the server. The pause then grows with the guest's dirty page cache alone.

**Restore in place.** The executor ends the inner environment and unmounts the workspace. The server rewinds the device to the target generation, dropping local writes, and the executor mounts it again (Fly's block-device post, L62).

**Must be true first.**

- The manifest gains a block variant under the next `CHECKPOINT_FORMAT` and `TURN_CHECKPOINT_FORMAT` values (`af_wire/src/durable_task/checkpoint/mod.rs:19`; `af_wire/src/run/turn_checkpoint/mod.rs:30`).
- The restore pin outlives the fill. Today a pin lasts 2 minutes for turns and 30 for tasks, and a link 5 (`checkpoints.md:111,138`). The pin therefore renews while its generation is an origin.
- An authorization names any single chunk on demand, where a restore batch holds at most four today (`recovery/session.rs:176`). `ublk_drv` loads on every runner host, and Firecracker's boot refuses a host without it.

**Implementation default:** `afr_volume` builds on `libublk` 0.4.8 and its `loop.rs` example, because a maintained crate covers the device protocol. Bubblewrap never attaches the device and refuses block generations (§3).

**The spike's record.** `docs/v2/reviews/m258-block-layer-spike.md` states what `FIFREEZE` costs and the write amplification at three chunk sizes, for D18. It also states whether a rewind needs the guest's block cache dropped, and what `modinfo dm_clone` prints.

- **Dimension 4.1** — spike: ublk serves a Firecracker drive → Test `a_ublk_device_serves_a_firecracker_drive_on_the_dev_host` (manual). Aiwa attaches a sparse file, served by a `libublk` example over `ublk_drv`, as a data drive; the guest formats, writes, syncs and runs `FIFREEZE`. The server is killed and restarted with `UBLK_F_USER_RECOVERY`, the synced file reads back, and the record lands.
- **Dimension 4.2** — a run starts before its last chunk arrives → Test `the_first_model_call_starts_before_every_chunk_is_fetched` (integration, kernel lane, Firecracker). A run reading 10 MiB of a restored 4 GiB generation sends its first model request while fetched chunks are fewer than total. GET requests equal the distinct chunks the device served, so a restore that fetches everything fails.
- **Dimension 4.3** — a tampered chunk serves no byte → Test `a_tampered_chunk_fails_its_read_and_serves_no_byte` (unit, `afr_volume`, an in-memory source). One flipped ciphertext byte makes the read answer an input and output (I/O) error, and no plaintext reaches the device. `checkpoint_chunk_refused` is logged, and the run ends closed.
- **Dimension 4.4** — a missing or killed block layer never serves a run → Test `a_block_layer_that_is_missing_or_killed_ends_closed` (integration, kernel lane, Firecracker). Without `ublk_drv`, boot refuses naming `ublk`; after `SIGKILL` mid-write, a restarted server re-attaches and the guest's synced file reads back. When re-attach fails, the run ends `sandbox_unavailable` and publishes nothing from that device.
- **Dimension 4.5** — a block save names only dirty chunks → Test `a_block_save_publishes_only_the_dirty_chunks` (unit, `afr_volume`). After a restore, writes to chunks 3 and 9 make `changes` yield exactly those two. The counters under the pause show the flush and the switch, and no chunk read.
- **Dimension 4.6** — a fill outliving its pin renews or ends closed → Test `a_lazy_restore_outliving_its_pin_renews_or_ends_closed` (integration, live Postgres). A fill past the pin's deadline renews it and keeps reading through single-chunk authorizations. When renewal is refused because the fence moved, reads stop, the run ends closed, and no stale byte is served.

### §5 — Stage 3: the workspace rooted in R2, the host disk a cache

Stage 3 makes R2 the workspace's home. **Gate:** §4 passes, Dimension 5.1 passes, Indy rules D16, D18 and D20, and checkpoint maintenance runs in production.

**Mechanism.** Between pins the server uploads dirty chunks in the background, each a new write-once object sealed under its own daemon-issued data key. Keys are issued 256 at a time (`af_wire` `CHECKPOINT_BATCH`). A pin publishes the map to Postgres once its chunks verify, so time to current grows only with chunks dirtied since the last upload.

**Backpressure.** When R2 is slower than writes, writes stall rather than outrun local space, and the host disk is the bound the operating system forces. A failed upload is retried as a new object, never an overwrite.

**Holds and loss.** A `Stop` hold whose last pin published drops the local cache, so a held workspace needs nothing on its host. A host death loses only writes after the last verified upload. For a turn, files newer than its conversation checkpoint are unused anyway (`checkpoints.md:84-99`).

**Collection.** Overwritten chunks become garbage that refs collect, which is Fly's log-structured disk lesson. No production code calls checkpoint maintenance today (`checkpoints.md:165`), so §5 adds that caller in the daemon. It collects only objects no generation or ancestor names.

- **Dimension 5.1** — spike: writeback's rate and stalls are measured → Test `a_writeback_spike_measures_upload_rate_and_stalls` (manual). On the dev host a fixture fleet clones and builds a repository under continuous writeback at D18's chunk size. `docs/v2/reviews/m258-writeback-spike.md` records PUTs and object rows per minute, publish lag, and what a writer sees under a throttled bucket.
- **Dimension 5.2** — every writeback is a new object → Test `dirty_chunks_upload_between_pins_as_new_objects` (integration, live Postgres). Writes between two pins become objects whose upload links all bind `If-None-Match: *`, each with its own wrapped key. A replayed upload to the same key is refused, and the bucket holds one copy.
- **Dimension 5.3** — no generation names an unverified chunk → Test `a_server_killed_at_each_ordered_step_never_publishes_an_unverified_chunk` (integration, fault injection). The server dies after a write, after registration, after the PUT, before read-back and before commit. No generation names an object without `verified_at`, and the current generation restores byte for byte.
- **Dimension 5.4** — a slow bucket stalls writes and fails none → Test `writes_stall_while_the_bucket_is_slower_than_the_cache_fills` (unit, `afr_volume`, a throttled store). With the cache at its disk bound, guest writes block, none fails, and no generation names a chunk not yet uploaded. Writes resume when the store drains, and `checkpoint_writeback_stalled` is logged once per stall.
- **Dimension 5.5** — maintenance collects garbage and keeps ancestors → Test `maintenance_collects_an_overwritten_chunk_once_nothing_names_it` (integration, live Postgres, through the daemon's production caller). An object superseded and named by no generation or ancestor is tombstoned. An object a ready delta's ancestor names is kept.
- **Dimension 5.6** — a workspace survives moves → Test `the_write_checkpoint_hold_move_restore_soak_verifies_every_byte` (integration, kernel lane, Firecracker, two runner processes). Two hundred cycles of write, checkpoint, hold, move to the other runner and restore run in turn. Every byte verifies each cycle, no generation is lost, and a held workspace leaves no cache file behind.

## Interfaces

```text
WorkspaceVolume (afr_engine, M258_001's storage concern)
  attachment() -> Directory | Image | BlockDevice
  pin(parent: Option<&Generation>) -> Pin       caller has paused tenant code; returns at the consistency point
  changes(&Pin) -> stream of Change              after resume: stage 1 file entries, stages 2-3 block chunks
  release(Pin, committed: bool)
Origin       Empty { bytes } | Generation { manifest, source: Arc<dyn ChunkSource>, fill: Eager | Lazy }
             a stage-1 volume answers Lazy by filling eagerly
ChunkSource  read(object) -> plaintext           supervisor, over signed links; checks digest, tag and identity
ChunkStore   ChunkSource + publish(changes)      supervisor in production; in memory behind test-util
Change       Entry(CheckpointEntry) | Chunk { key: File { path, index } | Block { index }, bytes }
BlockMap     { volume_bytes, chunk_bytes, chunks: index -> object }   stage 2 origin, published as manifest pages
Snapshots on Firecracker (M258_001's interface)
  take(id)     pause · pin · resume · publish
  restore(ref) inner environment down · origin swapped · inner environment up · plan read back
SnapshotRef  { id: Uuid7, kind: Disk, environment, generation, format: Files | Blocks }
Capabilities snapshots: none | chunk_map | vendor;  the pin pause per D22
Executor     pin_walk(parent) streams changed bytes (stage 1) · flush() is FIFREEZE or syncfs (stage 2)
Wire         both checkpoint plans gain parent: Option<checkpoint id>; a block manifest under the next format value
Schema       runner.checkpoints.parent_checkpoint_id -> runner.checkpoints (id), no cascade
```

The runner protocol `VERSION` (`af_wire/src/protocol/mod.rs:15`) moves once for the turn plan's `parent`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Publication fails after resume | failure | a PUT, read-back or commit fails | the head keeps the previous generation; staging removed; the next save diffs against it | `a_publication_failing_after_resume_keeps_the_previous_generation` |
| Publication still running | race | a boundary arrives mid-publication | the boundary skips as busy, with no pause | `a_boundary_finding_a_publication_running_skips` |
| Runner killed after resume | failure | `SIGKILL` between resume and commit | the previous generation stays current; boot removes staging | `a_runner_killed_between_resume_and_commit_keeps_the_previous_generation` |
| Tenant still running | failure | a live process at quiesce | the save skips as `BusyProcesses`; no pin | `the_pin_pause_stops_tenant_code_and_keeps_the_executor_answering` |
| Stat misses a change | failure | a same-tick, same-size rewrite | the change-time guard re-reads it | `a_same_tick_same_size_rewrite_is_saved` |
| Mapped write unseen | failure | a shared-mapping write to a dirty page | saved, or §2 blocks until Indy rules | `a_shared_mapping_write_is_saved_by_the_next_save` |
| Cache lost | failure | a new host or a restart | one full hash and a full manifest | `a_save_without_its_cache_reads_everything_once` |
| Ancestor purged | failure | maintenance or deletion under a delta | Postgres refuses; purge skips | `an_ancestor_of_a_ready_delta_is_never_purged` |
| Snapshot across environments | security | a ref from another engine, root or format | refused before any chunk is read | `a_snapshot_never_crosses_environments` |
| Plan rolled back | security | a snapshot taken under an older plan | the run's generation reads back before the executor answers | `a_restore_never_brings_back_an_older_plan` |
| Key or link in the guest | security | a leak through take or restore | the planted-token search fails the lane | `no_key_or_link_reaches_the_guest_across_take_and_restore` |
| Tampered chunk | security | ciphertext changed in R2 | an I/O error; no byte served; the run ends closed | `a_tampered_chunk_fails_its_read_and_serves_no_byte` |
| Block layer missing or killed | dependency | no `ublk_drv`, or the server dies | boot refuses; re-attach, or the run ends closed | `a_block_layer_that_is_missing_or_killed_ends_closed` |
| Pin expires mid-fill | timeout | a long fill, or a moved fence | the pin renews, or reads stop and the run ends closed | `a_lazy_restore_outliving_its_pin_renews_or_ends_closed` |
| Server killed mid-writeback | failure | a crash at any ordered step | no generation names an unverified chunk | `a_server_killed_at_each_ordered_step_never_publishes_an_unverified_chunk` |
| Bucket slower than writes | capacity | R2 throttles | writes stall; none fails | `writes_stall_while_the_bucket_is_slower_than_the_cache_fills` |
| Replayed upload | replay | a retried PUT to a used key | `If-None-Match: *` refuses it | `dirty_chunks_upload_between_pins_as_new_objects` |

## Invariants

1. No daemon call, seal or upload runs during the pause — the pipeline takes only the `Resumed<Pin>` that resume returns. Publishing under the pause cannot compile (Dimension 1.1).
2. One publication runs per owner — a one-permit semaphore per owner, taken with `try_acquire`, and a failed take skips the boundary (Dimension 1.4).
3. A generation is current only after every object verifies — the commit refuses any referenced object without `verified_at`, as today (`checkpoints.md:80`) (Dimension 5.3).
4. An ancestor of a ready delta cannot be deleted — `parent_checkpoint_id` is a foreign key with no cascade (Dimension 2.8).
5. Every object is written once — each upload link binds `If-None-Match: *`, and the runner refuses a link without it (Dimension 5.2).
6. A snapshot restores only into the environment it names — `restore` compares `SnapshotRef.environment` with the sandbox's before any read (Dimension 3.2).
7. No guest receives a key, a link or a bucket token — `ChunkStore` lives in the supervisor. No executor verb carries a key or a link (Dimension 3.4).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `checkpoint_paused` (log) | operator | resume after a pin | `owner_kind`, `engine`, `stage`, `entries`, `changed_bytes`, `paused_ms` | no path, content identity, key or link | `a_pause_does_work_in_entries_and_changed_bytes_only` |
| `checkpoint_published` (log) | operator | a background commit succeeds | `owner_kind`, `generation`, `delta`, `new_objects`, `uploaded_bytes`, `publish_ms` | as above | `a_save_resumes_before_it_registers_or_uploads` |
| `checkpoint_publication_failed` (log) | operator | publication ends without a commit | `owner_kind`, `step`, `error_code` | as above | `a_publication_failing_after_resume_keeps_the_previous_generation` |
| `checkpoint_chunk_refused` (log) | operator | a fetched chunk fails its check | `object_id`, `check` | no plaintext, key or link | `a_tampered_chunk_fails_its_read_and_serves_no_byte` |
| `checkpoint_writeback_stalled` (log) | operator | writes block on uploads | `engine`, `pending_chunks`, `stalled_ms` | host facts only | `writes_stall_while_the_bucket_is_slower_than_the_cache_fills` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every open owner decision is ruled (Discovery) | `git grep -n -E "^  - \*\*[A-Z][0-9]+\*\* .\[\?\]" -- "docs/v2/*/M258_002_*"` | no output | P0 | |
| R2 | Both spike records are committed (§4, §5) | `git ls-files docs/v2/reviews/m258-block-layer-spike.md docs/v2/reviews/m258-writeback-spike.md` | two lines | P0 | |
| R3 | A save resumes before publication, one per owner (§1) | `make test-unit-all` | exit 0; `a_save_resumes_before_it_registers_or_uploads`, `a_boundary_finding_a_publication_running_skips` pass | P0 | |
| R4 | The pause does work in entries and changed bytes only (§2) | `make test-unit-all` | exit 0; `a_pause_does_work_in_entries_and_changed_bytes_only` passes | P0 | |
| R5 | Failed and killed saves keep the previous generation, and chains keep ancestors and move hosts (§1, §2) | `make test-integration-rustd` | exit 0; `a_runner_killed_between_resume_and_commit_keeps_the_previous_generation ... ok`, `an_ancestor_of_a_ready_delta_is_never_purged ... ok`, `a_killed_runner_restores_a_delta_chain_on_another_host ... ok` | P0 | |
| R6 | Snapshots hold to one behaviour, never cross environments and never leak (§3) | `make test-runner-kernel ENGINE=firecracker` | exit 0; zero failed trials; `firecracker_passes_every_snapshot_trial` passed | P0 | |
| R7 | A lazy restore starts first and ends closed on a bad chunk (§4) | `make test-runner-kernel ENGINE=firecracker` | exit 0; `the_first_model_call_starts_before_every_chunk_is_fetched` passed | P0 | |
| R8 | Writeback never publishes an unverified chunk (§5) | `make test-integration-rustd` | exit 0; `a_server_killed_at_each_ordered_step_never_publishes_an_unverified_chunk ... ok` | P0 | |
| R9 | The freeze-through-publication claim is gone (§1) | `git grep -n -e "holds freeze until every scan" -- rustd docs/architecture` | no output | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the claim that the freeze spans publication | `git grep -n -e "holds freeze until every scan" -- rustd docs` | no output |

## Out of Scope

- The Sprites engine's snapshots on Fly's checkpoints, the engine policy and burst: M258_003.
- Memory snapshots, pause holds and pools of pre-booted VMs: not planned, as M258_001 records.
- A write-ahead journal for writes newer than the last boundary: not planned until a product promise needs them.
- Content-defined chunking with `fastcdc`, and `fanotify` change hints: not planned (Decomposition).
- Matching `rusqlite`, `libsqlite3-sys` and `redis` in `DATASTORE_CLIENTS` (`af_e2e/tests/layers/rules.rs:12`): its own change, which Indy schedules.
- A quota on a workspace's stored bytes: none exists (`checkpoints.md:166`), and none is invented here.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator reads `checkpoint_paused` lines whose `paused_ms` stays flat as a fleet's workspace grows from megabytes to gigabytes.
2. **Preserved user behaviour** — every checkpoint keeps today's encryption, write-once objects, verification and Postgres records. A fleet's configuration, tools and restore refusals stay as they are.
3. **Optimal-way check** — skip unchanged files and move every network step after resume. The gap is lazy restore, which needs a block layer.
4. **Rebuild-vs-iterate** — iterate on the publication pipeline for stage 1. Stages 2 and 3 rebuild the disk, each gated on its spike.
5. **What we build** — the pin seam, the identity cache, staging, delta chains, chunk-map snapshots, the ublk server and writeback.
6. **What we do NOT build** — JuiceFS, SQLite with Litestream, a journal, content-defined chunking, change hints, memory snapshots or a second ledger. Each fails a rule or buys little (Decomposition).
7. **Fit with existing features** — compounds with M258_001's executor files and inside-out guest. It must not destabilize verification before a generation becomes current.
8. **Surface order** — runner first, then the daemon's commit, refs and pins, then the architecture page; no command or page changes.
9. **Dashboard restraint** — the app shows nothing new. Operators read the five log events.
10. **Confused-user next step** — a refused restore names its reason, as today, and `docs/architecture/checkpoints.md` states the pause, the chain and what never crosses engines.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the pin and publication, then stage 1's walk, then snapshots on it, then stages 2 and 3, each behind its spike. Each Section leaves today's checkpoints working.
- **Alternatives considered:**
  - JuiceFS as the workspace: its client holds bucket keys and database credentials, and its metadata costs a database round trip per create.
  - SQLite with Litestream on the runner (the report's design B): a bucket key on every runner and a second ledger, the arrangement Fly removed.
  - A write-ahead journal through signed links: durability for files newer than the last boundary, which a restore cannot use.
  - Firecracker's vhost-user-blk (D17), and Network Block Device (NBD), whose Rust crate was last updated in March 2024.
  - `dm-era` or `dm-thin` deltas, which need the GNU General Public License (GPL) 3.0 tools or a subprocess in a core path.
  - `fanotify` hints, which lose events on overflow and miss `mmap` writes, and `fastcdc` chunking, which pays only for large files edited mid-file.
- **Patch-vs-refactor verdict:** a **refactor** because the save's shape, the manifest's chain and the disk's home all change.

## Discovery (consult log)

- > Owner (Indy, Nov 26, 2026): "Also i feel the checkpoint storage area on git is moving toward the code.storage area? do you think we abstract it as an sdk so anyone can call and use that?" and "just add to the spec for discovery or so when we do that" — context: recorded for later, not built here. The chunk store is git-shaped: encrypted chunks as blobs, manifests as trees, generations in Postgres as commits. Images in messages (M259_002) and browser screenshots (M259_001) need the same store. Recommendation: one crate with a clear API when this spec is built; a public SDK only when a customer needs it, since a public API brings versioning, auth, metering and docs. `[?]`: the crate boundary, and what "code.storage" refers to (unverified).
- > Owner (Indy, Nov 26, 2026): "also read this sprite design https://fly.io/blog/design-and-implementation/ so you know if you prefer to redesign anything", then "i am thinking we must do that" and "even in our checkpoint and restore to make them faster" — context: this spec applies the post's object-storage disk to our checkpoints, so checkpoint and restore move metadata.
- > Owner (Indy, Nov 26, 2026): "how about the litestream.io and the sqlite thing int he article" — context: asked, not ruled. The Overview answers it, and D16 carries the map's home.
- > Owner (Indy, Nov 26, 2026): "and juicefs too,  yes the chunk map .. to is an important post that mean the encrupt and stop the world will go away?" — context: the Overview answers it. JuiceFS is rejected, encryption stays and leaves the pause, and the pause shrinks to a flush and a pin.
- > Owner (Indy, Nov 26, 2026): "I prefer the milestone where ever we do the firecracker and sprites engine abstractions move (to me this is a compute layer) must  fix the checkpoint and snapshot, this must be the first milestone so we sunset bubblewrap and move on to firecracker and sprites" — context: ruled; M258 is the compute layer and fixes checkpoint and snapshot. Its packing into three Workstreams is O1.
- > Owner (Indy, Nov 26, 2026): "bubblewrap must be retired if we do 3rd party sprites based firecraker or any sandbox or firecracker engine our own." — context: block generations are built for Firecracker alone, and bubblewrap keeps file generations until M258_003 deletes it.
- **Open owner decisions.** Each stays open until Indy rules; each recommendation is Aiwa's.
  - **O1** `[?]` the compute layer's packing into three Workstreams, with this one checkpoint and snapshot. It is the coordinator's proposal after Indy's ruling. Recommend it; the report advised starting stage 1 on bubblewrap first, but M258_001 removes the host path that start would read.
  - **D16** `[?]` where the chunk map lives. Recommend design A: pages and generations in Postgres, and a runner-local cache only. It keeps every rule and one ledger, and the boundary is already the recovery unit (`checkpoints.md:84-99`).
  - **D17** `[?]` the block layer for stages 2 and 3; recommend ublk on the host. Firecracker's vhost-user-blk is developer preview, blocks Firecracker snapshots and slows page faults by up to 24% (Firecracker v1.18.0 `block-vhost-user.md:3-6,79-88,198-201`). The report's third reason, serving bubblewrap too, lapses once M258_003 deletes it.
  - **D18** `[?]` the block chunk size. Recommend measuring write amplification against request and row counts in Dimension 4.1. Today's 8 MiB is a buffer bound, not a forced limit (`af_wire/src/durable_task/checkpoint/mod.rs:20-21`).
  - **D19** `[?]` a separate disk for `/tmp`. At block level the disk holds `tmp/` too (`workspace_disk/mod.rs:7-11`), while today's checkpoint saves only `workspace/` (`recovery_capture.rs:121-127`). Recommend a separate scratch disk that no save holds, once §4 lands; it costs the shared disk's one limit.
  - **D20** `[?]` keys during continuous writeback. Recommend a fresh daemon-issued key per object, issued 256 per request. It keeps `checkpoints.md:115`, and the round trip is off the pause.
  - **D21** `[?]` the daemon's full read-back. Recommend keeping it, off the pause: it stops a runner that registered one identity and uploaded other bytes from poisoning later reuse (`verify.rs:76-81`). An R2 checksum would prove only the ciphertext, and whether R2 enforces one is unverified.
  - **D22** `[?]` the pin's pause. M258_001 maps `freeze` to `hold(mode)` and declares Firecracker `Stop` only, while bubblewrap's freezer also stops the executor that `export` needs (`cgroup/mod.rs:208-211`). Recommend amending M258_001's seam with a pin pause, declared apart from holds: bubblewrap's tenant leaf alone, and Firecracker's inner environment frozen by the executor.
- **Where the evidence and the sibling specs disagree.**
  - The report lets bubblewrap mount a block-level map (`object-store-workspace.md` §4e). The trait study and `afr_sandbox/src/toolbox/mod.rs:9-12` refuse to hand tenant-written ext4 to the host kernel, and this spec follows them.
  - The report keeps `freeze` and `thaw` (§4d), and its stage-1 bubblewrap variant reads the host path. M258_001 replaces both, so the walk runs in the executor on every engine.
  - The report's bubblewrap pause is today's freezer, which stops the executor too (`afr_sandbox/src/cgroup/mod.rs:208-211`). Once capture runs through `export`, that pause would stall its own walk, so D22 asks for a pause that spares the executor.
  - `checkpoints.md:164` says neither deployment sets `[checkpoint.storage]`, yet both do (`deploy/fly/agentsfleetd-prod/agentsfleetd.toml:132-135`). M258_001 corrects the page.
- **Consults** — the research reports named in Provenance. Fly's design post (L76-84) and block-device post (L22-64) are cited sources, not mechanisms to copy.
- **Model reviews** — none yet. The Elephant-Goldfish Model (EGM) review runs at `orly-spec` before the docs Pull Request.
- **Open blockers** — O1, D16 to D22, and the spikes in Dimensions 4.1 and 5.1.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
