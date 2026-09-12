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
| Capture revision B | `a4b38b6a7fca0ecc17d3ac516cfe62f036fb0bc3` |
| Evidence revision E | `84016ca667511c7e5e0758721a8c787d46d01f94` |
| Archive | `bench/baselines/datastore/m192-redis-historical/` |
| Topology fingerprint | `sha256:13b761e01637f8787c055f113480bd0cd56bb273e33548b24263d2e808e9c357` |
| Grade | 4 lanes and 12 samples validated |

The provenance record proves byte equality for production Rust source except
`afd_outbound/src/poster.rs`, whose exact SHA-256-pinned pre-dispatch ownership
seam defaults to accepting every entry and has a production behavior test.
Schema and build inputs are byte-equal. The complete Cargo lock differs only for the benchmark crate;
the normalized production lock and resolved production dependency closure are
equal. Each sample carries the result, raw lane log, raw datastore identity and
topology responses, resource identity, fixture counts, and SHA-256 digests.
The grader replays every scalar and series value from the recorded raw
operands, histograms, duration samples, and original datastore timing lines,
allowing only four machine-epsilon units of relative serialization noise. It compares
every archived byte with evidence revision E, so a coordinated result and
sidecar rewrite cannot create a new accepted history. The local rig holds an
exclusive process lock and checks for external Postgres/Redis clients at each
run and archive boundary. The request fixture is 18 bytes; the outbound
response is 12 bytes.

Run the fail-closed grade from the repository root:

```bash
make bench-datastore CHECK=baseline
```

The recorded local measurements are:

| Lane | Frozen parameters | Median | Three-sample range |
|---|---|---|---|
| Steer | 50 fleets, concurrency 8, 15-second window | 10,370.76 accepted/s; p95 1.187 ms | 10,183.92–10,731.15 accepted/s; p95 1.158–1.223 ms |
| Lease | 200 fleets, 8 runners, 30-second window | 78.61 polls/s; p95 228.607 ms | 71.13–82.21 polls/s; p95 203.391–251.775 ms |
| Outbound fixture | 200 jobs, one slow destination, 60-second window | 48.53 delivered/s; p95 3,827.711 ms | 48.45–48.56 delivered/s; p95 3,825.663–3,831.807 ms |
| Cardinality | 10,000 fleets | candidate query 1.306 ms; Redis 46,472,496 bytes | query 0.702–1.623 ms; Redis 46,472,496 bytes |

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
