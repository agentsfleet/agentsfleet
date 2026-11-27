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

**Date:** Nov 27, 2026

**Status:** IN_PROGRESS

**Priority:** P1, every save holds a fleet's run while the runner exports, uploads and the daemon reads back its whole workspace

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B1, in the one M258 Pull Request, built after M258_001 and M258_003 (O1, D22)

**Depends on:** M258_001, for the `afr_engine` seam, files through the executor, the `Snapshots` interface and trials, and the inner environment; M258_003, which deleted bubblewrap and runs production on Firecracker

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 26, 2026) from Indy's rulings and the research in `~/.local/state/agentsfleet-m254/` (`vm-engines/object-store-workspace.md`, `vm-engines/fly-sprites-design-post.md`, `vm-engines/design.md`, `vm-engines/sprites-cloud.md`, `space/trait-comparison.md`). Each repository citation was re-read at `632ce4efa`. Rewritten by Aiwa on Nov 27, 2026 to the recorded rulings, with every citation re-read at `b5c0f99c7`.

**Canonical architecture:** `docs/architecture/checkpoints.md` §How it behaves and §Limits; this spec rewrites its keys, save, publication and restore flows

---

## Overview

**Goal (testable):** A save pauses only for its entries and changed bytes, and a lazy restore starts a run before its last chunk arrives.

**The compute layer.** M258 is the compute layer, in one Pull Request built M258_001, M258_003, M258_002 (O1). M258_001 built the seam and our Firecracker engine, and M258_003 deleted bubblewrap, so Firecracker is the one engine here. The Sprites engine moved to M260_001.

**Terms.**
- R2 is Cloudflare's object store, where checkpoint chunks live. A generation is one published checkpoint: manifest pages and a current head in Postgres, encrypted chunks in R2.
- The pause is the window in which a save holds the run: no model call or tool runs, and from §2 tenant code is frozen. The pin is the instant that fixes what the save reads.
- A userspace block device (ublk) is a Linux block device whose reads and writes a process of ours answers. Rust paths that start with a crate name are under `rustd/crates/<layer>/`, at `b5c0f99c7`.

**Problem.** Read from source.
- **The run waits for the whole save.** The agent loop awaits the boundary's save (`afr_agent/src/loop/recovery_boundary.rs:215`). The boundary exports the workspace into a host stage, then scans, hashes, registers, seals, uploads and commits before it answers (`afr_supervisor/src/run_loop/recovery_capture.rs:99-128`).
- **Every save reads every byte.** The export streams the whole workspace (`recovery_capture.rs:109-110`), and the catalog hashes each file in 8 MiB chunks (`recovery/catalog.rs:168`; `recovery/files.rs:14`). It registers unchanged chunks too, 256 per request (`af_wire/src/durable_task/checkpoint/mod.rs:25`).
- **Turn manifests relist everything.** A turn save passes no base (`recovery/turn/capture.rs:210-215`), so its pages list every entry. A task increment lists every entry too, adding base paths only as `Remove` (`recovery/catalog.rs:122-128`).
- **The daemon reads every new object back.** The runner re-authorizes each upload, and the daemon GETs, opens and checks it before it counts (`recovery/turn/session.rs:157-165`; `afd_fleet/src/durable_task/checkpoint/transfer/verify.rs:54-87`).
- **Every object carries its own wrapped key.** Registration mints and wraps a data key per object (`afd_fleet/src/durable_task/storage/registration.rs:59-92`). Each workspace and repository keeps a wrapped identity key in its own table (`afd_db/schema/20261122090000_runner_checkpoint_identity_keys.sql:5-20`).
- **A restore reads everything first.** Every chunk is fetched, at most four per authorization (`recovery/session.rs:176`), before the first model call (`run_loop/turn_recovery.rs:20-62`).

```text
wait today ≈ export(E, B) + read and SHA-256(B) + ⌈C/256⌉·RTT + N·(seal + PUT + daemon GET + open)/4 + P·RTT + commit
E entries · B bytes · C chunks · N new chunks · P manifest pages · RTT one runner-to-daemon round trip
```

**Solution summary.** §1 seals every checkpoint with one vault key per workspace, behind a cipher the daemon's file selects, so no table stores a key. §2 splits a save at the pin: under the pause the executor copies only changed bytes, then the run resumes and publication follows, with no read-back. §3 makes the walk read only what changed.

§4 serves M258_001's `Snapshots` on Firecracker. §5 and §6 put the workspace on a host ublk device over a block-level chunk map in Postgres: §5 restores lazily, and §6 uploads dirty chunks continuously, so the host disk is only a cache.

**Design flow:**

```text
keys (daemon only)
  vault.secrets row ─► afd_vault load() ─► the workspace's key, handed with each authorization ─► ChunkCipher (aes_256_gcm | plain)
boundary
  quiesce               the executor has a call in flight ─► the save skips
  pause                 the executor freezes the guest's tenant leaf (D22)
  pin   stage 1         statx walk of E entries; changed bytes ΔB copied to root-only host staging
        stages 2-3      guest flush (FIFREEZE or syncfs); one generation switch in the block server
  resume                tenant code and the next model call run on
background, one publication per owner
  changes(pin) ─► register ΔC ─► seal under the derived key ─► PUT, If-None-Match: *, bound SHA-256
               ─► delta pages, parent = last committed ─► commit head, recording verified_at ─► release(pin)
restore
  stage 1               resolve the chain ─► fetch and check every chunk ─► renew() ─► import
  stage 2               map only ─► renew() ─► a read miss fetches one checked chunk
```

| | Still paused | After resume |
|---|---|---|
| Today | everything in the formula above | nothing |
| Stage 1 | stat walk of E entries, copy of ΔB | register, seal, PUT, pages, commit |
| Stages 2 and 3 | guest flush of its dirty page cache, one generation switch | the same, reading changed chunks from the pinned generation |

**What stays exactly as today, for safety.**

1. Chunks are sealed with AES-256-GCM (Advanced Encryption Standard, Galois/Counter Mode), bound to workspace, repository, object and format (`checkpoints.md:122`).
2. Objects are written once: every upload link binds `If-None-Match: *` and the ciphertext's SHA-256 (`afd_fleet/src/durable_task/storage/transfer.rs:116-123`).
3. A restore checks each chunk's ciphertext digest, tag and plaintext digest against Postgres before writing a byte, and fails closed (`afr_supervisor/src/recovery/transfer.rs:170-212`).
4. No runner or sandbox holds a bucket token, a listing right or a delete right (`checkpoints.md:114`).
5. Manifest pages, object rows and the current generation live in Postgres (`afd_db/schema/20261122090200_runner_checkpoints.sql`, `…090300_runner_checkpoint_pages.sql`, `…090500_runner_checkpoint_objects.sql`).

**What is unverified.**

- The racy-clean guard's exact form, and whether a write through a shared mapping always moves a file's change time (D23). The tests in §3 settle both.
- Whether R2 refuses a presigned PUT whose body differs from its bound `x-amz-checksum-sha256`. §2's probe checks every bucket before checkpoints activate.
- Which block layer serves a Firecracker drive best on the dev host, and what `FIFREEZE` costs in our guest. The spike in §5 settles them.

**What changes for a fleet.** Each line is a behaviour Indy accepts by approving this spec.

1. A save's pause no longer grows with the workspace's size. A fleet's tools stop for a walk of its files and a copy of what changed.
2. The run continues before its save commits. A crash in between falls back to the previous checkpoint, as a skipped save does today.
3. A background process the run left behind is frozen for the pin, where today it keeps running through a save.
4. After §5, a restored run starts before every chunk is fetched, and `/tmp` is a separate disk that no save holds (D19).
5. After §6, a held workspace keeps nothing on its host.

## PR Intent & comprehension handshake

- **PR title (eventual):** carried by M258_001's title for the one M258 Pull Request.
- **Intent (one sentence):** A save stops a fleet's tools only while changed bytes are copied, and a restored run starts before its workspace arrives.
- **Authoring handshake:** Indy wants the Sprites design applied to our checkpoint and restore "to make them faster". He ruled the map into Postgres, keys derived from one vault secret, R2's checksum in place of the read-back, and bubblewrap deleted first. ASSUMPTIONS I'M MAKING:
  1. M258_001 and M258_003 land first on the branch. This spec builds on `import`, `export`, `Snapshots` and its trials, the inner environment and `Executor::renew`, and adds only the pin pause (D22).
  2. Every ruling in Discovery holds; D23 stays open until Dimension 3.4 settles it.
  3. Nothing is backward compatible. The manifest format, the checkpoint tables and the runner protocol edit in place, with one `VERSION` move, and the dev database is torn down.
  4. One commit per Section. §5 and §6 build only after their spikes pass.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_fleet/src/durable_task/storage/registration.rs`: the per-object key minting §1 replaces.
2. `rustd/crates/daemon/afd_vault/src/bag/mod.rs`: the platform bag the checkpoint secret becomes.
3. `rustd/crates/runner/afr_supervisor/src/run_loop/recovery_capture.rs`: the save the run waits for today; §2 ends the wait at the pin.
4. `rustd/crates/runner/afr_supervisor/src/recovery/catalog.rs`: the scan and hash §3 replaces with a walk of identities.
5. `docs/architecture/checkpoints.md`: the publication, encryption and transfer rules this spec edits.
6. https://fly.io/blog/sprites-block-device.md: one ledger, a checkpoint as a volume version, and a restore that unmounts, rewinds and mounts.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M258_002_P1_API_DOCS_INFRA_OBS_SAVES_PAUSE_ONLY_FOR_WHAT_CHANGED.md` | EDIT | this spec; moves to `active/`, then `done/` |
| `docs/v2/reviews/m258-block-layer-spike.md`, `docs/v2/reviews/m258-writeback-spike.md` | CREATE | §5's and §6's spike records; helpers stay outside the repository |
| `rustd/crates/foundation/af_crypto/{Cargo.toml,src/checkpoint.rs,src/aad.rs}` | EDIT | §1: `ChunkCipher` with `Aes256Gcm` and `Plain`; `PreparedCheckpointKey`, `WrappedCheckpointKey` and `Aad::checkpoint_identity` go |
| `rustd/crates/daemon/afd_db/schema/20261122090000_runner_checkpoint_identity_keys.sql` | DELETE | §1 |
| `rustd/crates/daemon/afd_db/schema/{20261122090200_runner_checkpoints,20261122090400_runner_checkpoint_purges,20261122090500_runner_checkpoint_objects}.sql` | EDIT | edited in place: §1 drops the identity-key and envelope columns; §3 adds `parent_checkpoint_id` |
| `rustd/crates/daemon/afd_db/{src/migration.rs,tests/integration_migrate/**}` | EDIT | §1: the identity table's entry, grant and immutability rows go; Dimension 1.2 |
| `rustd/crates/daemon/afd_fleet/src/durable_task/{storage,checkpoint}/**`, `afd_fleet/src/durable_task/types.rs`, `afd_fleet/src/run/turn_checkpoint/**`, `afd_gateway/src/{lib.rs,connection/**}` | EDIT | §1: secret, derivation, bucket path; §2: read-back, its semaphores and budget gone, checksum probe, `verified_at` at commit; §3: parents; §5: single-chunk links, pin renewal; §6: writeback |
| `rustd/crates/daemon/afd_api_tenant/src/handler/secret/**` | EDIT, if D24 is ruled as recommended | the secret routes refuse the checkpoint secret's name |
| `rustd/crates/protocol/af_chunk_store/**` | CREATE | §2 to §5: the one storage crate both binaries share |
| `rustd/crates/protocol/af_wire/src/{durable_task/checkpoint/**,run/turn_checkpoint/**,protocol/mod.rs}`, `public/openapi.json` | EDIT | `parent` on both plans; a block manifest; the read-back budget goes; `VERSION` moves once; OpenAPI regenerated with `--no-banner` |
| `rustd/crates/runner/afr_engine/**` | EDIT | `WorkspaceVolume`, `SnapshotRef`'s fields, the executor digest in `Environment`, two trials |
| `rustd/crates/runner/{afr_executor,afr_guest,afr_engine_firecracker}/**` | EDIT | §2: the pin pause verbs and the tenant-leaf freeze; §3: the identity walk, its cache and staging; §4: `Snapshots`; §5: the flush, the ublk and `/tmp` drives, rewind, boot refusal |
| `rustd/crates/runner/afr_supervisor/src/{recovery/**,run_loop/**,client/turn_checkpoint.rs}`, `afr_agent/src/loop/recovery_boundary.rs` | EDIT | §2: the pause ends at the pin, publication after resume, no read-back, staging swept at boot; §3: delta chains |
| `rustd/crates/runner/afr_volume/**` | CREATE | §5, §6: the ublk server, its chunk cache, fetch, dirty bitmap and writeback |
| `rustd/crates/daemon/afd_fleet/tests/**`, `rustd/crates/e2e/af_e2e/{tests/recovery_process/**,benches/recovery/**}`, `rustd/crates/runner/main/examples/kernel_lane/**` | EDIT | keys, pins, chains, ancestors and collection against live Postgres, `identity_retention.rs` deleted; §3's chain restore and bench shape; the kernel trials of §2 to §5 |
| `rustd/crates/daemon/main/src/**` | EDIT | §6: the production caller of checkpoint maintenance (`checkpoints.md:172`) |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | `libublk` for §5; `aws-lc-rs` in `af_crypto` only if Dimension 1.8 picks it |
| `deploy/baremetal/agentsfleet-runner.service`, `playbooks/lib/runner/prepare.sh` | EDIT | §5: `ublk_drv` loads beside `erofs` (`agentsfleet-runner.service:23`) |
| `docs/architecture/{checkpoints,runner_execution}.md` | EDIT | keys, no read-back, the pause, the chain, the block layer, what a snapshot never crosses |
| `~/Projects/docs` `changelog.mdx` | EDIT, on that repository's own branch after merge | saves pause only for changed bytes |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR and RS-SOURCE for the new crates' errors through `error_shell!`; RS-UNSAFE for the ublk buffers; RS-RACE for one publication per owner.
- **`domains/sql.md`** — SQL-FILE and STS for the tables edited in place, SQL-FKNAME for `parent_checkpoint_id`, SQL-FKSTATE for the ancestor refusal, TXN, SQL-ONEFACT.
- **`tests.md`** — TEST-NEG, TEST-INJECT, TEST-PARTIAL for each ordered step of publication, TEST-RESIDUAL, TEST-KILL, TEST-REAL, TEST-LANE, TEST-SKIP, TEST-DELTA.
- **`code.md`** — NLG for everything edited in place, ORP, OWN for each pin, staging file and device, TIM for pin lifetimes, ECL, FLS.
- **`domains/auth.md`** — AUTH-RANDOM for the secret and nonces, AUTH-NOLOG for derived keys. **`domains/logging.md`** — LOG-EVENT, LOG-SECRET. **`domains/http-surface.md`** — HTTP-OPENAPI. **`domains/architecture.md`** — ARCH-WINS, ARCH-LAND.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` with `cargo shear` | yes | `libublk` enters only `afr_volume`; moved code drops what it leaves behind |
| `make test-unit-all` | yes | `af_chunk_store` joins the `crates/protocol/*` shard and `afr_volume` the `crates/runner/*` shard (`make/test-unit.mk:9,11`) |
| `make test-integration-rustd` | yes | keys, chains, ancestors, pins and collection run against live Postgres |
| `af_e2e/tests/layers.rs` | yes | `af_chunk_store` is `af_` in `protocol/`, `afr_volume` is `afr_` in `runner/`, and neither names a datastore client (`layers/rules.rs:12`) |
| `make test-runner-kernel` on a host with `/dev/kvm` | not in a green Pull Request | runs before the Pull Request opens; counts go in the Session Notes |
| OpenAPI drift | yes | regenerate with `--no-banner` |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | `afr_volume` splits into device, cache, fetch, bitmap and writeback modules |

## Prior-Art / Reference Implementations

- **Reference:** the vault's own AES-256-GCM construction, RustCrypto's `aes-gcm` beside `hmac` and `sha2` (`rustd/Cargo.toml:414-418`). The platform bag is `afd_vault/src/bag/mod.rs`, and §2's checksum probe mirrors the write-once probe (`storage/transfer.rs:41-95`).
- **Reference:** today's task overlay (`recovery/restore_manifest.rs:115-134`): turns gain the same parent chain, with `Remove` entries applied over a base.
- **Reference:** Fly's block device (https://fly.io/blog/sprites-block-device.md, L50-64): one ledger, a checkpoint as a version, a rewind in place. Divergence: the map lives in Postgres, and the server runs on the host, so the guest never reaches the bucket.
- **Reference:** `libublk` (https://github.com/ublk-org/libublk-rs, MIT OR Apache-2.0) and its `loop.rs` example, the shape of `afr_volume`; §5's spike pins the version. No block protocol is hand-written.
- **Reference:** git's index, the identity tuple and racy-clean rule behind §3's cache, which `recovery/files.rs:16-36` already computes.
- **CLI practices:** N/A; no command-line change.

## Sections (implementation slices)

Build order: §1 lands first, because every later Section seals under its keys. Stage 1 is §2 and §3, which need no new block layer, and §4's snapshots build on them. Stages 2 and 3 are §5 and §6, and each opens with a spike that commits only its record.

### §1 — One vault key per workspace seals every checkpoint, behind a cipher the daemon's file selects

**How keys move today.** The daemon opens an object's wrapped key at each authorization and returns it as `data_key`, beside the registered `nonce` (`afd_fleet/src/durable_task/checkpoint/transfer/signing.rs:88-110`; `af_wire/src/durable_task/checkpoint/objects.rs:165-168`). The supervisor zeroes it on drop (`afr_supervisor/src/recovery/mod.rs:79-87`). The identity key never leaves the daemon, which computes each content identity itself (`checkpoint/identity.rs:24-35`).

**The secret.** Each workspace holds one checkpoint secret: 32 bytes from `af_crypto::entropy` in a `vault.secrets` row, read as a platform bag through `afd_vault` `load()` (`afd_vault/src/load.rs:51-66`). `R2CheckpointStore` already holds the `Vault` it needs (`durable_task/storage/mod.rs:69-91`). The first save creates the row with `Vault::create`, which refuses a name the workspace holds, so a racing save loads the winner's row (`afd_vault/src/write.rs:40-45,89`).

**One key, three uses.** The workspace's key is used as it is stored (Indy: "I want to go with one raw key"): it seals every file chunk, it seals the turn history root, and it keys each chunk's content identity. Nothing is derived and no other key is stored. `object_nonce` stays, a random 96-bit nonce per object from `af_crypto::entropy`, because every object of a workspace now shares one key (`af_crypto/src/checkpoint.rs:62-65`). NIST SP 800-38D allows 2^32 random nonces under one key; at 8 MiB a chunk that is 32 exbibytes sealed per workspace, and a rotation resets the count. The turn history root is sealed under the same key with its own random nonce, and its per-checkpoint envelope columns go (`…090200_runner_checkpoints.sql:28-30`).

**The cipher is a trait with two implementations.** `af_crypto::ChunkCipher` has `seal` and `open`; `Aes256Gcm` is today's construction and `Plain` passes bytes through. The daemon's `[checkpoint] cipher` names one, `aes_256_gcm` by default; `plain` exists so a deployment can measure a save and a restore without encryption before turning it on, and boot logs `checkpoint_cipher_plain` once while it is set. The key is minted, loaded and handed over the same under both, and every digest and identity check runs under both. Each checkpoint row records the cipher that sealed it, bound from a named constant, so a restore opens it as it was sealed and a later change of the setting strands nothing. `make bench-recovery` measures a chunk and a full checkpoint, saved and restored, under `plain`, under RustCrypto's `aes-gcm` and under `aws-lc-rs`'s AES-256-GCM, whose bytes are interchangeable. The faster one ships; within noise RustCrypto stays, so `af_crypto` links no C.

**Who holds what.** The daemon loads the key and hands it to the runner with each save's and restore's authorization; the supervisor zeroes it on drop, as it does today's (`afr_supervisor/src/recovery/mod.rs:79-87`). The runner therefore holds the workspace's long-lived key in memory during a save or a restore, where today it holds one object's. Indy accepted that (Discovery): a compromised runner host that also reads the bucket reads every checkpoint of that workspace.

**Rotation and a lost key.** A master-key rotation re-wraps the row and leaves the key's bytes alone (`afd_vault/src/stored_envelope.rs:14`), so checkpoints keep opening. Rotating a workspace's key retires its checkpoints: the daemon writes the new key with `Vault::replace` (`afd_vault/src/write.rs:139`), marks the workspace's generations for purge, and the next save is a full one. A run interrupted across a rotation starts from its original message, as a turn with no checkpoint does (`checkpoints.md:100`). A key row that is missing or will not open ends the same way, and never serves a byte.

**Deleted, edited in place.** `runner.checkpoint_identity_keys` goes with its migration entry (`afd_db/src/migration.rs:169`), statements and retention pass (`checkpoint/maintenance/mod.rs:28-30`). `runner.checkpoint_objects` loses `checkpoint_identity_key_id` and its seven envelope columns (`…090500_runner_checkpoint_objects.sql:8,20-26`), and `runner.checkpoint_purges` loses `checkpoint_identity_key_id`. The bucket path drops the identity key's segment (`storage/transfer.rs:239-246`).

- **Dimension 1.1** — one key does all three uses, under either cipher → Test `one_workspace_key_seals_chunks_and_history_and_keys_identity` (unit, `af_crypto`). Under `Aes256Gcm`, a chunk and a history root sealed with one key open with it and fail their tag under another; the same content gives the same identity under one key and another under a second. Under `Plain`, `seal` returns its input and `open` returns it back, and the identity is unchanged.
- **Dimension 1.2** — no table holds a checkpoint key → Test `no_checkpoint_table_holds_key_material` (integration, `afd_db`, live Postgres). After migration, `runner.checkpoint_identity_keys` does not exist, and no `runner` column is named `dek_%`, `secret_%`, `kek_version` or `%identity_key%`. A schema keeping one envelope column fails it.
- **Dimension 1.3** — a restore on another runner opens under the daemon's key → Test `a_restore_on_another_runner_opens_chunks_under_the_workspace_key` (integration, `afd_fleet`, live Postgres and the S3 fixture). Runner A seals two generations and is dropped, and runner B's restore authorizations open every chunk. The key reaches B only in an authorization frame for that workspace, and a frame for another workspace carries another key.
- **Dimension 1.4** — the vault row is created once per workspace → Test `concurrent_first_saves_create_one_checkpoint_secret` (integration, live Postgres). Fifty first saves of one workspace, released from a barrier, leave one `vault.secrets` row under the key's name, and every chunk they sealed opens. A second workspace gets its own row with different bytes.
- **Dimension 1.5** — a rotated or lost key retires the checkpoints it sealed → Test `a_rotated_or_lost_key_starts_the_run_from_its_message` (integration, live Postgres). After a rotation, the workspace's earlier generations are marked for purge and the next save is full. A restore finding the row replaced or missing reads no checkpoint, writes no file, logs `checkpoint_key_unusable`, and the run starts from its original message.
- **Dimension 1.6** — the tenant sees the key's row and cannot change it → Test `the_checkpoint_secret_is_listed_and_refuses_a_tenant_write` (integration, `afd_api_tenant`, live Postgres). The secret list names the row; a replace or a delete of its name through the secret routes is refused, and the stored bytes are unchanged (D24).
- **Dimension 1.7** — a checkpoint opens as it was sealed → Test `a_checkpoint_opens_under_the_cipher_that_sealed_it` (integration, `afd_fleet`, live Postgres). A generation saved under `plain` restores after the setting moves to `aes_256_gcm`, and one saved under `aes_256_gcm` restores after it moves back; boot under `plain` logs `checkpoint_cipher_plain` once.
- **Dimension 1.8** — both ciphers are measured → Test `make bench-recovery` (manual, the dev host). The Session Notes hold a chunk's and a full checkpoint's save and restore times under `plain`, RustCrypto AES and `aws-lc-rs` AES, with the command that produced each.

### §2 — The pause ends at the pin, and publication runs after it with no read-back

This Section splits a save at the pin, with today's full walk behind it, so §3 changes only what the walk reads.

**The volume.** M258_001's seam gains `WorkspaceVolume`: `pin(parent)`, `changes(pin)` and `release(pin, committed)`. The caller has paused tenant code before `pin`, which returns once no later write can change what it reads. The supervisor's pipeline consumes `changes` unchanged across all three stages.

**The storage crate.** `af_chunk_store`, an `af_` crate in `protocol/`, is the one crate Indy ruled (Discovery), with no public SDK. It starts with `Change`; §3 moves the delta chain's resolution and the restore's read check into it, §4 adds `ChunkSource` and `ChunkStore`, and §5 `BlockMap`.

**The pin pause (D22).** Firecracker's quiesce reads only the executor's calls in flight, so a detached tenant process can write during an export (`afr_engine_firecracker/src/engine/vm.rs:114-120`; `afr_executor/src/server/session.rs:167-174`). **Implementation default:** the executor freezes the guest's tenant leaf through `afr_cgroup::Freezer` (`afr_engine_firecracker/src/guest/linux.rs:12-14,220-226`; `afr_cgroup/src/cgroup/freezer/mod.rs:44-71`). The executor sits in the guest's root cgroup beside the leaf (`guest/linux.rs:78-80`), so it keeps answering and the VM never stops.

**Publication.** After resume the pipeline registers, seals, uploads, writes pages and commits. The receipt reaches the agent loop after resume, so the next model call does not wait, and a run's end waits for its owner's publication. A boundary that finds its owner's publication running skips, as an optional save skips a busy slot (`checkpoints.md:36`).

- **State authority:** the owner's head, `thread.turns.head_checkpoint_id` or `fleet.durable_tasks.checkpoint_id`.
- **Guarded transition:** a commit advances the head only from the `expected_head` its plan named (`recovery/turn/capture.rs:234-237`).
- **Commit before release:** `release(pin, committed: true)` runs only after the commit answers, and the next pin diffs against that generation.
- **Failure assumption:** an uncertain commit is reconciled against its operation and generation, as today (`checkpoints.md:83`).

**No read-back (D21).** R2's own checksum stands in for the daemon's GET: each upload link binds `x-amz-checksum-sha256` (`storage/transfer.rs:116-123`). Before checkpoints activate, the daemon proves the bucket refuses a body unlike its bound checksum, beside today's write-once probe. **Implementation default:** the commit records `verified_at` for its plan's objects whose PUT the runner reports accepted, so no call is added.

A restore still runs every check of "What stays" item 3. A runner that registered one plaintext and uploaded another is caught there, at restore, which is the trade D21 accepted.

**Staging.** Changed bytes wait in one staging file per pin, beside today's stages and readable by the runner's user alone (`docs/architecture/runner_execution.md:186`). It holds plaintext at the workspace image's own trust, "every file the run wrote" (`afr_image/src/workspace_disk/mod.rs:14-16`). Boot removes every staging file, and today's per-save upload bounds cap its size (`checkpoints.md:139`).

- **Dimension 2.1** — the pause ends before any daemon call → Test `a_save_resumes_before_it_registers_or_uploads` (unit, `afr_supervisor`, a fake daemon and a recording volume). The log reads quiesce, pause, pin, resume, then register, seal, PUT, pages and commit. A register sent before resume, even from a spawned task, fails it.
- **Dimension 2.2** — the run does not wait for publication → Test `the_next_model_call_does_not_wait_for_publication` (unit, the agent loop over a fake supervisor). The fake daemon holds every PUT, the next model request is sent while the upload is held, and the receipt arrives after it. The run's settlement follows the commit, so an end that drops the publication fails it.
- **Dimension 2.3** — a failed publication keeps the previous generation → Test `a_publication_failing_after_resume_keeps_the_previous_generation` (integration, live Postgres). The store fails the third PUT, the head still names the previous generation, the staging file is gone, and `checkpoint_publication_failed` is logged. The next save's delta names that previous generation as its parent.
- **Dimension 2.4** — one publication per owner → Test `a_boundary_finding_a_publication_running_skips` (unit, contention). With the first save's upload held, 50 boundaries released from a barrier each skip as busy and none pauses. Exactly one generation commits.
- **Dimension 2.5** — a runner killed after resume loses nothing committed → Test `a_runner_killed_between_resume_and_commit_keeps_the_previous_generation` (integration, live Postgres). `SIGKILL` lands after resume and before commit, and the head names the previous generation. The next boot leaves no staging file, and a replacement restores that generation byte for byte.
- **Dimension 2.6** — the pin pause freezes tenant code and keeps the executor → Test `the_pin_pause_freezes_tenant_code_and_keeps_the_executor_answering` (integration, kernel lane, Firecracker). A detached tenant loop's counter, started under `setsid`, holds still through the pause while `list_dir` answers, and advances after resume. A call still in flight skips the save as `BusyExecutor` with no pin taken, and the VM monitor's process identifier is unchanged.
- **Dimension 2.7** — publication reads nothing back → Test `a_publication_reads_no_object_back` (integration, live Postgres and the S3 fixture in `afd_fleet/tests/integration_durable_tasks/decisions/checkpoint_server.rs`). A save of 10 new chunks commits while the fixture counts 10 PUTs and zero GET or HEAD requests. An object whose PUT the runner reports failed stays without `verified_at`, and the commit refuses.
- **Dimension 2.8** — a bucket that ignores the checksum blocks checkpoints → Test `a_bucket_that_accepts_a_wrong_checksum_blocks_checkpoints` (integration, the S3 fixture). A fixture that stores a body unlike its bound checksum leaves checkpoints unavailable, naming the checksum; one that refuses it lets them activate. A probe that checks only the write-once answer fails it.
- **Dimension 2.9** — a restore refuses an object unlike its registration → Test `a_restore_refuses_an_object_unlike_its_registration` (unit, `af_chunk_store`'s read check, an in-memory store). An object whose ciphertext opens under its key but whose plaintext differs from its registered digest writes no file, and the previous checkpoint stays. A check of the tag alone fails it.

### §3 — Stage 1: a save reads only what changed

**Identity cache.** The executor keeps each path's identity at the last committed pin: device, inode, size, and nanosecond modification and change times (`recovery/files.rs:16-36`). The cache is a hint that a restore never trusts. A missing cache, after a reboot or on a new host, costs one full hash.

**Under the pause.** The executor walks the tree with `statx` through directory descriptors, in place of an open and a stat per file. An unchanged identity is skipped unless its change time falls at or after the previous pin's clock tick, and that racy-clean guard re-reads it. Changed files are read, hashed with SHA-256 and streamed to staging over the executor's vsock link.

**After resume.** Only changed chunks are registered. The manifest is a delta against the parent generation, holding changed entries and `Remove`, resolved by the chain code moved from `recovery/restore_manifest.rs:115-134` into `af_chunk_store`. Turns gain the same chain, and the restore's read check moves there too (`recovery/transfer.rs:170-212`).

**The full-manifest rule.** A save writes a full manifest, with no parent, once the chain's delta pages, summed, would outnumber a full manifest's. The threshold comes from the data, so it is no invented limit. A cacheless save also writes a full manifest, and its unchanged chunks register as `Reused` and upload nothing.

**Ancestors.** **Implementation default:** `runner.checkpoints` gains `parent_checkpoint_id`, a foreign key to `runner.checkpoints` with no cascade, edited in place. Postgres then refuses to delete an ancestor that a delta names, and purge reads that refusal with `afd_db::constraint::violates_foreign_key`. A chain a full manifest has superseded is purged newest first.

- **Dimension 3.1** — untouched files are neither read nor registered → Test `a_save_reads_and_registers_only_the_changed_files` (unit, `afr_executor` walker and `afr_supervisor` catalog, a counting reader). With 1,000 files and 3 rewritten, bytes read under the pause equal those 3 files' bytes, and registrations name only their chunks. A walker that reads everything but registers 3 fails it, and the restore equals the source tree.
- **Dimension 3.2** — the pause does work in entries and changed bytes only → Test `a_pause_does_work_in_entries_and_changed_bytes_only` (unit, counters, sparse files). Workspaces of 1,000 entries, with 100 MiB or 64 GiB of untouched bytes and the same 3 changes, do equal work under the pause. Each counts 1,000 stats, the changed bytes, zero daemon calls and zero seals.
- **Dimension 3.3** — a change the stat cannot see is still saved → Test `a_same_tick_same_size_rewrite_is_saved` (unit). A file is rewritten within the previous pin's clock tick at the same size, its modification time set back. The change-time guard re-reads it, the restore holds the new bytes, and the test fixes the guard's form.
- **Dimension 3.4** — a write through a mapping is saved → Test `a_shared_mapping_write_is_saved_by_the_next_save` (integration, kernel lane, Firecracker). A tenant writes through `MAP_SHARED` to a page it wrote before the previous pin, and the next restore holds the new bytes. Whether that write moves the change time is D23, and a red blocks §3 until Indy rules.
- **Dimension 3.5** — a lost cache costs one full hash → Test `a_save_without_its_cache_reads_everything_once` (unit). With no cache, every file is read once and a full manifest is written. It resolves to the tree a warm-cache chain resolves to, and its unchanged chunks upload nothing.
- **Dimension 3.6** — a save writes only what changed → Test `a_turn_save_writes_only_changed_and_removed_entries` (unit, `afr_supervisor`). After a full base of 1,000 entries, a save that changed 2 files and deleted 1 writes 2 file entries and 1 `Remove`. Its `parent` names the base, and resolving the chain equals a full scan's manifest.
- **Dimension 3.7** — a chain never outgrows a full manifest → Test `a_chain_whose_deltas_outgrow_a_full_manifest_writes_a_full_one` (unit). With a 3-page full manifest, a chain reaching 4 summed delta pages makes the next save full, with no parent. A chain at 3 pages stays a delta.
- **Dimension 3.8** — an ancestor outlives every delta that names it → Test `an_ancestor_of_a_ready_delta_is_never_purged` (integration, live Postgres). Deleting a parent that a ready delta names fails on `parent_checkpoint_id`'s constraint, and purge skips it. Once a full manifest supersedes the chain and it settles, purge removes the chain newest first.
- **Dimension 3.9** — a delta chain moves hosts → Test `a_killed_runner_restores_a_delta_chain_on_another_host` (e2e, `af_e2e`, mirroring `killed_runner_mid_turn_restarts_on_another_host` at `af_e2e/tests/recovery_process/mod.rs:33`). A source runner saves a full manifest and three deltas through the real routes, frames and R2, then is killed. The replacement restores byte for byte, and its first model call follows the restore.
- **Dimension 3.10** — the measured pause stays flat as the workspace grows → Test `make bench-recovery` with the large-workspace, small-change shape (manual). Aiwa runs it for 100 MiB and 10 GiB of untouched files with one changed 1 MiB file. The larger's median pause falls within the spread of ten runs of the smaller, and the numbers go in the Session Notes.

### §4 — Snapshots on Firecracker, and what a snapshot never crosses

**Our engine's snapshots.** `afr_engine_firecracker` declares snapshots, where it declares none today (`afr_engine_firecracker/src/capabilities.rs:18`). It serves `Snapshots` on the volume and a `ChunkStore` the supervisor hands it at `prepare`. `take(id)` pauses, pins, resumes and publishes one generation, and `forget` drops the ref, so maintenance may collect what nothing else names.

**The ref.** `SnapshotRef` holds only its id today (`afr_engine/src/engine/snapshots/mod.rs:12-14`). It gains the sandbox's environment, the generation and its format, `Files` or `Blocks`. `Environment` gains the executor's digest here, at its first reader, as M258_001 §2 recorded.

**Restore inside the running VM.** `Executor::renew()` ends every process of the inner environment, empties the workspace and builds the environment again (`afr_executor/src/api/mod.rs:297-306`). The workspace is then rebuilt through `import` in stage 1 and by rewinding the block device in stage 2. The run's egress plan reads back in force before the executor answers, and the VM monitor keeps running.

**The production caller.** On Firecracker the supervisor's recovery save is `take`, and its restore is `restore`. The debug engine, `unsandboxed`, declares no snapshots (`afr_engine_unsandboxed/src/unsandboxed/mod.rs:43`) and keeps the stage-1 volume through `export`.

**One behaviour.** Our engine passes M258_001's four snapshot trials (`afr_engine/src/conformance/mod.rs:140-143`). This spec adds two for every engine that declares snapshots: a snapshot never crosses environments, and a restore never brings back an older egress plan.

| Saved state | Restores on | Never on, and why |
|---|---|---|
| A file-format generation | a Firecracker sandbox with the same toolbox and executor digests (`checkpoints.md:93`) | the debug engine, whose root is the host's own files, so the digest check refuses |
| A block-format generation | a Firecracker sandbox with the same digests | any host-kernel mount: nothing hands an unhashed image to the host's file-system parser (`afr_image/src/toolbox/mod.rs:9-12`) |
| Memory | nowhere | no engine snapshots memory here; Firecracker restores memory only on identical host software and hardware |

- **Dimension 4.1** — our engine passes every snapshot trial → Test `firecracker_passes_every_snapshot_trial` (integration, `make test-runner-kernel`). M258_001's four trials and this spec's two pass with the in-memory store, and no restore changes the VM monitor's process identifier. No trial reads skipped.
- **Dimension 4.2** — a snapshot never crosses environments → Test `a_snapshot_never_crosses_environments` (unit, `afr_engine` trial over fakes and our engine). A ref naming another toolbox digest, another executor digest or the debug engine's host root is refused naming `environment`, before any chunk is read. A block-format ref offered to the debug engine is refused the same way.
- **Dimension 4.3** — a restore never brings back an older plan → Test `a_restore_never_brings_back_an_older_plan` (unit trial over fakes; on Firecracker in 4.1's lane). A snapshot taken under plan generation 3 is restored while 4 is in force, and `in_force()` reads 4 before the executor answers. A fake whose restore brings back 3 fails the trial by name.
- **Dimension 4.4** — no key or link reaches the guest → Test `no_key_or_link_reaches_the_guest_across_take_and_restore` (integration, kernel lane, Firecracker). After a take and a restore, M258_001's planted-token search covers the guest's files, environment and `/proc`. It finds no derived key, nonce or signed URL, and finds a canary planted on purpose.

### §5 — Stage 2: lazy restore behind a host ublk device

Stage 2 makes a restore wait only for the chunks a run reads. **Gate:** Indy approves the spike's run on the dev host, and Dimension 5.1 passes.

**Mechanism.** `afr_volume`, built on `libublk`, serves the workspace as `/dev/ublkbN`, Firecracker's data drive. Its origin is a block-level chunk map in Postgres (D16, D17). A read miss fetches one object through a daemon-signed link and runs `af_chunk_store`'s read check, and writes land in a local sparse file.

**Block capture.** The server's dirty bitmap names the changed chunks. A pin is the executor's guest flush, `FIFREEZE` or `syncfs`, then one generation switch in the server. The pause then grows with the guest's dirty page cache alone.

**Restore in place.** The executor ends the inner environment and unmounts the workspace. The server rewinds the device to the target generation, dropping local writes, and the executor mounts it again (Fly's block-device post, L62).

**`/tmp` on its own disk (D19).** Today `/workspace` and `/tmp` share one drive (`afr_engine_firecracker/src/guest/linux.rs:64-69`), while a save holds only `/workspace` (`afr_executor/src/api/mod.rs:209-212`). The guest gains a scratch drive for `/tmp` that no save holds. **Implementation default:** each drive is a sparse file sized to the run's disk limit, because a sparse file costs the host only what is written.

**Must be true first.**

- The manifest gains a block variant under the next `CHECKPOINT_FORMAT` and `TURN_CHECKPOINT_FORMAT` values (`af_wire/src/durable_task/checkpoint/mod.rs:19`; `af_wire/src/run/turn_checkpoint/mod.rs:30`).
- The restore pin outlives the fill. A pin lasts 2 minutes for turns and 30 for tasks, and a link 5 (`checkpoints.md:116,143`). The pin renews while its generation is an origin.
- An authorization names any single chunk on demand, where a restore batch holds at most four today (`recovery/session.rs:176`). `ublk_drv` loads on every runner host, and Firecracker's boot refuses a host without it.

**The spike (D17, D18).** It compares ublk on the host, Firecracker's vhost-user-blk, and a FUSE-backed drive file (AWS Lambda's design, USENIX ATC 2023). ublk stays the build target unless the record shows it cannot serve, and another winner goes back to Indy. `docs/v2/reviews/m258-block-layer-spike.md` also records whether a rewind needs the guest's block cache dropped, and what `modinfo dm_clone` prints.

- **Dimension 5.1** — spike: three block layers serve a Firecracker drive → Test `the_block_layer_spike_measures_three_ways_to_serve_a_drive` (manual). Aiwa serves one sparse file to a guest data drive each way, and the guest formats, writes, syncs and runs `FIFREEZE`. The server is killed and restarted, and the synced file must read back. The record gives each way's first-read latency, `FIFREEZE` cost and write amplification at three chunk sizes, today's 8 MiB among them, so a one-way record fails.
- **Dimension 5.2** — a run starts before its last chunk arrives → Test `the_first_model_call_starts_before_every_chunk_is_fetched` (integration, kernel lane, Firecracker). A run reading 10 MiB of a restored 4 GiB generation sends its first model request while fetched chunks are fewer than total. GET requests equal the distinct chunks the device served, so a restore that fetches everything fails.
- **Dimension 5.3** — a tampered chunk serves no byte → Test `a_tampered_chunk_fails_its_read_and_serves_no_byte` (unit, `afr_volume`, an in-memory source). One flipped ciphertext byte makes the read answer an input and output (I/O) error, and no plaintext reaches the device. `checkpoint_chunk_refused` is logged, and the run ends closed.
- **Dimension 5.4** — a missing or killed block layer never serves a run → Test `a_block_layer_that_is_missing_or_killed_ends_closed` (integration, kernel lane, Firecracker). Without `ublk_drv`, boot refuses naming `ublk`; after `SIGKILL` mid-write, a restarted server re-attaches and the guest's synced file reads back. When re-attach fails, the run ends `sandbox_unavailable` and publishes nothing from that device.
- **Dimension 5.5** — a block save names only dirty chunks → Test `a_block_save_publishes_only_the_dirty_chunks` (unit, `afr_volume`). After a restore, writes to chunks 3 and 9 make `changes` yield exactly those two, and none from the `/tmp` drive. The counters under the pause show the flush and the switch, and no chunk read.
- **Dimension 5.6** — a fill outliving its pin renews or ends closed → Test `a_lazy_restore_outliving_its_pin_renews_or_ends_closed` (integration, live Postgres). A fill past the pin's deadline renews it and keeps reading through single-chunk authorizations. When renewal is refused because the fence moved, reads stop, the run ends closed, and no stale byte is served.

### §6 — Stage 3: the workspace rooted in R2, the host disk a cache

Stage 3 makes R2 the workspace's home. **Gate:** §5 passes, Indy approves the spike's run on the dev host, and Dimension 6.1 passes.

**Mechanism.** Between pins the server uploads dirty chunks in the background, each a new write-once object with its own nonce. Each is sealed under the key derived for the pending generation it joins (§1), so no key is minted or stored per chunk. A pin publishes the map once its chunks' PUTs are accepted, so time to current grows only with chunks dirtied since the last upload.

**Backpressure.** When R2 is slower than writes, writes stall rather than outrun local space, and the host disk is the bound the operating system forces. A failed upload is retried as a new object, never an overwrite.

**Holds and loss.** A `Stop` hold whose last pin published drops the local cache, so a held workspace needs nothing on its host. A host death loses only writes after the last accepted upload. For a turn, files newer than its conversation checkpoint are unused anyway (`checkpoints.md:89-102`).

**Collection.** Overwritten chunks become garbage that refs collect, which is Fly's log-structured disk lesson. No production code calls checkpoint maintenance today (`checkpoints.md:172`), so §6 adds that caller in the daemon. It collects only objects no generation or ancestor names.

- **Dimension 6.1** — spike: writeback's rate and stalls are measured → Test `a_writeback_spike_measures_upload_rate_and_stalls` (manual). On the dev host a fixture fleet clones and builds a repository under continuous writeback at D18's chunk size. `docs/v2/reviews/m258-writeback-spike.md` records PUTs and object rows per minute, publish lag, and what a writer sees under a throttled bucket.
- **Dimension 6.2** — every writeback is a new object → Test `dirty_chunks_upload_between_pins_as_new_objects` (integration, live Postgres). Writes between two pins become objects whose upload links all bind `If-None-Match: *` and a checksum, each with its own nonce under its generation's derived key. A replayed upload to the same key is refused, and the bucket holds one copy.
- **Dimension 6.3** — no generation names an unaccepted chunk → Test `a_server_killed_at_each_ordered_step_never_publishes_an_unaccepted_chunk` (integration, fault injection). The server dies after a write, after registration, after the PUT and before commit. No generation names an object without `verified_at`, and the current generation restores byte for byte.
- **Dimension 6.4** — a slow bucket stalls writes and fails none → Test `writes_stall_while_the_bucket_is_slower_than_the_cache_fills` (unit, `afr_volume`, a throttled store). With the cache at its disk bound, guest writes block, none fails, and no generation names a chunk not yet uploaded. Writes resume when the store drains, and `checkpoint_writeback_stalled` is logged once per stall.
- **Dimension 6.5** — maintenance collects garbage and keeps ancestors → Test `maintenance_collects_an_overwritten_chunk_once_nothing_names_it` (integration, live Postgres, through the daemon's production caller). An object superseded and named by no generation or ancestor is tombstoned. An object a ready delta's ancestor names is kept.
- **Dimension 6.6** — a workspace survives moves → Test `the_write_checkpoint_hold_move_restore_soak_verifies_every_byte` (integration, kernel lane, Firecracker, two runner processes). Two hundred cycles of write, checkpoint, hold, move to the other runner and restore run in turn. Every byte verifies each cycle, no generation is lost, and a held workspace leaves no cache file behind.

## Interfaces

```text
Keys (af_crypto::checkpoint, daemon only)
  trait ChunkCipher { seal, open }: Aes256Gcm | Plain, by `[checkpoint] cipher` (default aes_256_gcm); one key per workspace for chunks, history and identity
Vault        one platform bag per workspace: Vault::create once, then load()
Wire         CheckpointObject { data_key: the workspace's key, cipher, nonce: the object's registered nonce }
             both checkpoint plans gain parent: Option<checkpoint id>; a block manifest under the next format value
             TurnCheckpointAction::VERIFY_ANSWER_TIMEOUT and verifies_uploads() go
WorkspaceVolume (afr_engine)
  attachment() -> Directory | Image | BlockDevice
  pin(parent: Option<&Generation>) -> Pin       caller has paused tenant code; returns at the consistency point
  changes(&Pin) -> stream of Change              after resume: stage 1 file entries, stages 2-3 block chunks
  release(Pin, committed: bool)
af_chunk_store (protocol/)
  Change       Entry(CheckpointEntry) | Chunk { key: File { path, index } | Block { index }, bytes }
  ChunkSource  read(object) -> plaintext         checks ciphertext digest, tag and plaintext digest
  ChunkStore   ChunkSource + publish(changes)    supervisor over signed links; in memory behind test-util
  BlockMap     { volume_bytes, chunk_bytes, chunks: index -> object }   §5's origin, published as manifest pages
Origin       Empty { bytes } | Generation { manifest, source: Arc<dyn ChunkSource>, fill: Eager | Lazy }
Executor     pause() · resume() freeze and thaw the guest's tenant leaf
             pin_walk(parent) streams changed bytes (stage 1) · flush() is FIFREEZE or syncfs (stage 2)
Snapshots on Firecracker
  take(id)     pause · pin · resume · publish
  restore(ref) renew() · origin swapped · plan read back
SnapshotRef  { id: Uuid7, environment, generation, format: Files | Blocks }
Schema       runner.checkpoints.parent_checkpoint_id -> runner.checkpoints (id), no cascade
             runner.checkpoint_identity_keys dropped; the objects' identity-key and envelope columns dropped
```

The runner protocol `VERSION` (`af_wire/src/protocol/mod.rs:15`) moves once, for the turn plan's `parent` and the read-back's removal.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Secret replaced or deleted | failure | the vault row's bytes change | a restore of an earlier generation stops on the tag; the run ends closed | `a_restore_after_the_secret_is_replaced_fails_closed` |
| First saves race | race | two first saves of one workspace | one vault row; the losing save loads it | `concurrent_first_saves_create_one_checkpoint_secret` |
| Publication fails after resume | failure | a PUT or the commit fails | the head keeps the previous generation; staging removed; the next save diffs against it | `a_publication_failing_after_resume_keeps_the_previous_generation` |
| Publication still running | race | a boundary arrives mid-publication | the boundary skips as busy, with no pause | `a_boundary_finding_a_publication_running_skips` |
| Runner killed after resume | failure | `SIGKILL` between resume and commit | the previous generation stays current; boot removes staging | `a_runner_killed_between_resume_and_commit_keeps_the_previous_generation` |
| Executor still serving | failure | a call in flight at quiesce | the save skips as `BusyExecutor`; no pin | `the_pin_pause_freezes_tenant_code_and_keeps_the_executor_answering` |
| Bucket ignores the checksum | dependency | a bucket stores a body unlike its bound SHA-256 | checkpoints stay unavailable, naming the checksum | `a_bucket_that_accepts_a_wrong_checksum_blocks_checkpoints` |
| Object unlike its registration | failure | a runner uploaded other bytes than it registered | the restore's plaintext check refuses; no file is written | `a_restore_refuses_an_object_unlike_its_registration` |
| Stat misses a change | failure | a same-tick, same-size rewrite | the change-time guard re-reads it | `a_same_tick_same_size_rewrite_is_saved` |
| Mapped write unseen | failure | a shared-mapping write to a dirty page | saved, or §3 blocks until D23 is ruled | `a_shared_mapping_write_is_saved_by_the_next_save` |
| Cache lost | failure | a reboot or a new host | one full hash and a full manifest | `a_save_without_its_cache_reads_everything_once` |
| Ancestor purged | failure | maintenance or deletion under a delta | Postgres refuses; purge skips | `an_ancestor_of_a_ready_delta_is_never_purged` |
| Snapshot across environments | security | a ref from another root, executor or format | refused before any chunk is read | `a_snapshot_never_crosses_environments` |
| Plan rolled back | security | a snapshot taken under an older plan | the run's generation reads back before the executor answers | `a_restore_never_brings_back_an_older_plan` |
| Key or link in the guest | security | a leak through take or restore | the planted-token search fails the lane | `no_key_or_link_reaches_the_guest_across_take_and_restore` |
| Tampered chunk | security | ciphertext changed in R2 | an I/O error; no byte served; the run ends closed | `a_tampered_chunk_fails_its_read_and_serves_no_byte` |
| Block layer missing or killed | dependency | no `ublk_drv`, or the server dies | boot refuses; re-attach, or the run ends closed | `a_block_layer_that_is_missing_or_killed_ends_closed` |
| Pin expires mid-fill | timeout | a long fill, or a moved fence | the pin renews, or reads stop and the run ends closed | `a_lazy_restore_outliving_its_pin_renews_or_ends_closed` |
| Server killed mid-writeback | failure | a crash at any ordered step | no generation names an unaccepted chunk | `a_server_killed_at_each_ordered_step_never_publishes_an_unaccepted_chunk` |
| Bucket slower than writes | capacity | R2 throttles | writes stall; none fails | `writes_stall_while_the_bucket_is_slower_than_the_cache_fills` |
| Replayed upload | replay | a retried PUT to a used key | `If-None-Match: *` refuses it | `dirty_chunks_upload_between_pins_as_new_objects` |

## Invariants

1. No table stores a checkpoint key. A derived key is `SecretBytes`, which implements neither `Serialize` nor a statement encoding (`af_crypto/src/secret/mod.rs:44-45`), and Dimension 1.2 checks the schema.
2. No daemon call, seal or upload runs during the pause. The pipeline takes only the `Resumed<Pin>` that resume returns, so publishing under the pause cannot compile (Dimension 2.1).
3. One publication runs per owner. A one-permit semaphore per owner is taken with `try_acquire`, and a failed take skips the boundary (Dimension 2.4).
4. A generation is current only after every object it names has `verified_at`. The commit refuses any other, and records `verified_at` only for objects whose bound PUT was accepted (Dimensions 2.7, 6.3).
5. An ancestor of a ready delta cannot be deleted, because `parent_checkpoint_id` is a foreign key with no cascade (Dimension 3.8).
6. Every object is written once. Each upload link binds `If-None-Match: *` and the ciphertext's checksum, and the runner refuses a link without them (Dimension 6.2).
7. A snapshot restores only into the environment it names. `restore` compares `SnapshotRef.environment` with the sandbox's before any read (Dimension 4.2).
8. No guest receives a key, a link or a bucket token. `ChunkStore` lives in the supervisor, and no executor verb carries a key or a link (Dimension 4.4).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `checkpoint_secret_created` (log) | operator | a first save creates a workspace's secret | `workspace_id` | no key bytes | `concurrent_first_saves_create_one_checkpoint_secret` |
| `checkpoint_paused` (log) | operator | resume after a pin | `owner_kind`, `engine`, `stage`, `entries`, `changed_bytes`, `paused_ms` | no path, content identity, key or link | `a_pause_does_work_in_entries_and_changed_bytes_only` |
| `checkpoint_published` (log) | operator | a background commit succeeds | `owner_kind`, `generation`, `delta`, `new_objects`, `uploaded_bytes`, `publish_ms` | as above | `a_save_resumes_before_it_registers_or_uploads` |
| `checkpoint_publication_failed` (log) | operator | publication ends without a commit | `owner_kind`, `step`, `error_code` | as above | `a_publication_failing_after_resume_keeps_the_previous_generation` |
| `checkpoint_chunk_refused` (log) | operator | a fetched chunk fails its check | `object_id`, `check` | no plaintext, key or link | `a_tampered_chunk_fails_its_read_and_serves_no_byte` |
| `checkpoint_writeback_stalled` (log) | operator | writes block on uploads | `engine`, `pending_chunks`, `stalled_ms` | host facts only | `writes_stall_while_the_bucket_is_slower_than_the_cache_fills` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every open owner decision is ruled (Discovery) | `git grep -n -E "^  - \*\*[A-Z][0-9]+\*\* .\[\?\]" -- "docs/v2/*/M258_002_*"` | no output | P0 | |
| R2 | Both spike records are committed (§5, §6) | `git ls-files docs/v2/reviews/m258-block-layer-spike.md docs/v2/reviews/m258-writeback-spike.md` | two lines | P0 | |
| R3 | Keys derive from one vault secret, and none is stored (§1) | `make test-integration-rustd` | exit 0; `no_checkpoint_table_holds_key_material ... ok`, `a_restore_on_another_runner_opens_chunks_under_daemon_derived_keys ... ok`, `concurrent_first_saves_create_one_checkpoint_secret ... ok` | P0 | |
| R4 | Keys derive deterministically, a save resumes before publication, one per owner, and the pause does work in entries and changed bytes only (§1 to §3) | `make test-unit-all` | exit 0; `a_chunk_key_derives_from_its_secret_and_checkpoint_alone`, `a_save_resumes_before_it_registers_or_uploads`, `a_boundary_finding_a_publication_running_skips`, `a_pause_does_work_in_entries_and_changed_bytes_only` pass | P0 | |
| R5 | Publication reads nothing back, failed and killed saves keep the previous generation, chains keep ancestors and move hosts, and writeback never publishes an unaccepted chunk (§2, §3, §6) | `make test-integration-rustd` | exit 0; `a_publication_reads_no_object_back ... ok`, `a_runner_killed_between_resume_and_commit_keeps_the_previous_generation ... ok`, `an_ancestor_of_a_ready_delta_is_never_purged ... ok`, `a_killed_runner_restores_a_delta_chain_on_another_host ... ok`, `a_server_killed_at_each_ordered_step_never_publishes_an_unaccepted_chunk ... ok` | P0 | |
| R6 | Snapshots hold to one behaviour, and a lazy restore starts first (§4, §5) | `make test-runner-kernel` | exit 0; zero failed trials; `firecracker_passes_every_snapshot_trial` and `the_first_model_call_starts_before_every_chunk_is_fetched` passed | P0 | |
| R7 | The key table and the read-back are gone (§1, §2) | `git grep -n -w -e checkpoint_identity_keys -e checkpoint_identity_key_id -e remote_scope -e read_object -e VERIFY_ANSWER_TIMEOUT -- rustd docs/architecture` | no output | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `runner.checkpoint_identity_keys`, with its schema file, migration entry, statements, retention pass and test, and the objects' and purges' `checkpoint_identity_key_id` | `git grep -n -w -e checkpoint_identity_keys -e checkpoint_identity_key_id -- rustd docs/architecture` | no output |
| the objects' seven envelope columns, per-object key minting, the identity key's wrapping binding and its maintenance counter | `git grep -n -w -e dek_ciphertext -e PreparedCheckpointKey -e WrappedCheckpointKey -e checkpoint_identity -e identity_keys -- rustd/crates/daemon/afd_db/schema/20261122090500_runner_checkpoint_objects.sql rustd/crates/daemon/afd_fleet/src/durable_task rustd/crates/daemon/afd_fleet/src/run/turn_checkpoint/sql/objects_scope.sql rustd/crates/foundation/af_crypto/src` | no output |
| the daemon's read-back and its budgets | `git grep -n -w -e remote_scope -e read_object -e verify_object -e VERIFY_ANSWER_TIMEOUT -e verifies_uploads -e checkpoint_verifications -- rustd` | no output |
| the claim that the freeze spans publication | `git grep -n -e "holds freeze until every scan" -- rustd docs` | no output |

## Out of Scope

- The Sprites engine and its snapshots on Fly's checkpoints, the engine policy and burst: M260_001.
- Memory snapshots, pause holds and pools of pre-booted VMs, as M258_001 records; a write-ahead journal; content-defined chunking and `fanotify` hints (Decomposition): not planned.
- Matching `rusqlite`, `libsqlite3-sys` and `redis` in `DATASTORE_CLIENTS` (`af_e2e/tests/layers/rules.rs:12`): its own change, which Indy schedules.
- A quota on a workspace's stored bytes: none exists (`checkpoints.md:173`), and none is invented here.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator reads `checkpoint_paused` lines whose `paused_ms` stays flat as a fleet's workspace grows from megabytes to gigabytes.
2. **Preserved user behaviour** — every checkpoint keeps AES-256-GCM, write-once objects, the restore's checks and Postgres records. A fleet's configuration, tools and restore refusals stay as they are.
3. **Optimal-way check** — derive keys, skip unchanged files and move every network step after resume. The gap is lazy restore, which needs a block layer.
4. **Rebuild-vs-iterate** — iterate on keys and the publication pipeline for stage 1. Stages 2 and 3 rebuild the disk, each gated on its spike.
5. **What we build** — derived keys, the pin seam, the identity cache, staging, delta chains, chunk-map snapshots, the ublk server and writeback.
6. **What we do NOT build** — JuiceFS, SQLite with Litestream, a key table, a read-back, a journal, content-defined chunking, change hints, memory snapshots or a public SDK.
7. **Fit with existing features** — compounds with M258_001's executor files and inner environment. It must not weaken the restore's checks before a generation is used.
8. **Surface order** — keys first, then the runner's pause, then the daemon's commit, refs and pins, then the architecture page; no command or page changes.
9. **Dashboard restraint** — the app shows nothing new. Operators read the six log events.
10. **Confused-user next step** — a refused restore names its reason, as today. `docs/architecture/checkpoints.md` states the keys, the pause, the chain and what a snapshot never crosses.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** keys, the pin and publication, stage 1's walk, snapshots on it, then stages 2 and 3, each behind its spike. Each Section leaves checkpoints working.
- **Alternatives considered:**
  - Today's wrapped key per object, its identity-key table and the daemon's read-back of every object: replaced by Indy's Keys and D21 rulings.
  - JuiceFS, whose client holds bucket keys and database credentials beside tenant code. SQLite with Litestream, a bucket key on every runner and a second ledger, which Fly removed (D16).
  - A write-ahead journal through signed links: durability for files newer than the last boundary, which a restore cannot use.
  - Network Block Device (NBD), whose Rust crate was last updated in March 2024. `dm-era` or `dm-thin` deltas, which need the GNU General Public License (GPL) 3.0 tools or a subprocess in a core path.
  - `fanotify` hints, which lose events on overflow and miss `mmap` writes, and `fastcdc` chunking, which pays only for large files edited mid-file.
- **Patch-vs-refactor verdict:** a **refactor**, because the keys, the save's shape, the manifest's chain and the disk's home all change.

## Discovery (consult log)

- > Owner (Indy, Nov 26, 2026): "Also i feel the checkpoint storage area on git is moving toward the code.storage area? do you think we abstract it as an sdk so anyone can call and use that?" and "just add to the spec for discovery or so when we do that" — context: ruled below as one crate now, `af_chunk_store`, and an SDK later. Images in messages (M259_002) and browser screenshots (M259_001) need the same store. What "code.storage" refers to is unverified.
- > Owner (Indy, Nov 26, 2026): "also read this sprite design https://fly.io/blog/design-and-implementation/ so you know if you prefer to redesign anything", then "i am thinking we must do that" and "even in our checkpoint and restore to make them faster" — context: this spec applies the post's object-storage disk to our checkpoints.
- > Owner (Indy, Nov 26, 2026): "how about the litestream.io and the sqlite thing int he article" and "and juicefs too,  yes the chunk map .. to is an important post that mean the encrupt and stop the world will go away?" — context: ruled as D16. Encryption stays and leaves the pause, and the pause shrinks to a flush and a pin, never zero.
- > Owner (Indy, Nov 26, 2026): "I prefer the milestone where ever we do the firecracker and sprites engine abstractions move (to me this is a compute layer) must  fix the checkpoint and snapshot, this must be the first milestone so we sunset bubblewrap and move on to firecracker and sprites" — context: M258 is the compute layer and fixes checkpoint and snapshot; O1 packs it into one Pull Request.
- > Owner (Indy, Nov 26, 2026): "bubblewrap must be retired if we do 3rd party sprites based firecraker or any sandbox or firecracker engine our own." — context: M258_003 deleted bubblewrap before this Workstream (D22), so every Section here is Firecracker's.
- **Owner rulings, Nov 26, 2026** (verbatim in AskUserQuestion dialogs and messages; these govern this spec).
  - **Keys** supersede D20. > Owner (Indy, Nov 26, 2026): "On encryption keep things simpler, and less call", "I dont want the humoungous set of encryption keys to maintain", chose "Derive keys, store none (Recommended)", then "follow our standard practice of using vault, you have crap invented another identity later for checkpoint with separate tables that is insane" — context: §1. Each workspace's one checkpoint secret is a standard `vault.secrets` row read through `afd_vault` `load()`. Chunk keys and the identity key derive from it by HKDF-SHA256, and the identity table and per-object key columns go. D25 below supersedes the derivation: the key is used as stored.
  - **D16** > Owner (Indy, Nov 26, 2026): "the chunk map in Postgres. No SQLite, no Litestream, no JuiceFS." — context: the start message; §5's `BlockMap`.
  - **D17** > Owner (Indy, Nov 26, 2026): chose "ublk on the host (Recommended)" — "i want to do a spike vm test using the available options here ublks vs firecracker vhost-user-blk, are there other options? that SPACE uses or any other recommended by AWS?" — context: §5's spike compares ublk, vhost-user-blk and a FUSE-backed drive file (AWS Lambda's design, USENIX ATC 2023).
  - **D18** > Owner (Indy, Nov 26, 2026): chose "Measure 3 sizes, pick by data (Recommended)" — context: Dimension 5.1.
  - **D19** > Owner (Indy, Nov 26, 2026): chose "Separate /tmp disk (Recommended)" — context: §5.
  - **D21** > Owner (Indy, Nov 26, 2026): chose "Trust R2's own checksum" — context: §2. The daemon's read-back goes; a restore still checks each chunk's tag and identity against Postgres and fails closed.
  - **D22** > Owner (Indy, Nov 26, 2026): chose "Delete bubblewrap before M258_002 (Recommended)" — context: build order M258_001, M258_003, M258_002; the pin pause is Firecracker's inner environment, frozen by the executor.
  - **Storage crate** > Owner (Indy, Nov 26, 2026): chose "One crate now, SDK later (Recommended)" — context: `af_chunk_store`, §2.
  - **O1** > Owner (Indy, Nov 26, 2026): chose "One PR for all of M258".
- **Open owner decisions.** Each stays open until Indy rules; each recommendation is Aiwa's.
  - **D23** `[?]` whether a write through a shared mapping always moves a file's change time. Dimension 3.4 settles it on the kernel lane. If it does not, recommend re-reading every file a tenant mapped writable since the last pin.
  - **D24** ruled. > Owner (Indy, Nov 27, 2026): chose "Tenant vault, read-only" — "Daemon only must write it, the name can be the <short form workspace id>checkpoint just like short git commit ids, Ideally leave it visible, but not manageble by an enduser" and "These are system artifacts so how do we manage it in the vault? if a naming convention is followed there is no reason for refusal and dups." — context: the checkpoint secret is a row in the tenant's own `vault.secrets`, listed to the tenant, written by the daemon alone; the secret routes refuse a replace or delete of its name beside the connector-grant check (`afd_api_tenant/src/handler/secret/support.rs:77-79`). The row is one per workspace under the table's unique `(workspace_id, key_name)`, so its name is one fixed constant and needs no workspace id in it.
  - **D25** ruled. > Owner (Indy, Nov 27, 2026): "What is HKDF? I would say just use the same key for chunks and checkpoint that we coined in D24. So we know per workspace this is the key used.", then "i want to go with one raw key. But first i need to test hence what i would suggest is the implementation on minting keys and such can be build. But have noop trait or whatever or unsecure trait for now which passes thru the encrypt and decrypt. have your encrypt decrypt built too. But in the toml have a flag that uses no op. So essentially everything happens but its just we turn in on after measuring how the unencrypted plan chunk, full checkpoint or whatever take time. And what will be the secure one take?" — context: §1 uses the workspace key as stored for chunks, the history root and identities, behind `ChunkCipher` with `Aes256Gcm` and `Plain`, chosen by `[checkpoint] cipher`. Then: chose "Raw key to the runner, accepted"; "can you use tnay other crate since we use aws-lc-sys?"; "but we will have to think on the case where keys are compromised or invalid. but i know our vault inherently offers way to rotate" — context: the runner holds the workspace's key in memory during a save or restore, so a compromised runner host that also reads the bucket reads that workspace's checkpoints; Dimension 1.8 measures both AES libraries; §1's rotation paragraph and Dimension 1.5.
- **Consults** — the research reports named in Provenance; Fly's design post (L76-84) and block-device post (L22-64) are cited sources, not mechanisms to copy. The report's bubblewrap halves lapsed when M258_003 deleted the engine.
- **Model reviews** — none yet. The Elephant-Goldfish Model (EGM) review runs at `orly-spec` before the docs Pull Request.
- **Open blockers** — none; Indy approved dev host access on Nov 27, 2026 ("Dev host access: yes go ahead"), which the spikes in Dimensions 5.1 and 6.1 use.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
