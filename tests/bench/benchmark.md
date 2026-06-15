# Benchmark baseline

Pinned bench numbers per Zig toolchain version, captured on **one machine** so
toolchain upgrades can be measured for regressions.

> **Numbers are machine-specific.** A delta is only valid when both versions are
> measured on the *same host*. To compare an upgrade, re-run the previous
> version's benches in a worktree on the old toolchain — never compare across
> machines.

## How to reproduce

```bash
# Tier-1 micro (no infra needed):
make _bench-micro

# Redis XADD concurrency (8 producer threads; needs a live TLS Redis):
BENCH_REDIS=1 \
  REDIS_URL="rediss://:agentsfleet@localhost:6379" \
  REDIS_TLS_CA_CERT_FILE="$(pwd)/.tmp/redis-ca.crt" \
  make bench-redis
```

Both build with `-Dwith-bench-tools=true -Doptimize=ReleaseFast`. Captured on
macOS (Apple Silicon).

## Tier-1 micro — avg time/run (lower is better)

| benchmark                | zig 0.15.2     | zig 0.16.0 | Δ        |
|--------------------------|----------------|------------|----------|
| route_match              | 634 ns         | 624 ns     | ≈        |
| error_registry_lookup    | 1.346 µs       | 1.337 µs   | ≈        |
| keyset_cursor_roundtrip  | 1.655 ms       | 1.326 ms   | −20%     |
| json_encode_response     | 35.9 µs        | 22.1 µs    | −38%     |
| uuid_v7_generate         | 32 µs (p75) ¹  | 14.3 µs    | −54%     |
| webhook_signature_verify | 1.67 µs (p75)¹ | 1.02 µs    | faster   |
| activity_chunk_encode    | 798 ns         | 502 ns     | −37%     |

¹ 0.15.2 showed millisecond-scale outliers on these two (σ in ms); p75 is the
stable central figure. 0.16.0 variance was tight (σ in ns/µs).

## Redis XADD concurrency — 8 threads × 1000 ops (higher is better)

| metric            | zig 0.15.2     | zig 0.16.0     | Δ      |
|-------------------|----------------|----------------|--------|
| throughput        | 15,090 ops/sec | 17,782 ops/sec | +17.8% |
| wall (8,000 ops)  | 530 ms         | 450 ms         | −15%   |
| per-thread spread | 8 ms           | 6 ms           | tighter|

## Notes

- 0.16.0 is faster than or equal to 0.15.2 on every path — no regression — with
  the largest gains on JSON encode (−38%), the chunk encoder (−37%), and redis
  throughput (+17.8%), plus much lower variance.
- The bench tooling did not compile before this baseline (a missing `auth_codes`
  module dependency in the `bench_app` bridge, which silently broke `make bench`);
  repaired alongside the 0.16 toolchain migration.

## Updating on the next toolchain upgrade

1. Run both suites on the new toolchain (commands above).
2. Re-run the previous version's benches on the **same machine** (worktree on the
   old toolchain) for a valid delta.
3. Add a column for the new version; flag any regression over ~10%.
