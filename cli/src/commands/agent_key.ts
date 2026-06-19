// External Agent Key CLI commands — Effect-shaped.
//
// Manages agt_a API keys issued to LangGraph/CrewAI/Composio agents.
// The raw key is shown once at creation and cannot be retrieved again.
//
// agentsfleet agent-key add    --workspace <ws> --agent <id> --name <name> [--description <desc>]
// agentsfleet agent-key list   --workspace <ws>
// agentsfleet agent-key delete --workspace <ws> <agent_key_id>

import { Effect } from "effect";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import { WORKSPACES_PATH } from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import { ValidationError, type CliError } from "../errors/index.ts";

interface AgentKeyResponse {
  readonly agent_key_id?: string;
  readonly key?: string;
  readonly created_at?: number | string | null;
}

interface AgentRow {
  readonly agent_key_id?: string;
  readonly name?: string;
  readonly description?: string;
  readonly last_used_at?: number | string | null;
}

interface AgentListResponse {
  readonly items?: ReadonlyArray<AgentRow>;
}

export interface AgentAddArgs {
  readonly workspaceId: string | undefined;
  readonly agentId: string | undefined;
  readonly name: string | undefined;
  readonly description: string | undefined;
}

const requireFlag = (
  value: string | undefined,
  detail: string,
  suggestion: string,
): Effect.Effect<string, ValidationError> =>
  value
    ? Effect.succeed(value)
    : Effect.fail(new ValidationError({ detail, suggestion }));

const requireValidId = (
  value: string,
  fieldName: string,
): Effect.Effect<string, ValidationError> => {
  const check = validateRequiredId(value, fieldName);
  if (!check.ok) {
    return Effect.fail(
      new ValidationError({
        detail: check.message,
        suggestion: "pass a valid uuidv7",
      }),
    );
  }
  return Effect.succeed(value);
};

const resolveWorkspaceId = (
  override: string | undefined,
): Effect.Effect<string, CliError, Workspaces> =>
  Effect.gen(function* () {
    if (override) return override;
    const workspaces = yield* Workspaces;
    const state = yield* workspaces.load;
    return yield* requireFlag(
      state.current_workspace_id ?? undefined,
      "agent-key command requires --workspace <id> or an active workspace context",
      "run `agentsfleet workspace use <id>` or pass --workspace <id>",
    );
  });

const agentKeysPath = (workspaceId: string): string =>
  `${WORKSPACES_PATH}${encodeURIComponent(workspaceId)}/agent-keys`;

const agentKeyPath = (workspaceId: string, agentKeyId: string): string =>
  `${WORKSPACES_PATH}${encodeURIComponent(workspaceId)}/agent-keys/${encodeURIComponent(agentKeyId)}`;

export const agentAddEffectFromArgs = (
  args: AgentAddArgs,
): Effect.Effect<
  void,
  CliError,
  Analytics | CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const workspaceId = yield* resolveWorkspaceId(args.workspaceId);
    const agentId = yield* requireFlag(
      args.agentId,
      "agent-key add requires --agent <id>",
      "pass --agent <agent_id>",
    );
    const name = yield* requireFlag(
      args.name,
      "agent-key add requires --name <name>",
      "pass --name <name>",
    );
    const description = args.description ?? "";

    const res = yield* http.request<AgentKeyResponse>({
      path: agentKeysPath(workspaceId),
      method: "POST",
      body: { agent_id: agentId, name, description },
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }
    yield* output.success(`Agent key created: ${res.agent_key_id ?? ""}`);
    yield* output.info("");
    // The shown-once warning belongs on stdout next to the key.
    // Output.warn would route to stderr; surface this as an info line
    // so the integration test (which reads stdout) still sees it.
    yield* output.info("API Key (shown once — store securely):");
    yield* output.info(`  ${res.key ?? ""}`);
    yield* output.info("");
    yield* output.info(`Use as: Authorization: Bearer <key>`);
    yield* output.info(`Authenticated agent: ${agentId}`);
    yield* output.info("");
    yield* output.printTable(
      [
        { key: "label", label: "" },
        { key: "value", label: "" },
      ],
      [
        { label: AGENT_ID_2, value: res.agent_key_id ?? "" },
        { label: "agent_id", value: agentId },
        { label: FIELD_NAME, value: name },
        {
          label: "created_at",
          value: res.created_at ? new Date(res.created_at).toISOString() : "—",
        },
      ],
    );
  });

export const agentListEffectFromArgs = (
  workspaceIdFlag: string | undefined,
): Effect.Effect<
  void,
  CliError,
  Analytics | CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const workspaceId = yield* resolveWorkspaceId(workspaceIdFlag);

    const res = yield* http.request<AgentListResponse>({
      path: agentKeysPath(workspaceId),
      token,
    });
    const agents = res.items ?? [];

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }
    if (agents.length === 0) {
      yield* output.info("no agent keys found");
      return;
    }
    yield* output.printTable(
      [
        { key: FIELD_NAME, label: "NAME" },
        { key: "description", label: "DESCRIPTION" },
        { key: "last_used_at", label: "LAST_USED" },
        { key: AGENT_ID_2, label: "AGENT_KEY_ID" },
      ],
      agents.map((a) => ({
        name: a.name ?? "",
        description: a.description ?? "",
        last_used_at: a.last_used_at
          ? new Date(a.last_used_at).toISOString()
          : "never",
        agent_key_id: a.agent_key_id ?? "",
      })),
    );
  });

export const agentDeleteEffectFromArgs = (
  workspaceIdFlag: string | undefined,
  agentKeyIdPositional: string | undefined,
  agentKeyIdFlag: string | undefined,
): Effect.Effect<
  void,
  CliError,
  Analytics | CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const workspaceId = yield* resolveWorkspaceId(workspaceIdFlag);
    yield* requireValidId(workspaceId, "workspace_id");
    const agentKeyIdRaw = yield* requireFlag(
      agentKeyIdPositional ?? agentKeyIdFlag,
      "agent-key delete requires <agent_key_id>",
      "pass <agent_key_id> as positional or --agent-key-id <id>",
    );
    const agentKeyId = yield* requireValidId(agentKeyIdRaw, "key_id");

    yield* http.request<unknown>({
      path: agentKeyPath(workspaceId, agentKeyId),
      method: "DELETE",
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({ deleted: true, agent_key_id: agentKeyId });
    } else {
      yield* output.success(
        `Agent key ${agentKeyId} deleted. Key immediately invalidated.`,
      );
    }
  });
const AGENT_ID_2 = "agent_key_id" as const;
const FIELD_NAME = "name" as const;
