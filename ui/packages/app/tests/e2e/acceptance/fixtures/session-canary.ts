/**
 * Append-only sink for session-canary observations.
 *
 * The spec observes and writes counts; `scripts/check-session-keeper-canary.ts`
 * grades them. Keeping the two apart matters: a spec that decided its own
 * verdict could quietly move the line it was measuring against, and the whole
 * point of the canary is that the line does not move.
 *
 * One JSON Lines file per capture run, appended rather than rewritten, because
 * three browser projects write concurrently and a read-modify-write would lose
 * whichever lane lost the race. The capture reads the lines back and assembles
 * the report.
 */
import * as fs from "node:fs";
import * as path from "node:path";

export type ScenarioName =
  | "session_lifetime_continuity"
  | "background_expiry"
  | "offline_online"
  | "focus_restoration"
  | "resumed_server_action";

export type CanaryObservation = {
  cohort: string;
  browser: string;
  scenario: ScenarioName;
  completed_attempts: number;
  unexpected_auth_failures: number;
  recovery_required: number;
  recovery_succeeded: number;
  refresh_eligible: number;
  duplicate_refreshes: number;
};

/** Default sink. The capture overrides it per cohort so runs cannot mix. */
const DEFAULT_SINK = "test-results/session-keeper-observations.jsonl";

export function sinkPath(): string {
  return process.env.AGENTSFLEET_CANARY_SINK ?? path.join(process.cwd(), DEFAULT_SINK);
}

/**
 * Append one cell. `appendFileSync` with a single write is atomic enough for
 * lines this small on every platform we run, which is why the format is JSON
 * Lines rather than a JSON array needing a rewrite.
 */
export async function appendCanaryObservation(observation: CanaryObservation): Promise<void> {
  const target = sinkPath();
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.appendFileSync(target, `${JSON.stringify(observation)}\n`, "utf8");
}

/**
 * Count Clerk session-refresh requests the page actually issues.
 *
 * `duplicate_refreshes` is one of the three signals the verdict turns on — it
 * is the cost of keeping a keeper that is no longer needed, visible as BOTH
 * the keeper's timer and Clerk's own SDK refreshing the same session. Reporting
 * it as a hardcoded zero would make the canary claim a measurement it never
 * took, and the checker would compare zero against zero forever.
 *
 * Matches the token/client endpoints clerk-js uses for refresh, on a Clerk
 * host, so ordinary application traffic is not counted.
 */
export function attachRefreshCounter(page: {
  on: (event: "request", handler: (req: { url: () => string }) => void) => void;
}): { count: () => number; reset: () => void } {
  let seen = 0;
  page.on("request", (req) => {
    const url = req.url();
    if (!/clerk/i.test(new URL(url).hostname)) return;
    if (/\/v1\/client(\/|$)|\/tokens(\?|$)|\/touch(\?|$)/.test(url)) seen += 1;
  });
  return { count: () => seen, reset: () => { seen = 0; } };
}

/** Read every observation written so far. Missing file means none yet. */
export function readCanaryObservations(file: string): CanaryObservation[] {
  if (!fs.existsSync(file)) return [];
  return fs
    .readFileSync(file, "utf8")
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as CanaryObservation);
}
