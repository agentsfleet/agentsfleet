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
| Capture revision B | `71b58da9345786b8918411b4035c08159abbf166` |
| Archive | `bench/baselines/datastore/m192-redis-historical/` |
| Topology fingerprint | `sha256:4e6aa76cdf9726d15071c4f51b9f4f7e6a09bcbf08cfdbd0e895f073e3f135e0` |
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
| Steer | 50 fleets, concurrency 8, 15-second window | 12,854.09 accepted/s; p95 0.923 ms | 12,012.30–14,532.89 accepted/s; p95 0.820–1.044 ms |
| Lease | 200 fleets, 8 runners, 30-second window | 141.19 polls/s; p95 100.671 ms | 93.67–141.78 polls/s; p95 99.455–196.735 ms |
| Outbound fixture | 200 jobs, one slow destination, 60-second window | 48.56 delivered/s; p95 3,823.615 ms | 48.42–48.60 delivered/s; p95 3,821.567–3,837.951 ms |
| Cardinality | 10,000 fleets | candidate query 0.584 ms; Redis 46,472,496 bytes | query 0.435–0.639 ms; Redis bytes identical in all samples |

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
