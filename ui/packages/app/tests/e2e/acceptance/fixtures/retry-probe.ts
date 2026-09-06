/**
 * Drives the dashboard's server-side transport against whatever origin
 * NEXT_PUBLIC_API_URL names, in a process of its own so the origin the
 * transport captures at module load is the one the spec started. Run with
 * `bun run retry-probe.ts <scenario>`; prints one JSON line.
 */
import { request, requestWithRetry } from "@/lib/api/client";

const READ_PATH = "/v1/thing";
const WRITE_PATH = "/v1/writes";
const TOKEN = "acceptance-token";
const SCENARIO = { read: "read", write: "write" } as const;

type Outcome = { settled: "answered" | "failed"; status: number | undefined; name: string | undefined; elapsedMs: number };

async function run(scenario: string): Promise<Outcome> {
  const startedAt = Date.now();
  const call =
    scenario === SCENARIO.write
      ? requestWithRetry(WRITE_PATH, { method: "POST", body: "{}" }, TOKEN, { maxAttempts: 3 })
      : request(READ_PATH, { method: "GET" }, TOKEN);
  return call.then(
    () => ({ settled: "answered", status: undefined, name: undefined, elapsedMs: Date.now() - startedAt }),
    (err: unknown) => ({
      settled: "failed",
      status: (err as { status?: number }).status,
      name: err instanceof Error ? err.constructor.name : undefined,
      elapsedMs: Date.now() - startedAt,
    }),
  );
}

process.stdout.write(`${JSON.stringify(await run(process.argv[2] ?? SCENARIO.read))}\n`);
