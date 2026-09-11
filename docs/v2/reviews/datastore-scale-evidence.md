---
type: reference
audience: contributor
verified: 2026-09-12
product_version: 0.30.0
executable: true
---

# Datastore scale evidence

This index records the immutable evidence produced while executing M192. It is
an input to readiness decisions; a historical Redis measurement does not prove
Dragonfly behavior or set a future pass budget.

## Redis historical campaign

| Field | Value |
|---|---|
| Campaign | `m192-redis-historical` |
| Environment | Repository-owned local rig on macOS arm64, 8 logical CPUs, 16 GiB memory |
| Baseline revision B0 | `521ca4037ebbd23056f8b3b63dcf9c2fa34f650d` |
| Capture revision B | `f75e10d4e0ffe1e73fccbd9e3aac99681c9c7886` |
| Evidence revision E | `5eb0c2044ac15e6fa8f2204541fe3ebaf8aaa08f` |
| Archive | `bench/baselines/datastore/m192-redis-historical/` |
| Topology fingerprint | `sha256:13b761e01637f8787c055f113480bd0cd56bb273e33548b24263d2e808e9c357` |
| Grade | 4 lanes and 12 samples validated |

The provenance record proves byte equality for production Rust source, schema,
and build inputs. The complete Cargo lock differs only for the benchmark crate;
the normalized production lock and resolved production dependency closure are
equal. Each sample carries the result, raw lane log, raw datastore identity and
topology responses, resource identity, fixture counts, and SHA-256 digests.
The grader also compares every archived byte with evidence revision E, so a
coordinated result and sidecar rewrite cannot create a new accepted history.

Run the fail-closed grade from the repository root:

```bash
make bench-datastore CHECK=baseline
```

The recorded local measurements are:

| Lane | Frozen parameters | Median | Three-sample range |
|---|---|---|---|
| Steer | 50 fleets, concurrency 8, 15-second window | 10,012.88 accepted/s; p95 1.265 ms | 9,822.39–10,176.39 accepted/s; p95 1.215–1.267 ms |
| Lease | 200 fleets, 8 runners, 30-second window | 78.58 polls/s; p95 214.143 ms | 71.65–86.17 polls/s; p95 212.479–263.935 ms |
| Outbound fixture | 200 jobs, one slow destination, 60-second window | 48.40 delivered/s; p95 3,837.951 ms | 48.22–48.57 delivered/s; p95 3,823.615–3,860.479 ms |
| Cardinality | 10,000 fleets | candidate query 0.606 ms; Redis 46,472,496 bytes | query 0.544–1.429 ms; Redis 46,472,496 bytes |

Every archived report completed without an abort and swept at least as many
fixtures as it created. The grader also requires identical machine resources,
topology, and lane parameters across each lane's samples, rejects extra or
missing sidecars, and regenerates provenance from Git before accepting the
archive.

## Remaining M192 evidence

Dragonfly prototype, cluster, durability, retention, coordination, Cloud,
rehearsal, and rollout evidence is not yet recorded. Those gates remain open in
the active M192 specification, and no development or production datastore was
contacted by this campaign.
