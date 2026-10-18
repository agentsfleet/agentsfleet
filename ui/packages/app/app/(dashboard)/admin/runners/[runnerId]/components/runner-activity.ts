import type { RunnerEventItem } from "@/lib/api/runners";
import { z } from "zod";
import { Instant } from "@/lib/api/decode";
import { NETWORK_POLICY_LABELS, RUNNER_ADMIN_STATE, RUNNER_EVENT_TYPE, RUNNER_LAST_SEEN_NEVER, SANDBOX_TIER_LABELS,
  type RunnerLifecycleEventType } from "@/lib/api/runners-types";

const MetadataSchema = z.object({
  from_status: z.string().optional(), to_status: z.string().optional(),
  host_id: z.string().optional(), sandbox_tier: z.string().optional(),
  network_policy: z.string().optional(), worker_count: z.number().int().positive().optional(),
  last_seen_at: Instant.optional(),
});
const DETAIL_SEPARATOR = " · ";
const STATE_LABELS = {
  [RUNNER_ADMIN_STATE.active]: "Active", [RUNNER_ADMIN_STATE.cordoned]: "New work paused",
  [RUNNER_ADMIN_STATE.draining]: "Draining", [RUNNER_ADMIN_STATE.drained]: "Drained",
  [RUNNER_ADMIN_STATE.revoked]: "Revoked",
};
const COPY: Record<RunnerLifecycleEventType, {activity: string; details: string}> = {
  [RUNNER_EVENT_TYPE.registered]: {activity: "Runner registered", details: "Runner added to the platform."},
  [RUNNER_EVENT_TYPE.online]: {activity: "Runner connected", details: "Connection established."},
  [RUNNER_EVENT_TYPE.offline]: {activity: "Runner stopped responding", details: "No recent heartbeat received."},
  [RUNNER_EVENT_TYPE.cordoned]: {activity: "New work paused", details: "Runner stopped accepting new work."},
  [RUNNER_EVENT_TYPE.draining]: {activity: "Runner started draining", details: "Active work can finish; new work is paused."},
  [RUNNER_EVENT_TYPE.drained]: {activity: "Runner finished draining", details: "All active work finished."},
  [RUNNER_EVENT_TYPE.revoked]: {activity: "Runner access revoked", details: "Runner access was removed."},
  [RUNNER_EVENT_TYPE.tokenRotated]: {activity: "Runner access token replaced", details: "A new access token was issued."},
  [RUNNER_EVENT_TYPE.policyAssigned]: {activity: "Runner policy changed", details: "Runner policy was updated."},
};

export function runnerActivity(item: RunnerEventItem & {event_type: RunnerLifecycleEventType}): {activity: string; details: string} {
  const copy = COPY[item.event_type];
  const parsed = MetadataSchema.safeParse(item.metadata);
  if (!parsed.success) return copy;
  const meta = parsed.data;
  if (item.event_type === RUNNER_EVENT_TYPE.online && meta.last_seen_at !== undefined && Date.parse(meta.last_seen_at) === RUNNER_LAST_SEEN_NEVER) {
    return {...copy, details: "First connection established."};
  }
  if (meta.from_status && meta.to_status) return {...copy,
    details: `${label(STATE_LABELS, meta.from_status)} → ${label(STATE_LABELS, meta.to_status)}`};
  const parts = [
    meta.host_id ? `Host: ${meta.host_id}` : null,
    meta.sandbox_tier ? `Isolation: ${label(SANDBOX_TIER_LABELS, meta.sandbox_tier)}` : null,
    meta.network_policy ? `Network: ${label(NETWORK_POLICY_LABELS, meta.network_policy)}` : null,
    meta.worker_count === undefined ? null : `Workers: ${meta.worker_count}`,
  ].filter((part) => part !== null);
  return {...copy, details: parts.length ? parts.join(DETAIL_SEPARATOR) : copy.details};
}

function label(labels: Readonly<Record<string, string>>, value: string): string {
  const recorded = Object.hasOwn(labels, value) ? labels[value] : undefined;
  return recorded ?? value;
}
