import type {RunnerLiveness, RunnerState} from "./runner-control-types";

export type {RunnerLease, RunnerLeaseResponse} from "./runner-control-leases";
export type {RunnerLiveness} from "./runner-control-types";

/** Fields the execution and preflight journeys observe from the operator list. */
export interface RunnerListItem {
  id: string;
  host_id: string;
  status: RunnerState;
  liveness: RunnerLiveness;
  last_seen_at: string;
}

export interface RunnerListResponse {
  items: RunnerListItem[];
  total: number | null;
  next_cursor: string | null;
}
