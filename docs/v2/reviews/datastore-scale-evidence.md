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
| Capture revision B | `a24d3055d721a7bde8f7294e521a24db991ef4c5` |
| Archive | `bench/baselines/datastore/m192-redis-historical/` |
| Topology fingerprint | `sha256:13b761e01637f8787c055f113480bd0cd56bb273e33548b24263d2e808e9c357` |
| Grade | 4 lanes and 12 samples validated |

The provenance record proves byte equality for production Rust source, schema,
and build inputs. The complete Cargo lock differs only for the benchmark crate;
the normalized production lock and resolved production dependency closure are
equal. Each sample carries the result, raw lane log, raw datastore identity and
topology responses, resource identity, fixture counts, and SHA-256 digests.

Run the fail-closed grade from the repository root:

```bash
make bench-datastore CHECK=baseline
```

The recorded local measurements are:

| Lane | Frozen parameters | Median | Three-sample range |
|---|---|---|---|
| Steer | 50 fleets, concurrency 8, 15-second window | 2,521.30 accepted/s; p95 11.615 ms | 2,181.68–3,237.84 accepted/s; p95 9.143–12.591 ms |
| Lease | 200 fleets, 8 runners, 30-second window | 56.30 polls/s; p95 291.327 ms | 55.03–60.54 polls/s; p95 269.823–318.463 ms |
| Outbound fixture | 200 jobs, one slow destination, 60-second window | 48.49 delivered/s; p95 3,825.663 ms | 46.88–48.75 delivered/s; p95 3,805.183–3,969.023 ms |
| Cardinality | 10,000 fleets | candidate query 1.225 ms; Redis 46,472,496 bytes | query 0.544–1.346 ms; Redis 46,472,496–46,490,264 bytes |

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
