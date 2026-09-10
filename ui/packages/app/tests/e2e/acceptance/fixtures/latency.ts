import { expect, type Page, type TestInfo } from "@playwright/test";
import type { WorkspaceFetchAuditSnapshot } from "@/lib/acceptance/workspace-fetch-audit";

/**
 * Measurement support for the dashboard latency lane.
 *
 * Two rules the surrounding spec depends on, enforced here so no individual
 * measurement can forget one:
 *  - A disabled audit FAILS rather than skipping. Against a deployed target the
 *    env gate may be unset, every record call is then a no-op, and the counts
 *    read zero — a zero-count inventory is indistinguishable from a page that
 *    issued nothing, so publishing it would be worse than not measuring.
 *  - A non-zero pre-count FAILS rather than being subtracted. The counter is
 *    module-global in the app server, so another spec's reads landing in this
 *    window mean the sample is not this page's and cannot be repaired by
 *    arithmetic.
 */

export const AUDIT_URL = "/acceptance-audit/workspace-fetches";

// Every published figure comes from this many navigations; a percentile over
// fewer is a rumour. `firstMs` is reported apart from the percentiles because
// the first navigation pays compile and connection costs the rest do not.
export const LATENCY_SAMPLE_COUNT = 5;

const NOT_FOUND_STATUS = 404;
const P50 = 50;
const P95 = 95;
const PERCENT_MAX = 100;
const EMPTY_TOTAL = 0;
const AUDIT_HEADERS = {
  "x-acceptance-token":
    process.env.AGENTSFLEET_E2E_AUDIT_TOKEN ?? "local-acceptance-audit-token",
} as const;

export type StageSummary = {
  stage: string;
  samples: number;
  firstMs: number;
  p50Ms: number;
  p95Ms: number;
};

export async function readAudit(page: Page): Promise<WorkspaceFetchAuditSnapshot> {
  const response = await page.request.get(AUDIT_URL, { headers: AUDIT_HEADERS });
  expect(response.ok(), "the acceptance audit must answer").toBe(true);
  return (await response.json()) as WorkspaceFetchAuditSnapshot;
}

/**
 * Clears the counter and proves it is both reachable and zeroed. Call before
 * every navigation whose reads are counted.
 */
export async function resetAuditOrFail(page: Page): Promise<void> {
  const response = await page.request.post(AUDIT_URL, { headers: AUDIT_HEADERS });
  expect(
    response.status(),
    "the audit route is disabled — set AGENTSFLEET_E2E_AUDIT=1 on the app server; " +
      "measuring against a disabled audit publishes zeros",
  ).not.toBe(NOT_FOUND_STATUS);
  expect(response.ok(), "the audit reset must succeed").toBe(true);

  const snapshot = (await response.json()) as WorkspaceFetchAuditSnapshot;
  expect(
    snapshot.total,
    "another test's reads are in the counter; this sample is not this page's",
  ).toBe(EMPTY_TOTAL);
}

/** Nearest-rank percentile, so every reported figure is a sample that happened. */
export function percentileMs(samples: readonly number[], percentile: number): number {
  expect(samples.length, "a percentile needs at least one sample").toBeGreaterThan(0);
  const ordered = [...samples].sort((a, b) => a - b);
  const rank = Math.ceil((percentile / PERCENT_MAX) * ordered.length);
  const index = Math.min(Math.max(rank, 1), ordered.length) - 1;
  return ordered[index] ?? ordered[ordered.length - 1] ?? 0;
}

export function summarizeStage(stage: string, samples: readonly number[]): StageSummary {
  expect(
    samples.length,
    `${stage}: ${samples.length} samples collected, ${LATENCY_SAMPLE_COUNT} declared`,
  ).toBe(LATENCY_SAMPLE_COUNT);
  return {
    stage,
    samples: samples.length,
    firstMs: samples[0] ?? 0,
    p50Ms: percentileMs(samples, P50),
    p95Ms: percentileMs(samples, P95),
  };
}

/** Times one awaited step, returning its value beside the wall time it cost. */
export async function timed<T>(step: () => Promise<T>): Promise<{ value: T; durationMs: number }> {
  const startedAt = Date.now();
  const value = await step();
  return { value, durationMs: Date.now() - startedAt };
}

/**
 * Attaches a stage table to the run. The architecture page quotes these rows,
 * so the run that produced a number is always recoverable from the report.
 */
export async function attachStageTable(
  testInfo: TestInfo,
  title: string,
  rows: readonly StageSummary[],
): Promise<void> {
  const header = "| Stage | Samples | First (ms) | p50 (ms) | p95 (ms) |\n|---|---|---|---|---|";
  const body = rows
    .map((r) => `| ${r.stage} | ${r.samples} | ${r.firstMs} | ${r.p50Ms} | ${r.p95Ms} |`)
    .join("\n");
  await testInfo.attach(title, {
    body: `### ${title}\n\n${header}\n${body}\n`,
    contentType: "text/markdown",
  });
}
