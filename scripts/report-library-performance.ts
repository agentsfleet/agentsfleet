#!/usr/bin/env bun
/**
 * Validate a pair of library-performance capture reports.
 *
 * This command decides ONE question: are these two reports structurally sound
 * and mutually comparable? It never decides whether performance got better or
 * worse. p50, p95, p99 and payload sizes are read, range-checked for internal
 * consistency, and otherwise carried through untouched.
 *
 * That separation is the whole point. A latency threshold in a universal check
 * fails on a noisy runner, gets widened until it cannot fail, and then reports
 * success forever — so the check that runs everywhere validates STRUCTURE, and
 * the numbers are evidence a human reads. Capture may fail for setup,
 * execution, schema, sanitization, or output correctness. It may not fail
 * because a percentile moved.
 *
 * Comparability is stricter than validity on purpose: two reports taken under
 * different pool sizes, warm states, or concurrency are both valid and are not
 * comparable, and silently diffing them is how a configuration change gets read
 * as a regression.
 *
 * Usage:
 *   bun scripts/report-library-performance.ts --check \
 *     --baseline test-results/library-performance/baseline.json \
 *     --candidate test-results/library-performance/candidate.json
 */

const SCHEMA_VERSION = 1;

/** Exit codes. Distinct so a caller can tell "bad input" from "not comparable". */
const EXIT_OK = 0;
const EXIT_USAGE = 2;
const EXIT_INVALID = 3;

const FLAG_CHECK = "--check";
const FLAG_BASELINE = "--baseline";
const FLAG_CANDIDATE = "--candidate";

const OK_LINE = "comparison=valid";

/** `typeof` tags, named because each is compared at more than one site. */
const T_STRING = "string";
const T_NUMBER = "number";

/** Which report a diagnostic is about. */
const LABEL_BASELINE = "baseline";
const LABEL_CANDIDATE = "candidate";
const WHERE_METADATA = "metadata";

/**
 * `timeout` and `cancelled` are the same wire word in two different closed sets
 * — an outcome of a read and a result of a pool acquire. Named once so the two
 * sets cannot drift to different spellings of the same condition.
 */
const V_TIMEOUT = "timeout";
const V_CANCELLED = "cancelled";

/**
 * The closed label sets, mirroring `observability/library_stages.zig`. Kept as
 * `as const` arrays so narrowing survives and a value outside them is a parse
 * failure rather than a silently accepted new series.
 *
 * `fleet_detail` is absent from SURFACES deliberately — that route was stripped
 * unconsumed, so a report naming it was produced by something that no longer
 * exists and is not a report of this system.
 */
const SURFACES = ["tenant_models", "global_models", "fleet_summary"] as const;
const STAGES = [
  "next_upstream",
  "auth_verify",
  "pool_wait",
  "authorize",
  "sql",
  "secret_project",
  "map",
  "serialize",
  "cache_revision",
  "cache_lookup",
] as const;
const OUTCOMES = [
  "ok",
  "invalid",
  "unauthorized",
  "forbidden",
  "not_found",
  V_TIMEOUT,
  V_CANCELLED,
  "dependency_error",
  "internal_error",
] as const;
const CACHE_VALUES = ["hit", "miss", "bypass", "stale", "not_applicable"] as const;
const POOL_RESULTS = ["acquired", V_TIMEOUT, V_CANCELLED, "error"] as const;

const REGION_CLASSES = ["local", "single_region", "multi_region"] as const;
const WARM_STATES = ["cold", "warm"] as const;

/**
 * Every metadata key, spelled exactly once. The parser and the comparability
 * check both read this table, so a field cannot be validated under one spelling
 * and compared under another.
 */
const META_KEY = {
  fixture_sha256: "fixture_sha256",
  build_profile: "build_profile",
  database_version: "database_version",
  pool_size: "pool_size",
  replica_count: "replica_count",
  region_class: "region_class",
  warm_state: "warm_state",
  concurrency: "concurrency",
} as const;

/** Metadata fields that must match byte-for-byte for two runs to be comparable. */
const METADATA_FIELDS = Object.values(META_KEY);

type Surface = (typeof SURFACES)[number];
type Stage = (typeof STAGES)[number];
type Outcome = (typeof OUTCOMES)[number];
type CacheValue = (typeof CACHE_VALUES)[number];
type PoolResult = (typeof POOL_RESULTS)[number];

type Metadata = {
  fixture_sha256: string;
  build_profile: string;
  database_version: string;
  pool_size: number;
  replica_count: number;
  region_class: (typeof REGION_CLASSES)[number];
  warm_state: (typeof WARM_STATES)[number];
  concurrency: number;
};

type Aggregate = {
  surface: Surface;
  stage: Stage;
  outcome: Outcome;
  cache: CacheValue;
  pool_result: PoolResult;
  sample_count: number;
  p50_seconds: number;
  p95_seconds: number;
  p99_seconds: number;
  payload_bytes: number;
};

type Report = {
  schema_version: number;
  commit_sha: string;
  metadata: Metadata;
  aggregates: Aggregate[];
};

/**
 * Tagged union rather than `{ ok: boolean; error?: string }` — the optional
 * field shape lets a caller read `.value` on a failure and get `undefined`,
 * which is how a validation failure turns into a downstream crash far from
 * its cause.
 */
type Parsed<T> = { ok: true; value: T } | { ok: false; error: string };

function fail<T>(error: string): Parsed<T> {
  return { ok: false, error };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readString(source: Record<string, unknown>, key: string, where: string): Parsed<string> {
  const raw = source[key];
  if (typeof raw !== T_STRING || raw.length === 0) {
    return fail(`${where}.${key} must be a non-empty string`);
  }
  return { ok: true, value: raw };
}

/**
 * A count or a size: integral and never negative. Rejects NaN and Infinity,
 * which `typeof x === "number"` alone happily admits and which would then
 * propagate through every comparison as a silently true one.
 */
function readNonNegativeInt(
  source: Record<string, unknown>,
  key: string,
  where: string,
): Parsed<number> {
  const raw = source[key];
  if (typeof raw !== T_NUMBER || !Number.isInteger(raw) || raw < 0) {
    return fail(`${where}.${key} must be a non-negative integer`);
  }
  return { ok: true, value: raw };
}

function readSeconds(
  source: Record<string, unknown>,
  key: string,
  where: string,
): Parsed<number> {
  const raw = source[key];
  if (typeof raw !== T_NUMBER || !Number.isFinite(raw) || raw < 0) {
    return fail(`${where}.${key} must be a finite non-negative number`);
  }
  return { ok: true, value: raw };
}

function readEnum<T extends string>(
  source: Record<string, unknown>,
  key: string,
  permitted: readonly T[],
  where: string,
): Parsed<T> {
  const raw = source[key];
  if (typeof raw !== T_STRING || !permitted.includes(raw as T)) {
    return fail(`${where}.${key} must be one of: ${permitted.join(", ")}`);
  }
  return { ok: true, value: raw as T };
}

function parseMetadata(raw: unknown): Parsed<Metadata> {
  if (!isRecord(raw)) return fail(`${WHERE_METADATA} must be an object`);

  const fixture = readString(raw, META_KEY.fixture_sha256, WHERE_METADATA);
  if (!fixture.ok) return fixture;
  const profile = readString(raw, META_KEY.build_profile, WHERE_METADATA);
  if (!profile.ok) return profile;
  const database = readString(raw, META_KEY.database_version, WHERE_METADATA);
  if (!database.ok) return database;
  const poolSize = readNonNegativeInt(raw, META_KEY.pool_size, WHERE_METADATA);
  if (!poolSize.ok) return poolSize;
  const replicas = readNonNegativeInt(raw, META_KEY.replica_count, WHERE_METADATA);
  if (!replicas.ok) return replicas;
  const region = readEnum(raw, META_KEY.region_class, REGION_CLASSES, WHERE_METADATA);
  if (!region.ok) return region;
  const warm = readEnum(raw, META_KEY.warm_state, WARM_STATES, WHERE_METADATA);
  if (!warm.ok) return warm;
  const concurrency = readNonNegativeInt(raw, META_KEY.concurrency, WHERE_METADATA);
  if (!concurrency.ok) return concurrency;

  return {
    ok: true,
    value: {
      fixture_sha256: fixture.value,
      build_profile: profile.value,
      database_version: database.value,
      pool_size: poolSize.value,
      replica_count: replicas.value,
      region_class: region.value,
      warm_state: warm.value,
      concurrency: concurrency.value,
    },
  };
}

function parseAggregate(raw: unknown, index: number): Parsed<Aggregate> {
  const where = `aggregates[${index}]`;
  if (!isRecord(raw)) return fail(`${where} must be an object`);

  const surface = readEnum(raw, "surface", SURFACES, where);
  if (!surface.ok) return surface;
  const stage = readEnum(raw, "stage", STAGES, where);
  if (!stage.ok) return stage;
  const outcome = readEnum(raw, "outcome", OUTCOMES, where);
  if (!outcome.ok) return outcome;
  const cache = readEnum(raw, "cache", CACHE_VALUES, where);
  if (!cache.ok) return cache;
  const poolResult = readEnum(raw, "pool_result", POOL_RESULTS, where);
  if (!poolResult.ok) return poolResult;

  const sampleCount = readNonNegativeInt(raw, "sample_count", where);
  if (!sampleCount.ok) return sampleCount;
  // Strictly positive: an aggregate row describing zero samples has no
  // percentiles to describe, so its p-values would be fabricated.
  if (sampleCount.value === 0) return fail(`${where}.sample_count must be positive`);

  const p50 = readSeconds(raw, "p50_seconds", where);
  if (!p50.ok) return p50;
  const p95 = readSeconds(raw, "p95_seconds", where);
  if (!p95.ok) return p95;
  const p99 = readSeconds(raw, "p99_seconds", where);
  if (!p99.ok) return p99;

  // Ordering is an INTERNAL-CONSISTENCY check, not a threshold: it asks
  // whether these three numbers can describe one distribution, never whether
  // that distribution is fast enough.
  if (!(p50.value <= p95.value && p95.value <= p99.value)) {
    return fail(`${where} percentiles must satisfy p50 <= p95 <= p99`);
  }

  const payload = readNonNegativeInt(raw, "payload_bytes", where);
  if (!payload.ok) return payload;

  return {
    ok: true,
    value: {
      surface: surface.value,
      stage: stage.value,
      outcome: outcome.value,
      cache: cache.value,
      pool_result: poolResult.value,
      sample_count: sampleCount.value,
      p50_seconds: p50.value,
      p95_seconds: p95.value,
      p99_seconds: p99.value,
      payload_bytes: payload.value,
    },
  };
}

/** The tuple that identifies one aggregate row across two runs. */
export function aggregateKey(a: Aggregate): string {
  return [a.surface, a.stage, a.outcome, a.cache, a.pool_result].join("|");
}

export function parseReport(raw: unknown, label: string): Parsed<Report> {
  if (!isRecord(raw)) return fail(`${label} must be a JSON object`);

  if (raw.schema_version !== SCHEMA_VERSION) {
    return fail(`${label}.schema_version must be ${SCHEMA_VERSION}`);
  }
  const commit = readString(raw, "commit_sha", label);
  if (!commit.ok) return commit;

  const metadata = parseMetadata(raw[WHERE_METADATA]);
  if (!metadata.ok) return fail(`${label}: ${metadata.error}`);

  if (!Array.isArray(raw.aggregates)) return fail(`${label}.aggregates must be an array`);
  if (raw.aggregates.length === 0) return fail(`${label}.aggregates must not be empty`);

  const aggregates: Aggregate[] = [];
  const seen = new Set<string>();
  for (const [index, entry] of raw.aggregates.entries()) {
    const parsed = parseAggregate(entry, index);
    if (!parsed.ok) return fail(`${label}: ${parsed.error}`);
    const key = aggregateKey(parsed.value);
    // A repeated key means two rows claim the same series. Merging them here
    // would invent a number neither row reported.
    if (seen.has(key)) return fail(`${label}: duplicate aggregate key ${key}`);
    seen.add(key);
    aggregates.push(parsed.value);
  }

  return {
    ok: true,
    value: {
      schema_version: SCHEMA_VERSION,
      commit_sha: commit.value,
      metadata: metadata.value,
      aggregates,
    },
  };
}

/**
 * Decide comparability. Returns the list of reasons the pair is NOT comparable;
 * empty means it is.
 *
 * No timing or payload value appears in any condition here — that absence is
 * the property `test_library_performance_report_validation` exists to pin.
 */
export function compareReports(baseline: Report, candidate: Report): string[] {
  const problems: string[] = [];

  if (baseline.commit_sha === candidate.commit_sha) {
    problems.push(
      `${LABEL_BASELINE} and ${LABEL_CANDIDATE} name the same commit ${baseline.commit_sha}; a run compared against itself proves nothing`,
    );
  }

  for (const field of METADATA_FIELDS) {
    const before = baseline.metadata[field];
    const after = candidate.metadata[field];
    if (before !== after) {
      problems.push(
        `${WHERE_METADATA}.${field} differs: ${LABEL_BASELINE}=${before} ${LABEL_CANDIDATE}=${after}`,
      );
    }
  }

  const baselineKeys = new Set(baseline.aggregates.map(aggregateKey));
  const candidateKeys = new Set(candidate.aggregates.map(aggregateKey));

  for (const key of baselineKeys) {
    if (!candidateKeys.has(key)) problems.push(`${LABEL_CANDIDATE} is missing aggregate ${key}`);
  }
  for (const key of candidateKeys) {
    if (!baselineKeys.has(key)) problems.push(`${LABEL_CANDIDATE} adds unmatched aggregate ${key}`);
  }

  return problems;
}

function readFlag(argv: string[], flag: string): string | null {
  const index = argv.indexOf(flag);
  if (index === -1 || index + 1 >= argv.length) return null;
  return argv[index + 1] ?? null;
}

async function loadJson(path: string, label: string): Promise<Parsed<unknown>> {
  const file = Bun.file(path);
  if (!(await file.exists())) return fail(`${label} report not found at ${path}`);
  try {
    return { ok: true, value: await file.json() };
  } catch (cause) {
    return fail(`${label} report at ${path} is not valid JSON: ${String(cause)}`);
  }
}

async function main(argv: string[]): Promise<number> {
  if (!argv.includes(FLAG_CHECK)) {
    console.error(
      `usage: bun scripts/report-library-performance.ts ${FLAG_CHECK} ${FLAG_BASELINE} <path> ${FLAG_CANDIDATE} <path>`,
    );
    return EXIT_USAGE;
  }

  const baselinePath = readFlag(argv, FLAG_BASELINE);
  const candidatePath = readFlag(argv, FLAG_CANDIDATE);
  if (baselinePath === null || candidatePath === null) {
    console.error(`${FLAG_BASELINE} and ${FLAG_CANDIDATE} are both required`);
    return EXIT_USAGE;
  }

  const rawBaseline = await loadJson(baselinePath, LABEL_BASELINE);
  if (!rawBaseline.ok) {
    console.error(rawBaseline.error);
    return EXIT_INVALID;
  }
  const rawCandidate = await loadJson(candidatePath, LABEL_CANDIDATE);
  if (!rawCandidate.ok) {
    console.error(rawCandidate.error);
    return EXIT_INVALID;
  }

  const baseline = parseReport(rawBaseline.value, LABEL_BASELINE);
  if (!baseline.ok) {
    console.error(baseline.error);
    return EXIT_INVALID;
  }
  const candidate = parseReport(rawCandidate.value, LABEL_CANDIDATE);
  if (!candidate.ok) {
    console.error(candidate.error);
    return EXIT_INVALID;
  }

  const problems = compareReports(baseline.value, candidate.value);
  if (problems.length > 0) {
    for (const problem of problems) console.error(problem);
    return EXIT_INVALID;
  }

  console.log(OK_LINE);
  return EXIT_OK;
}

if (import.meta.main) {
  process.exit(await main(Bun.argv.slice(2)));
}
