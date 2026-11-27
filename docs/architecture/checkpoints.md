---
type: explanation
audience: contributor
verified: 2026-11-27
product_version: 0.90.0
executable: false
---

# Checkpoints

## What it is

A checkpoint is a saved copy of a run's progress. A replacement runner restores the latest complete checkpoint when the first runner dies mid-turn.

| Question | Answer |
|---|---|
| What it holds | The conversation, accepted answers, finished child-agent results, memory changes and the folded messages it covers; workspace files when the run has a sandbox |
| Who owns it | One ordinary turn (`thread.turns.head_checkpoint_id`) or one durable task (`fleet.durable_tasks.checkpoint_id`), never both |
| Where history lives | Sealed under the workspace's checkpoint key, in Postgres |
| Where files live | Sealed objects in Cloudflare R2 object storage, configured by `[checkpoint.storage]` |
| What seals them | One key per workspace, in that workspace's vault; `[checkpoint] cipher` names the cipher |
| Who moves the bytes | The runner's supervisor, through short-lived signed links the daemon issues |
| What turns resume on | `[runner.recovery] resume_from_checkpoint`, `false` in both deployments |

## Why it exists

A runner can vanish at any moment, and another runner then takes the turn ([failure recovery](runner_fleet.md#failure-recovery-model)). Without a checkpoint, the replacement starts the turn again from its first message.

A checkpoint limits the repeat to the work done since the last save. Encryption and daemon-held storage credentials keep tenant files safe if a runner host or the bucket leaks.

## How it behaves

### When the runner saves

The agent saves at a safe boundary: after a completed model turn and its input reconciliation, before the next model call. Busy child agents, open command sessions, unfinished file work or live tenant processes defer the save.

A file save needs a verified toolbox and a free transfer slot on the runner. An optional save skips a busy slot and keeps the previous checkpoint.

```text
Harness::save_boundary                         afr_agent loop/recovery_boundary.rs
  |-- file slot busy -> skip -> keep the previous ready checkpoint
  `-- admitted -> BoundaryOwner::capture       afr_supervisor run_loop/recovery_capture.rs
         -> quiesce: a tenant process still runs -> skip this save
         -> export the workspace through the executor into a host stage
         -> scan the stage, encrypt, authorize, upload, read back and verify
         -> publish the exact root and manifest under the run's fence
         -> remove the stage -> acknowledge the saved generation
```

Firecracker answers `quiesce` by asking the guest's executor whether it is idle. A workspace that will not export skips the save, and the run goes on ([capture](../../rustd/crates/runner/afr_supervisor/src/run_loop/recovery_capture.rs)).

A checkpoint names its turn and owner, never a version of the fleet. A restore under a different model keeps the conversation and tool results. It drops only reasoning the old provider signed, because another provider cannot read it.

### Where the records live

| Workload | Saved state | Owner |
|---|---|---|
| Ordinary turn, no files | Encrypted history root in Postgres | The turn's head on `thread.turns` |
| Ordinary turn with files | The same root, plus an ordered manifest and encrypted file chunks in R2 | The same turn head |
| Durable task | Task history and a file manifest over a shared repository base | The task's checkpoint and generation |

```text
thread.turns.head_checkpoint_id ──┐        ┌── fleet.durable_tasks.checkpoint_id
                                  ▼        ▼
                         runner.checkpoints (turn_id XOR durable_task_id)
                           │ pages · pins · object_refs
                           ▼
                         runner.checkpoint_objects        vault.secrets (checkpoint_key, one per workspace)
                           │ delete trigger
                           ▼
                         runner.checkpoint_purges

runner.checkpoint_bases: a workspace's verified repository base, shared by task checkpoints
thread.replay_receipts:  scope <workspace>:turn:<turn>:checkpoint, (scope, operation_id) unique
```

The [checkpoint schema](../../rustd/crates/daemon/afd_db/schema/20261122090200_runner_checkpoints.sql) enforces one owner per row. A base outlives the task that first captured it ([bases](../../rustd/crates/daemon/afd_db/schema/20261122090100_runner_checkpoint_bases.sql)).

Storage writes for one workspace run one at a time. Object registration, verification, purge acknowledgement, base removal and a key rotation's object removal first call `identity.lock_workspace_storage`.
It takes `FOR NO KEY UPDATE` on the workspace row, so the key-share locks of `identity.lock_workspace` and foreign keys stay free ([function](../../rustd/crates/daemon/afd_db/schema/20261108120300_identity_workspaces.sql)).

### Publication

A new generation becomes usable only when its root and every object are verified and its head advances in Postgres. An uncertain publication is reconciled against the original operation and generation.

If a refused replay leaves a pending generation, the owner must prove it was abandoned before another save ([publication](../../rustd/crates/runner/afr_supervisor/src/recovery/turn/publication.rs)).

### Restore on a replacement runner

```text
runner A dies          its renewals stop
lease deadline passes  the turn becomes assignable
runner B assigned      the turn's fence rises from N to N+1; B starts renewing
B restores             the exact checkpoint, checked against turn, generation, root and toolbox
B adopts               the folded messages the checkpoint covers, under fence N+1
B runs                 the first resumed model call; one fenced settlement at the end
```

| Head on the turn | What the replacement does |
|---|---|
| Ready | Restores the root and its matching files |
| Absent | Starts from the original message and any unsettled folded messages |
| Ready, but its workspace key is missing, will not open or did not seal it | Logs `checkpoint_key_unusable`, retires the workspace's checkpoints and starts from the original message |
| Invalid root or file | Stops before any model call or tool |

A required file restore waits for a transfer slot while renewal continues. The supervisor writes the files into a host stage through checked directory descriptors ([restore](../../rustd/crates/runner/afr_supervisor/src/recovery/turn/restore.rs)).

It then imports the stage through the executor into the sandbox's workspace, which must be empty ([staging](../../rustd/crates/runner/afr_supervisor/src/staging/mod.rs)). Firecracker offers no host path into a microVM, so capture and restore both cross through the executor.

Frames from the dead run carry the old fence and cannot change the new attempt. With resume off, a turn holding a committed checkpoint waits at its fleet's head after its run expires; saving still happens.

Work after the last save can repeat, external effects included. A retried `message` or schedule call lands once, because each names its call and the daemon stores the first answer. A replacement run's opening note lists the calls the dead run finished.

### Object storage and encryption

The daemon alone resolves the R2 endpoint, bucket and vault credential from `[checkpoint.storage]` ([storage](../../rustd/crates/daemon/afd_fleet/src/durable_task/storage/mod.rs)). The runner and its sandbox never hold a bucket token, a listing right or a delete right.

The daemon signs each upload or download link for one object, valid five minutes by default. An upload link binds the length, a SHA-256 checksum and `If-None-Match: *`, so an object is written once and never replaced.

Before a task that targets a pull request starts, the daemon proves the bucket refuses a second write to one key. A bucket that accepts it blocks the task.

Each workspace holds one checkpoint key: 32 bytes from `af_crypto::entropy`, in a `vault.secrets` row named `checkpoint_key` in the workspace's own vault ([keys](../../rustd/crates/daemon/afd_fleet/src/durable_task/storage/keys.rs)). The workspace's first save creates the row, and of two racing first saves one creates it and the other loads it.

The key is used as stored. It seals every file chunk and the turn's history root, and it keys each chunk's content identity. No checkpoint table holds a key, and no other key is derived from it.

The secret list shows the row to the tenant. The tenant's secret routes refuse to create, replace or delete that name with 403 `UZ-VAULT-006`.

`[checkpoint] cipher` names what seals a new checkpoint: `aes_256_gcm` by default, or `plain`, which stores bytes as they are so a deployment can measure a save and a restore without encryption. Boot logs `checkpoint_cipher_plain` while `plain` is set. Each checkpoint and each object records its cipher, and a restore opens with the recorded one ([checkpoint cipher](../../rustd/crates/foundation/af_crypto/src/checkpoint.rs)).

The daemon hands the key, the cipher and the object's registered nonce to the supervisor with each save's and restore's authorization. The supervisor seals with AES-256-GCM (Advanced Encryption Standard, Galois/Counter Mode), bound to workspace, repository, object and format, and wipes the key when it drops it. A compromised runner host that also reads the bucket reads that workspace's checkpoints.

Every object carries its own random 96-bit nonce, because all of a workspace's objects share one key.

Content identity is a keyed hash-based message authentication code (HMAC) over the plaintext's SHA-256 digest and size, under the workspace's key. Equal content within one workspace and repository reuses one stored object. Nothing is shared across workspaces, so one tenant cannot learn what another stores.

### Rotating a workspace's key

A platform operator rotates one workspace's key with `agentsfleet platform checkpoint-key rotate <workspace_id>`, which calls `POST /v1/admin/workspaces/{workspace_id}/checkpoint-key/rotate` and needs `platform-key:admin`.

```text
one transaction
  delete the workspace's checkpoint generations   heads clear; an interrupted run starts from its message
  delete its repository bases
  delete its object rows                          each records a bucket delete in runner.checkpoint_purges
  write a new key with the vault
answer: retired_generations, retired_objects      the next save uploads everything again
```

A key row that is missing, will not open or did not seal the head ends the same way. The daemon finds it when it assigns the replacement run, and no byte of the old checkpoint is served ([resume](../../rustd/crates/daemon/afd_fleet/src/run/turn_checkpoint/head.rs)).

Restore checks the ciphertext digest, the authentication tag and the plaintext identity before writing any file. A mismatch keeps the previous checkpoint and blocks that restore.

### Sizes and transfer limits

| Bound | Value | Source |
|---|---|---|
| File chunk | 8 MiB of plaintext | `af_wire` `CHECKPOINT_OBJECT_BYTES` |
| Turn manifest page | 512 KiB | `af_wire` `TURN_CHECKPOINT_PAGE_BYTES` |
| Task manifest | 1 MiB pages, 256 objects per authorization, 64 MiB in total | `af_wire` durable-task checkpoint; `afd_fleet` `checkpoint/commit.rs` |
| Conversation history | 64 MiB serialized | `afr_agent` `engine/recovery.rs` |
| Turn checkpoint message | One 32 MiB runner frame | `af_wire` `MESSAGE_BYTES_MAX` |
| New upload per save | Turn 16 GiB; task base 16 GiB; task increment 1 GiB | `afr_supervisor` `recovery/turn/capture.rs`, `recovery/capture.rs` |
| Runner concurrency | Two file operations at once, four transfers each | `afr_supervisor` `recovery/admission.rs`, `recovery/uploads.rs` |
| One object request | 60 seconds | `afr_supervisor` `recovery/transfer.rs` |
| Upload readback answer | 35 seconds; two checks per session, eight per daemon replica | `af_wire` `turn_checkpoint/command.rs`; `afd_gateway` |
| Transfer window and restore pin | 15 minutes; pin 2 minutes for turns, 30 for tasks | [configuration reference](config_reference.md) |

A restore window holds up to four 8 MiB chunks and can span files. Two windows hold at most 64 MiB of plaintext before transport overhead ([window](../../rustd/crates/runner/afr_supervisor/src/recovery/window.rs)).

Remote object reads and writes run outside the database transaction. The daemon rechecks the pin deadline and custody before recording a verified upload.

### Checking recovery on a rig

The recovery bench kills a source runner after a complete checkpoint and connects a replacement. It verifies the higher fence and the restored inputs, then waits for settlement.

Run it on a local rig whose database the bench may reset.

```bash
make bench-recovery PROFILE=rig BENCH_FLEETS=1 BENCH_WORKSPACES=shared
```

```text
→ [bench-recovery] profile=rig
... Running benches/recovery/main.rs (<varies>)
{"shape":"recovery","workload":"api_only","fleets":1,...}
```

Latency runs from the later of the old run's and thread's expiry to the first restored model request ([bench](../../rustd/crates/e2e/af_e2e/benches/recovery/measure.rs)).

The bench first prints what sealing and opening cost, with no datastore: one 8 MiB chunk and a 1 GiB checkpoint, stored plain, under RustCrypto's AES-256-GCM and under `aws-lc-rs`'s ([cipher bench](../../rustd/crates/e2e/af_e2e/benches/recovery/cipher.rs)). `--cipher-only` stops after those lines.

## Limits

- Resume is off in both deployments (`[runner.recovery] resume_from_checkpoint = false`). Both set `[checkpoint.storage]` to their own R2 bucket, so file checkpoints save ([prod](../../deploy/fly/agentsfleetd-prod/agentsfleetd.toml), [dev](../../deploy/fly/agentsfleetd-dev/agentsfleetd.toml)).
- Without `[checkpoint.storage]`, the daemon refuses file checkpoints; history-only checkpoints still save.
- No engine offers snapshots. M258_002 replaces this file checkpoint path with a chunk map behind the seam's snapshot interface.
- No production code calls checkpoint maintenance (`Tasks::maintain_checkpoints`). A running daemon deletes no settled checkpoint and collects no orphaned upload; the retention keys change nothing.
- No quota caps a workspace's total stored bytes. The per-save upload bounds above are the only size limits.
- A signed link cannot be revoked before it expires. A stopped run cannot publish, and no sandbox receives a link or key.
- A master-key rotation is not built. Rotating a workspace's checkpoint key deletes its checkpoints instead of sealing them again.
- Work after the last save can repeat, external effects included.

## Related pages

- [Runner fleet](runner_fleet.md): how a dead runner's turn is reassigned
- [Durable tasks](durable_tasks.md): task lifecycle and guarded publication
- [Runner runtime](runner_execution.md): the sandbox and the held workspace between runs
- [Configuration reference](config_reference.md): `[checkpoint] cipher`, `[checkpoint.storage]`, `[turns.checkpoints]` and `[durable_tasks.checkpoints]`
- [Sandbox evaluation](../reviews/space-sandbox/sandbox_evaluation.md): the design study that proposed this storage
