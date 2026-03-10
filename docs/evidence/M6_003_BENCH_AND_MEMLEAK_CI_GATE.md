# M6_003 Bench + Memleak CI Gate Evidence

Date: Mar 10, 2026

## Scope

Evidence for `M6_003` acceptance criterion `5.5`:

- CI executes `memleak` without manual intervention.
- CI executes `bench` without manual intervention.

## CI Wiring

- Memleak workflow: `.github/workflows/memleak.yml`
  - Runs `make memleak` on GitHub Actions (`ubuntu` + Debian container with `valgrind`).
- Bench workflow: `.github/workflows/bench.yml`
  - Starts deterministic local `/healthz` fixture on `127.0.0.1:38080`.
  - Runs `make bench` with CI-safe benchmark settings.
  - Uploads `.tmp/api-bench-*.json` artifacts.

## Threshold Contract Implemented

- Benchmark thresholds encoded in `src/tools/api_bench_runner.zig` (zBench-backed Zig runner):
  - Bench: `error_rate <= 0.01`, `p95 <= 250ms`, `rss_growth <= 128MB`
  - Soak: `error_rate <= 0.02`, `p95 <= 400ms`, `rss_growth <= 256MB`
  - Profile: `error_rate <= 0.02`, `p95 <= 300ms`, `rss_growth <= 192MB`
- `make bench` contract remains stable:
  - `BENCH_MODE=bench|soak|profile`
  - env-configurable thresholds (`API_BENCH_MAX_*`)
  - JSON artifacts under `.tmp/`
  - non-zero exit on gate failure
- Memleak failure contract remains deterministic:
  - Allocator leak test failure returns non-zero.
  - Linux valgrind run uses `--error-exitcode=1`.

## Local Verification Notes

- Command: `API_BENCH_URL=http://127.0.0.1:38080/healthz API_BENCH_DURATION_SEC=6 API_BENCH_CONCURRENCY=12 API_BENCH_MAX_P95_MS=500 make bench`
  - Result: pass (`total=29130 ok=29130 fail=0 timeout=0`, `p95=4.77ms`, `error_rate=0.000000`)
  - Artifact: `.tmp/api-bench-bench-1773161536953.json`
- Command: `API_BENCH_URL=http://127.0.0.1:38080/healthz API_BENCH_DURATION_SEC=4 API_BENCH_CONCURRENCY=8 API_BENCH_MAX_P95_MS=500 make bench BENCH_MODE=profile`
  - Result: pass (`total=21895 ok=21895 fail=0 timeout=0`, `p95=2.80ms`, `error_rate=0.000000`)
  - Artifacts:
    - `.tmp/api-bench-profile-1773161554439.json`
    - `.tmp/api-bench-profile-timeline-1773161554439.json`
- Command: `make memleak`
  - Result: pass (`All 111 tests passed`; macOS leaks gate invoked)
