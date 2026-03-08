#!/usr/bin/env bun

function nowMs() {
  return performance.now();
}

function percentile(sorted, p) {
  if (sorted.length === 0) return 0;
  const idx = Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1));
  return sorted[idx];
}

function toNum(value, fallback) {
  const n = Number(value);
  return Number.isFinite(n) && n > 0 ? n : fallback;
}

function modeDefaults(mode) {
  if (mode === "soak") {
    return { durationSec: 600, concurrency: 12, timeoutMs: 4000, maxErrorRate: 0.02, maxP95Ms: 400 };
  }
  if (mode === "profile") {
    return { durationSec: 45, concurrency: 16, timeoutMs: 3000, maxErrorRate: 0.02, maxP95Ms: 300 };
  }
  return { durationSec: 20, concurrency: 20, timeoutMs: 2500, maxErrorRate: 0.01, maxP95Ms: 250 };
}

async function main() {
  const mode = String(process.env.BENCH_MODE || "bench").toLowerCase();
  if (!["bench", "soak", "profile"].includes(mode)) {
    console.error(`error: invalid BENCH_MODE=${mode}; expected bench|soak|profile`);
    process.exit(2);
  }

  const defaults = modeDefaults(mode);
  const url = String(process.env.API_BENCH_URL || "http://127.0.0.1:3000/healthz");
  const method = String(process.env.API_BENCH_METHOD || "GET").toUpperCase();
  const durationSec = toNum(process.env.API_BENCH_DURATION_SEC, defaults.durationSec);
  const concurrency = Math.floor(toNum(process.env.API_BENCH_CONCURRENCY, defaults.concurrency));
  const timeoutMs = Math.floor(toNum(process.env.API_BENCH_TIMEOUT_MS, defaults.timeoutMs));
  const maxErrorRate = Number(process.env.API_BENCH_MAX_ERROR_RATE ?? defaults.maxErrorRate);
  const maxP95Ms = Number(process.env.API_BENCH_MAX_P95_MS ?? defaults.maxP95Ms);

  const startedAt = Date.now();
  const deadline = startedAt + durationSec * 1000;
  const latencies = [];
  const timeline = [];

  let ok = 0;
  let fail = 0;
  let timeout = 0;
  let inFlight = 0;

  let secWindowStart = Date.now();
  let secWindowTotal = 0;
  let secWindowFail = 0;

  async function requestOnce() {
    inFlight += 1;
    secWindowTotal += 1;

    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), timeoutMs);
    const t0 = nowMs();

    try {
      const res = await fetch(url, {
        method,
        signal: ctrl.signal,
      });
      const dt = nowMs() - t0;
      latencies.push(dt);

      if (res.ok) {
        ok += 1;
      } else {
        fail += 1;
        secWindowFail += 1;
      }
    } catch (err) {
      const dt = nowMs() - t0;
      latencies.push(dt);
      fail += 1;
      secWindowFail += 1;
      if (err && err.name === "AbortError") timeout += 1;
    } finally {
      clearTimeout(timer);
      inFlight -= 1;

      const now = Date.now();
      if (now - secWindowStart >= 1000) {
        timeline.push({
          ts: new Date(now).toISOString(),
          in_flight: inFlight,
          req: secWindowTotal,
          fail: secWindowFail,
        });
        secWindowStart = now;
        secWindowTotal = 0;
        secWindowFail = 0;
      }
    }
  }

  async function workerLoop() {
    while (Date.now() < deadline) {
      await requestOnce();
    }
  }

  const workers = Array.from({ length: concurrency }, () => workerLoop());
  await Promise.all(workers);

  const endedAt = Date.now();
  const total = ok + fail;
  const errorRate = total > 0 ? fail / total : 1;
  const sorted = latencies.slice().sort((a, b) => a - b);

  const summary = {
    mode,
    config: {
      url,
      method,
      duration_sec: durationSec,
      concurrency,
      timeout_ms: timeoutMs,
      max_error_rate: maxErrorRate,
      max_p95_ms: maxP95Ms,
    },
    counts: {
      total,
      ok,
      fail,
      timeout,
    },
    latency_ms: {
      p50: Number(percentile(sorted, 50).toFixed(2)),
      p95: Number(percentile(sorted, 95).toFixed(2)),
      p99: Number(percentile(sorted, 99).toFixed(2)),
      max: Number((sorted[sorted.length - 1] || 0).toFixed(2)),
    },
    rates: {
      req_per_sec: Number((total / Math.max(1, durationSec)).toFixed(2)),
      error_rate: Number(errorRate.toFixed(6)),
    },
    started_at: new Date(startedAt).toISOString(),
    ended_at: new Date(endedAt).toISOString(),
    passed: errorRate <= maxErrorRate && percentile(sorted, 95) <= maxP95Ms,
  };

  const stamp = new Date(startedAt).toISOString().replace(/[:.]/g, "-");
  const outPath = `.tmp/api-bench-${mode}-${stamp}.json`;
  await Bun.write(outPath, `${JSON.stringify(summary, null, 2)}\n`);

  if (mode === "profile") {
    const timelinePath = `.tmp/api-bench-profile-timeline-${stamp}.json`;
    await Bun.write(timelinePath, `${JSON.stringify({ timeline }, null, 2)}\n`);
    summary.profile_timeline_path = timelinePath;
  }

  console.log(`mode=${mode} url=${url} duration=${durationSec}s concurrency=${concurrency}`);
  console.log(`total=${summary.counts.total} ok=${summary.counts.ok} fail=${summary.counts.fail} timeout=${summary.counts.timeout}`);
  console.log(`latency_ms p50=${summary.latency_ms.p50} p95=${summary.latency_ms.p95} p99=${summary.latency_ms.p99} max=${summary.latency_ms.max}`);
  console.log(`req_per_sec=${summary.rates.req_per_sec} error_rate=${summary.rates.error_rate}`);
  console.log(`artifact=${outPath}`);
  if (summary.profile_timeline_path) console.log(`profile_timeline=${summary.profile_timeline_path}`);

  if (!summary.passed) {
    console.error(
      `error: bench gate failed (error_rate=${summary.rates.error_rate} > ${maxErrorRate} or p95=${summary.latency_ms.p95}ms > ${maxP95Ms}ms)`,
    );
    process.exit(1);
  }
}

main().catch((err) => {
  console.error(`error: ${String(err)}`);
  process.exit(1);
});
