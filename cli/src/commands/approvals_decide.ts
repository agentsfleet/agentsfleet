// `agentsfleet approvals approve|deny` — the write half of the inbox.
//
// Split from approvals.ts because the daemon splits it: reading a gate needs
// `ApprovalRead`, deciding one needs `ApprovalResolve`, and the decision is its
// own path segment carrying its own capability. Keeping the two files apart
// mirrors that boundary and keeps each inside the length cap.
//
// There is no `--decision` flag and no prompt-driven default. The operator
// names the decision as a subcommand or the command does nothing: an approval
// that could be reached by a default is an approval nobody made.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsApprovalDecisionPath } from "../lib/api-paths.ts";
import type { CliError } from "../errors/index.ts";
import { GATE_DECISION, type GateDecision } from "../constants/approvals.ts";
import { requireGateId } from "./approvals.ts";

const METHOD_POST = "POST" as const;

/** The daemon's answer to a decision: what the gate became and who made it. */
interface ResolutionResponse {
  readonly gate_id?: string | null;
  readonly action_id?: string | null;
  readonly outcome?: string | null;
  readonly resolved_at?: number | string | null;
  readonly resolved_by?: string | null;
}

const decideEffect = (
  decision: GateDecision,
  gateIdPositional: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const workspaceId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const gateId = yield* requireGateId(gateIdPositional);

    const res = yield* http.request<ResolutionResponse>({
      path: wsApprovalDecisionPath(workspaceId, gateId, decision),
      method: METHOD_POST,
      body: {},
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }
    // The outcome comes from the daemon rather than being assumed from the verb
    // the operator typed: a gate someone else already decided answers with the
    // decision that actually stands, and printing the typed verb would lie.
    const outcome = res.outcome ?? decision;
    yield* output.success(`Gate ${gateId} ${outcome}.`);
  });

export const approvalsApproveEffectFromArgs = (
  gateId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> => decideEffect(GATE_DECISION.approve, gateId);

export const approvalsDenyEffectFromArgs = (
  gateId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> => decideEffect(GATE_DECISION.deny, gateId);
