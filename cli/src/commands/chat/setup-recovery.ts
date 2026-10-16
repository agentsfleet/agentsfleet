// Recovery metadata preserves the original boundary; private message bytes stay with the caller.

import { OUTPUT_FORMAT, type Output } from "../../services/output.ts";

export const SETUP_ACTION = { save: "save", publish: "publish", apply: "apply" } as const;
type Base = { readonly operation_id: string; readonly draft_candidate_id: string };
export type SetupRecovery = Base & (
  | { readonly action: typeof SETUP_ACTION.save; readonly name: string; readonly review_digest: string; readonly start_kind: string }
  | { readonly action: typeof SETUP_ACTION.publish; readonly label: string; readonly target?: { readonly id: string; readonly etag: string } }
  | { readonly action: typeof SETUP_ACTION.apply; readonly fleet_id: string; readonly review_digest: string; readonly setup_etag: string }
);
export type RecoveryKey = { readonly action: SetupRecovery["action"]; readonly key: string };

export function renderSetupRecovery(output: Output, recovery: SetupRecovery) {
  const metadata = { kind: "setup_operation_recovery", ...recovery };
  return output.format === OUTPUT_FORMAT.json ? output.printJsonErr(metadata)
    : output.warn(`Retry the same action with --operation-id ${recovery.operation_id} --candidate-id ${recovery.draft_candidate_id} and its original arguments. Recovery: ${JSON.stringify(metadata)}`);
}
