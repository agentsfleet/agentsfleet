/**
 * Connectivity evidence for the soak journeys.
 *
 * A turn that fails while a probe also failed is an outage: it is recorded as a
 * failed turn, never resent, and the journey continues with the next turn. A
 * turn that fails while every probe answered is an application failure and
 * stops the journey. Probing an unrelated host separates this machine's
 * network from api-dev's own availability.
 */
const PROBE_INTERVAL_MS = 3_000;
const PROBE_TIMEOUT_MS = 8_000;
const HEALTHY_STREAK = 3;
const RECOVERY_LIMIT_MS = 15 * 60_000;
const WINDOW_SLACK_MS = 5_000;

type Probe = {at: number; target: "api" | "neutral"; ok: boolean; detail: string};
export type OutageClass = "local-or-transport" | "api-dev-unavailable";
export type Outage = {kind: OutageClass; failedProbes: Probe[]};

class OutageWatch {
  private readonly probes: Probe[] = [];
  private timer: ReturnType<typeof setInterval> | undefined;
  private targets: Record<Probe["target"], string> = {api: "", neutral: ""};

  start(api: string, neutral: string): void {
    if (this.timer) return;
    this.targets = {api: `${api}/readyz`, neutral};
    this.timer = setInterval(() => void this.sample(), PROBE_INTERVAL_MS);
    this.timer.unref?.();
  }

  stop(): void { clearInterval(this.timer); this.timer = undefined; }

  private async probe(target: Probe["target"]): Promise<Probe> {
    const at = Date.now();
    try {
      const response = await fetch(this.targets[target], {signal: AbortSignal.timeout(PROBE_TIMEOUT_MS), redirect: "manual"});
      // Any HTTP answer proves the path; only api-dev must also be ready.
      const ok = target === "neutral" || response.ok;
      return {at, target, ok, detail: String(response.status)};
    } catch (error) {
      return {at, target, ok: false, detail: String(error)};
    }
  }

  private async sample(): Promise<Probe[]> {
    const results = await Promise.all([this.probe("api"), this.probe("neutral")]);
    this.probes.push(...results);
    for (const probe of results) if (!probe.ok) console.error(JSON.stringify({soakProbeFailed: {...probe, at: new Date(probe.at).toISOString()}}));
    return results;
  }

  /** The outage that overlapped [from, to], if any probe failed inside it. */
  during(from: number, to: number): Outage | null {
    const failed = this.probes.filter(probe => !probe.ok && probe.at >= from - WINDOW_SLACK_MS && probe.at <= to + WINDOW_SLACK_MS);
    if (!failed.length) return null;
    const kind = failed.some(probe => probe.target === "neutral") ? "local-or-transport" : "api-dev-unavailable";
    return {kind, failedProbes: failed};
  }

  /** Probe until both targets answer several times in a row; a lasting outage ends the run. */
  async waitHealthy(): Promise<void> {
    const deadline = Date.now() + RECOVERY_LIMIT_MS;
    let streak = 0;
    while (streak < HEALTHY_STREAK) {
      if (Date.now() > deadline) throw new Error("Connectivity did not recover within the outage limit");
      const results = await this.sample();
      streak = results.every(result => result.ok) ? streak + 1 : 0;
      await new Promise(resolve => setTimeout(resolve, PROBE_INTERVAL_MS));
    }
  }
}

export const outageWatch = new OutageWatch();

/** Every failed turn the run recorded and moved past, for the final report. */
export const outageLedger: Record<string, unknown>[] = [];

/** Consecutive failed turns after which a journey stops and the supervisor moves it on. */
export const MAX_CONSECUTIVE_FAILURES = 5;

/** A failed turn no outage explains: recorded and moved past, but the run still fails. */
export const APPLICATION_FAILURE = "application-provider-or-unknown";

export function classify(outage: Outage | null): string {
  return outage ? outage.kind : APPLICATION_FAILURE;
}

/** Keep the screen as the failure left it, before the page is reloaded. */
export async function captureFailure(page: import("@playwright/test").Page, label: string): Promise<string | null> {
  const dir = process.env.SOAK_CHECKPOINT_DIR;
  if (!dir) return null;
  const file = `${dir}/failure-${label}-${Date.now()}.png`;
  await page.screenshot({path: file, fullPage: true}).catch(() => undefined);
  return file;
}

type Page = import("@playwright/test").Page;
type Request = import("@playwright/test").Request;
const STALLED_AFTER_MS = 5_000;
const MAX_PENDING_LISTED = 20;
const inFlight = new WeakMap<Page, Map<Request, number>>();

/** Watches `page`'s requests, so a stalled step can name what it was waiting on. */
export function trackRequests(page: Page): void {
  if (inFlight.has(page)) return;
  const open = new Map<Request, number>();
  inFlight.set(page, open);
  page.on("request", request => open.set(request, Date.now()));
  page.on("requestfinished", request => open.delete(request));
  page.on("requestfailed", request => open.delete(request));
}

/** Requests `page` started and has not finished after a few seconds, oldest first. Streams stay open by design and are labelled by type. */
export function pendingRequests(page: Page): {method: string; url: string; type: string; waitedMs: number}[] {
  const now = Date.now();
  return [...(inFlight.get(page) ?? new Map<Request, number>()).entries()]
    .filter(([, started]) => now - started >= STALLED_AFTER_MS)
    .sort(([, a], [, b]) => a - b)
    .slice(0, MAX_PENDING_LISTED)
    .map(([request, started]) => ({method: request.method(), url: request.url().split("?")[0]!,
      type: request.resourceType(), waitedMs: now - started}));
}
