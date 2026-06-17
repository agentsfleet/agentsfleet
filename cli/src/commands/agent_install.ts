// Agent install + update — Effects mirroring the imperative leaves
// they replaced. Each reads the workspace-scoped MainLayer services
// (CliConfig + Output + HttpClient + Credentials + Workspaces) and
// emits CliError variants on failure.
//
// `loadSkillFromPath` is sync filesystem IO — wrapped in `Effect.try`
// so the SkillLoadError surfaces on the typed error channel as a
// ConfigError (operator can act on the message).

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsAgentsPath, wsAgentPath } from "../lib/api-paths.ts";
import {
  loadSkillFromPath,
  type LoadedSkill,
  type SkillLoadError,
} from "../lib/load-skill-from-path.ts";
import { validateRequiredId } from "../program/validators.ts";
import { OPT_FROM } from "../constants/cli-flags.ts";
import {
  ConfigError,
  ValidationError,
  type CliError,
} from "../errors/index.ts";

interface InstallResponse {
  readonly agent_id?: string;
  readonly name?: string;
  readonly webhook_urls?: Record<string, string>;
}

interface UpdateResponse {
  readonly config_revision?: number | string | null;
}

const USAGE_INSTALL = "agentsfleet install --from <path>";
const USAGE_UPDATE =
  "agentsfleet agent update <agent_id> --from <path>";

const loadBundle = (
  fromPath: string,
): Effect.Effect<LoadedSkill, ConfigError> =>
  Effect.try({
    try: () => loadSkillFromPath(fromPath),
    catch: (err) => {
      const loadErr = err as SkillLoadError;
      return new ConfigError({
        detail: `${loadErr.code}: ${loadErr.message}`,
        suggestion: "verify the path exists and contains a skill.md + trigger.md",
      });
    },
  });

const requireFromPath = (
  fromPath: string | null | undefined,
  usage: string,
): Effect.Effect<string, ValidationError> => {
  if (typeof fromPath !== "string" || fromPath.length === 0) {
    return Effect.fail(
      new ValidationError({
        detail: "--from <path> is required",
        suggestion: `usage: ${usage}`,
      }),
    );
  }
  return Effect.succeed(fromPath);
};

export const installEffectFromFlags = (
  fromPath: string | null | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;

    const path = yield* requireFromPath(fromPath, USAGE_INSTALL);
    const wsId = yield* requireWorkspaceId;
    const bundle = yield* loadBundle(path);
    const token = yield* resolveAuthToken;

    const res = yield* http.request<InstallResponse>({
      path: wsAgentsPath(wsId),
      method: "POST",
      body: {
        trigger_markdown: bundle.trigger_md,
        source_markdown: bundle.skill_md,
      },
      token,
    });

    const displayName = res.name || bundle.fallback_name;

    if (config.jsonMode) {
      yield* output.printJson({
        status: "installed",
        agent_id: res.agent_id,
        webhook_urls: res.webhook_urls ?? {},
        name: displayName,
      });
      return;
    }

    yield* output.success(`${displayName} is live.`);
    if (res.agent_id) yield* output.info(`  Agent ID: ${res.agent_id}`);
    const urls = res.webhook_urls ?? {};
    const sources = Object.keys(urls);
    if (sources.length > 0) {
      yield* output.info("  Webhook URLs (register on the upstream provider):");
      for (const source of sources) {
        yield* output.info(`    ${source}: ${urls[source]}`);
      }
    }
  });

export const updateEffectFromArgs = (
  agentId: string | undefined,
  fromPath: string | null | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;

    if (!agentId) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "agent_id is required",
          suggestion: `usage: ${USAGE_UPDATE}`,
        }),
      );
    }
    const idCheck = validateRequiredId(agentId, "agent_id");
    if (!idCheck.ok) {
      return yield* Effect.fail(
        new ValidationError({
          detail: idCheck.message,
          suggestion: `usage: ${USAGE_UPDATE}`,
        }),
      );
    }

    const path = yield* requireFromPath(fromPath, USAGE_UPDATE);
    const wsId = yield* requireWorkspaceId;
    const bundle = yield* loadBundle(path);
    const token = yield* resolveAuthToken;

    const res = yield* http.request<UpdateResponse>({
      path: wsAgentPath(wsId, agentId),
      method: "PATCH",
      body: {
        trigger_markdown: bundle.trigger_md,
        source_markdown: bundle.skill_md,
      },
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({
        status: "updated",
        agent_id: agentId,
        config_revision: res.config_revision,
      });
      return;
    }

    yield* output.success(`${agentId} updated.`);
    if (res.config_revision != null) {
      yield* output.info(`  Config revision: ${res.config_revision}`);
    }
  });

export { OPT_FROM };
