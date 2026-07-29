// The one producer of the runner surface's paths. No component inlines the
// route string (Invariant: the route string is written once), so a rename is a
// one-file change and a grep for the literal finds exactly this module.

export const RUNNER_VIEW = {
  leases: "leases",
  activity: "activity",
} as const;

export type RunnerView = (typeof RUNNER_VIEW)[keyof typeof RUNNER_VIEW];

const RUNNERS_BASE_PATH = "/admin/runners";

/** An absent, empty or unknown view resolves to Leases — the page's main object. */
export function resolveRunnerView(value: string | undefined): RunnerView {
  return value === RUNNER_VIEW.activity ? RUNNER_VIEW.activity : RUNNER_VIEW.leases;
}

export function runnersIndexPath(): string {
  return RUNNERS_BASE_PATH;
}

export function runnerPath(runnerId: string, view?: RunnerView): string {
  const base = `${RUNNERS_BASE_PATH}/${runnerId}`;
  return view && view !== RUNNER_VIEW.leases ? `${base}?view=${view}` : base;
}
