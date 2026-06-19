// Agent CLI top-level command Effects — status / stop / resume / kill /
// delete. Install + update live in agent_install.ts; list/logs/events/
// steer/credential leaves live in sibling files (agent_list.ts,
// agent_logs.ts, agent_events.ts, agent_steer.ts, agent_credential.ts).
//
// Each command yields services from the MainLayer (CliConfig, Output,
// HttpClient, Credentials, Workspaces, Analytics) and emits one of the
// CliError variants on failure. The dispatcher's renderError prints the
// detail + suggestion + (for ServerError) request_id.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsAgentsPath, wsAgentPath } from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import {
  AGENTSFLEET_STATUS,
  type AgentMutationStatus,
} from "../constants/agent-status.ts";
import { formatDollars } from "../constants/billing.ts";
import {
  ValidationError,
  type CliError,
} from "../errors/index.ts";

const STATUS_PAST_TENSE: Record<AgentMutationStatus, string> = {
  [AGENTSFLEET_STATUS.STOPPED]: "stopped",
  [AGENTSFLEET_STATUS.ACTIVE]: "resumed",
  [AGENTSFLEET_STATUS.KILLED]: "killed",
};

const STATUS_VERB: Record<AgentMutationStatus, string> = {
  [AGENTSFLEET_STATUS.STOPPED]: "stop",
  [AGENTSFLEET_STATUS.ACTIVE]: "resume",
  [AGENTSFLEET_STATUS.KILLED]: "kill",
};

interface AgentListItem {
  readonly name?: string;
  readonly status?: string;
  readonly events_processed?: number;
  readonly budget_used_nanos?: number | null;
}

interface AgentListResponse {
  readonly items?: ReadonlyArray<AgentListItem>;
}

const requireAgentId = (
  agentId: string | undefined,
  usage: string,
): Effect.Effect<string, ValidationError> =>
  Effect.gen(function* () {
    if (!agentId) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "agent_id is required",
          suggestion: `usage: ${usage}`,
        }),
      );
    }
    const check = validateRequiredId(agentId, "agent_id");
    if (!check.ok) {
      return yield* Effect.fail(
        new ValidationError({ detail: check.message, suggestion: `usage: ${usage}` }),
      );
    }
    return agentId;
  });

export const statusEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const http = yield* HttpClient;
  const wsId = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;

  const res = yield* http.request<AgentListResponse>({
    path: wsAgentsPath(wsId),
    token,
  });

  if (config.jsonMode) {
    yield* output.printJson(res);
    return;
  }

  const agents = res.items ?? [];
  if (agents.length === 0) {
    yield* output.info(
      "No agents running. Install one with: agentsfleet install --from <path>",
    );
    return;
  }

  yield* output.printSection("Agents");
  for (const z of agents) {
    yield* output.printKeyValue({
      Name: z.name ?? "",
      Status: z.status ?? "",
      Events: String(z.events_processed ?? 0),
      Budget: formatDollars(z.budget_used_nanos),
    });
  }
});

const setStatusEffect = (
  agentId: string | undefined,
  status: AgentMutationStatus,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const verb = STATUS_VERB[status];
    const wsId = yield* requireWorkspaceId;
    const id = yield* requireAgentId(agentId, `agentsfleet ${verb} <agent_id>`);
    const token = yield* resolveAuthToken;

    const res = yield* http.request<unknown>({
      path: wsAgentPath(wsId, id),
      method: "PATCH",
      body: { status },
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
    } else {
      yield* output.success(`${id} ${STATUS_PAST_TENSE[status]}.`);
    }
  });

export const stopEffectFromId = (
  agentId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> => setStatusEffect(agentId, AGENTSFLEET_STATUS.STOPPED);

export const resumeEffectFromId = (
  agentId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> => setStatusEffect(agentId, AGENTSFLEET_STATUS.ACTIVE);

export const killEffectFromId = (
  agentId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> => setStatusEffect(agentId, AGENTSFLEET_STATUS.KILLED);

export const deleteEffectFromId = (
  agentId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const wsId = yield* requireWorkspaceId;
    const id = yield* requireAgentId(agentId, "agentsfleet delete <agent_id>");
    const token = yield* resolveAuthToken;

    yield* http.request<unknown>({
      path: wsAgentPath(wsId, id),
      method: "DELETE",
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({ agent_id: id, deleted: true });
    } else {
      yield* output.success(`${id} deleted.`);
    }
  });
