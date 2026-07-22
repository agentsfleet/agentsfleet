export type EventFailurePresentation = {
  label: string;
  guidance: "startup" | null;
};

const FAILURE_PRESENTATION: Record<string, EventFailurePresentation> = {
  startup_posture: {
    label: "Failed a startup safety check",
    guidance: "startup",
  },
  policy_deny: { label: "Blocked by fleet policy", guidance: null },
  timeout_kill: { label: "Timed out", guidance: null },
  oom_kill: { label: "Ran out of memory", guidance: null },
  resource_kill: { label: "Hit a resource limit", guidance: null },
  runner_crash: { label: "The runner crashed", guidance: null },
  transport_loss: { label: "Lost connection to the runner", guidance: null },
  landlock_deny: { label: "Blocked by the sandbox policy", guidance: null },
  lease_expired: { label: "The run's lease expired", guidance: null },
  renewal_terminate: { label: "Stopped by lease renewal policy", guidance: null },
  budget_breach: { label: "Fleet budget limit reached", guidance: null },
};

export function presentEventFailure(tag: string): EventFailurePresentation {
  return FAILURE_PRESENTATION[tag] ?? { label: tag, guidance: null };
}
