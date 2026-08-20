const ACCEPTANCE_LANE = {
  deterministic: "deterministic",
  live: "live",
} as const;

const TERMINAL_OUTCOME = {
  passed: "passed",
  failed: "failed",
  preflightFailed: "preflight_failed",
  summaryMissing: "summary_missing",
} as const;

const BUN_TEST_TIMEOUT_MS = 120_000;
const SUMMARY_TAIL_MAX_CHARACTERS = 64 * 1024;
const PREFLIGHT_ENTRYPOINT = "test/acceptance/global-setup.ts";
const LIVE_TEARDOWN_PRELOAD = "./test/acceptance/global-teardown.ts";
export const LIVE_FILE_CONCURRENCY = 2;
export const LIVE_HANDSHAKE_FILE = "test/acceptance/lifecycle-after-login.spec.ts";
export const LIVE_SERIAL_FILE = "test/acceptance/tenant-provider-mutation.spec.ts";
const SUMMARY_EVENT = "cli_acceptance_lane_summary";
const REGISTERED_RE = /Ran\s+(\d+)\s+tests?\b/;
const PASSED_RE = /(\d+)\s+pass\b/;
const FAILED_RE = /(\d+)\s+fail\b/;
const SKIPPED_RE = /(\d+)\s+skip\b/;

type AcceptanceLane = (typeof ACCEPTANCE_LANE)[keyof typeof ACCEPTANCE_LANE];

export interface LaneCounts {
  readonly registered: number;
  readonly passed: number;
  readonly failed: number;
  readonly skipped: number;
}

export const DETERMINISTIC_ACCEPTANCE_FILES: ReadonlyArray<string> = [
  "test/acceptance/acceptance-lanes.test.ts",
  "test/acceptance/argument-negatives.spec.ts",
  "test/acceptance/fixtures/clerk-admin.test.ts",
  "test/acceptance/fixtures/workspace-hydration.test.ts",
  "test/acceptance/flags-and-env.spec.ts",
  "test/acceptance/help-and-errors.spec.ts",
  "test/acceptance/memory-read.spec.ts",
  "test/acceptance/options-metavar.spec.ts",
  "test/acceptance/streaming-follow.spec.ts",
];

export const LIVE_ACCEPTANCE_FILES: ReadonlyArray<string> = [
  "test/acceptance/concurrency.spec.ts",
  "test/acceptance/fleet-update-delete.spec.ts",
  "test/acceptance/install-negatives.spec.ts",
  "test/acceptance/lifecycle-after-login.spec.ts",
  "test/acceptance/lifecycle-with-token.spec.ts",
  "test/acceptance/login-negatives.spec.ts",
  "test/acceptance/logs-events-live.spec.ts",
  "test/acceptance/perf.spec.ts",
  "test/acceptance/referential-integrity.spec.ts",
  "test/acceptance/secret-vault.spec.ts",
  "test/acceptance/steer-live.spec.ts",
  "test/acceptance/tenant-provider-mutation.spec.ts",
  "test/acceptance/workspace-mutation.spec.ts",
];

interface CommandResult {
  readonly exitCode: number;
  readonly outputTail: string;
}

interface LaneResult {
  readonly exitCode: number;
  readonly counts: LaneCounts | null;
}

export interface LiveExecutionPlan {
  readonly handshake: string;
  readonly parallel: ReadonlyArray<string>;
  readonly serial: string;
}

export function parseLaneCounts(output: string): LaneCounts | null {
  const registered = readCount(output, REGISTERED_RE);
  const passed = readCount(output, PASSED_RE);
  const failed = readCount(output, FAILED_RE);
  if (registered === null || passed === null || failed === null) return null;
  return {
    registered,
    passed,
    failed,
    skipped: readCount(output, SKIPPED_RE) ?? 0,
  };
}

function readCount(output: string, pattern: RegExp): number | null {
  const value = output.match(pattern)?.[1];
  if (value === undefined) return null;
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : null;
}

function acceptanceFiles(lane: AcceptanceLane): ReadonlyArray<string> {
  return lane === ACCEPTANCE_LANE.deterministic
    ? DETERMINISTIC_ACCEPTANCE_FILES
    : LIVE_ACCEPTANCE_FILES;
}

export function liveExecutionPlan(): LiveExecutionPlan {
  return {
    handshake: LIVE_HANDSHAKE_FILE,
    parallel: LIVE_ACCEPTANCE_FILES.filter((file) =>
      file !== LIVE_HANDSHAKE_FILE && file !== LIVE_SERIAL_FILE
    ),
    serial: LIVE_SERIAL_FILE,
  };
}

function parseLane(value: string | undefined): AcceptanceLane {
  if (value === ACCEPTANCE_LANE.deterministic || value === ACCEPTANCE_LANE.live) {
    return value;
  }
  throw new Error(`acceptance lane must be ${ACCEPTANCE_LANE.deterministic} or ${ACCEPTANCE_LANE.live}`);
}

async function executeAndStream(command: ReadonlyArray<string>): Promise<CommandResult> {
  const child = Bun.spawn([...command], {
    cwd: process.cwd(),
    env: process.env,
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdoutTail, stderrTail, exitCode] = await Promise.all([
    streamTail(child.stdout, process.stdout),
    streamTail(child.stderr, process.stderr),
    child.exited,
  ]);
  return { exitCode, outputTail: `${stdoutTail}\n${stderrTail}` };
}

async function runTestFile(file: string): Promise<LaneResult> {
  const result = await executeAndStream([
    "bun",
    "test",
    "--preload",
    LIVE_TEARDOWN_PRELOAD,
    file,
    "--timeout",
    String(BUN_TEST_TIMEOUT_MS),
  ]);
  return {
    exitCode: result.exitCode,
    counts: parseLaneCounts(result.outputTail),
  };
}

async function runBoundedFiles(
  files: ReadonlyArray<string>,
  concurrency: number,
): Promise<ReadonlyArray<LaneResult>> {
  const results: LaneResult[] = [];
  let nextIndex = 0;
  async function worker(): Promise<void> {
    while (nextIndex < files.length) {
      const index = nextIndex++;
      const file = files[index];
      if (file) results[index] = await runTestFile(file);
    }
  }
  await Promise.all(
    Array.from({ length: Math.min(concurrency, files.length) }, worker),
  );
  return results;
}

function combineLaneResults(results: ReadonlyArray<LaneResult>): LaneResult {
  let registered = 0;
  let passed = 0;
  let failed = 0;
  let skipped = 0;
  for (const result of results) {
    if (result.counts === null) return { exitCode: 1, counts: null };
    registered += result.counts.registered;
    passed += result.counts.passed;
    failed += result.counts.failed;
    skipped += result.counts.skipped;
  }
  return {
    exitCode: results.every((result) => result.exitCode === 0) ? 0 : 1,
    counts: { registered, passed, failed, skipped },
  };
}

async function streamTail(
  stream: ReadableStream<Uint8Array>,
  destination: NodeJS.WriteStream,
): Promise<string> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let tail = "";
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      destination.write(value);
      tail = `${tail}${decoder.decode(value, { stream: true })}`
        .slice(-SUMMARY_TAIL_MAX_CHARACTERS);
    }
    return `${tail}${decoder.decode()}`.slice(-SUMMARY_TAIL_MAX_CHARACTERS);
  } finally {
    reader.releaseLock();
  }
}

function writeSummary(
  lane: AcceptanceLane,
  counts: LaneCounts,
  durationMs: number,
  terminalOutcome: string,
): void {
  process.stdout.write(
    `${SUMMARY_EVENT} lane=${lane} registered=${counts.registered} passed=${counts.passed} ` +
      `failed=${counts.failed} skipped=${counts.skipped} duration_ms=${durationMs} ` +
      `terminal_outcome=${terminalOutcome}\n`,
  );
}

async function runLane(lane: AcceptanceLane): Promise<number> {
  const startedAt = Date.now();
  if (lane === ACCEPTANCE_LANE.live) {
    const preflight = await executeAndStream(["bun", PREFLIGHT_ENTRYPOINT]);
    if (preflight.exitCode !== 0) {
      writeSummary(lane, { registered: 0, passed: 0, failed: 1, skipped: 0 },
        Date.now() - startedAt, TERMINAL_OUTCOME.preflightFailed);
      return preflight.exitCode;
    }
  }

  let result: LaneResult;
  if (lane === ACCEPTANCE_LANE.live) {
    const plan = liveExecutionPlan();
    const handshake = await runTestFile(plan.handshake);
    if (handshake.exitCode !== 0 || handshake.counts === null) {
      const counts = handshake.counts ?? { registered: 0, passed: 0, failed: 1, skipped: 0 };
      writeSummary(lane, counts, Date.now() - startedAt, TERMINAL_OUTCOME.failed);
      return 1;
    }
    result = combineLaneResults([
        handshake,
        ...await runBoundedFiles(
          plan.parallel,
          LIVE_FILE_CONCURRENCY,
        ),
        await runTestFile(plan.serial),
      ]);
  } else {
    const deterministicResult = await executeAndStream([
        "bun",
        "test",
        ...acceptanceFiles(lane),
        "--timeout",
        String(BUN_TEST_TIMEOUT_MS),
      ]);
    result = {
      exitCode: deterministicResult.exitCode,
      counts: parseLaneCounts(deterministicResult.outputTail),
    };
  }
  const counts = result.counts;
  if (counts === null) {
    writeSummary(lane, { registered: 0, passed: 0, failed: 1, skipped: 0 },
      Date.now() - startedAt, TERMINAL_OUTCOME.summaryMissing);
    return 1;
  }
  writeSummary(lane, counts, Date.now() - startedAt,
    result.exitCode === 0 ? TERMINAL_OUTCOME.passed : TERMINAL_OUTCOME.failed);
  return result.exitCode;
}

if (import.meta.main) {
  try {
    process.exitCode = await runLane(parseLane(process.argv[2]));
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    process.stderr.write(`${message}\n`);
    process.exitCode = 1;
  }
}
