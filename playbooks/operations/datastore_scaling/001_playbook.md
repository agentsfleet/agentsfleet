# Datastore Scaling Readiness

**Owner:** Human approves deployed and paid operations; Agent runs local proof.
**Current scope:** Redis historical capture on the repository-owned local rig.

This route prepares evidence for the Dragonfly migration. It does not switch a
deployment, provision Dragonfly Cloud, or retire Redis. Those actions remain
blocked on the later readiness and cutover gates in M192.

## Historical Redis baseline

The capture plan binds the campaign to baseline revision
`521ca4037ebbd23056f8b3b63dcf9c2fa34f650d`. The capture command refuses any
production source, schema, build-input, normalized lockfile, or resolved
production dependency change between that revision and the committed capture
revision.

Run from a clean committed worktree with Docker available:

```bash
make bench-datastore-capture PROFILE=rig
```

The target starts and resets the repository's compose Postgres and Redis for
each sample. It captures three samples for each of `steer`, `lease`, `outbound`,
and `cardinality`, immediately copying the fixed result path into a unique
campaign path. Rig URLs and every server-advertised node must be loopback.

Grade the checked-in archive without changing either datastore:

```bash
make bench-datastore CHECK=baseline
```

The grade fails on missing or extra sidecars, changed bytes, mismatched
digests, inconsistent resources or topology, incomplete fixture cleanup, or
incomparable revisions.

## Interrupted-run recovery

Each lane prints `run_prefix=bench-...` before it creates fixtures. Ctrl-C asks
the lane to stop, waits for its workers, and then runs the normal prefix sweep.
If the process itself is killed before that sweep completes, recover only the
printed prefix:

```bash
make bench-sweep PREFIX=bench-<timestamp>-<process-id> PROFILE=rig
```

The command accepts only the minted prefix alphabet, rechecks the loopback
datastores and their advertised nodes, and deletes only objects carrying that
prefix. It is idempotent.

## Complete when

- `make bench-datastore CHECK=baseline` validates four lanes and twelve samples.
- The active M192 spec records the capture revision and evidence path.
- Remote `dev` and `prod` profiles still refuse before opening a socket until
  their deployment identity can be verified.
